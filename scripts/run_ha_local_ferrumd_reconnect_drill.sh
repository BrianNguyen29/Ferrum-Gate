#!/usr/bin/env bash
# run_ha_local_ferrumd_reconnect_drill.sh
# Local HA ferrumd reconnect drill.
#
# Verifies: ferrumd starts against primary DSN, readyz passes, primary stopped,
# standby promoted, ferrumd restarted against standby DSN, readyz passes,
# lightweight smoke request passes, app-level RTO measured.
#
# Boundary: local-only, manual/optional, no production HA claim.
# This is a local Docker simulation, not evidence of production readiness.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.ha-local.yml"
PROJECT_NAME="ferrumgate_ha_local"
PRIMARY_CONTAINER="ferrumgate_postgres_ha_primary"
STANDBY_CONTAINER="ferrumgate_postgres_ha_standby"

PASS=0
FAIL=0
SKIP=0

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }
skip() { echo "[SKIP] $1"; SKIP=$((SKIP + 1)); }

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_ha_local_ferrumd_reconnect_drill.sh

Runs a local ferrumd reconnect drill against the HA primary/standby simulation:
  1. Ensures HA local setup is running (or invokes setup).
  2. Builds ferrumd if needed.
  3. Starts ferrumd against primary DSN.
  4. Verifies readyz/deep.
  5. Stops primary container (failure injection).
  6. Promotes standby to writable primary.
  7. Restarts ferrumd with standby DSN.
  8. Verifies readyz/deep and lightweight smoke request.
  9. Measures app-level RTO.
  10. Teardown or leaves cleanup controlled.

Boundary: local-only; manual/optional; not executed in CI.
EOF
    exit 0
fi

echo ""
echo "========================================"
echo "HA Local ferrumd Reconnect Drill — Preflight"
echo "========================================"
echo ""

if command -v docker >/dev/null 2>&1; then
    pass "docker is available"
else
    fail "docker is not available"
    exit 1
fi

if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose -p "$PROJECT_NAME")
elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose -p "$PROJECT_NAME")
else
    fail "docker compose is not available"
    exit 1
fi

if command -v cargo >/dev/null 2>&1; then
    pass "cargo is available"
else
    fail "cargo is not available"
    exit 1
fi

if command -v curl >/dev/null 2>&1; then
    pass "curl is available"
else
    fail "curl is not available"
    exit 1
fi

# Ensure HA setup is running
BOTH_UP=true
for c in "$PRIMARY_CONTAINER" "$STANDBY_CONTAINER"; do
    if ! docker inspect "$c" >/dev/null 2>&1; then
        BOTH_UP=false
        break
    fi
    STATUS=$(docker inspect --format='{{.State.Status}}' "$c" 2>/dev/null || echo "unknown")
    if [[ "$STATUS" != "running" ]]; then
        BOTH_UP=false
        break
    fi
done

if [[ "$BOTH_UP" == "true" ]]; then
    pass "HA local simulation already running"
else
    echo "[INFO] HA local simulation not running; invoking setup..."
    bash "$REPO_ROOT/scripts/setup_ha_local.sh"
    pass "HA local setup completed"
fi

# Helper to run psql via docker exec with SQL passed through env
psql_standby_at() {
    docker exec -e FERRUM_SQL="$1" "$STANDBY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

# Verify standby is still in recovery before we start
IN_RECOVERY=$(psql_standby_at "SELECT pg_is_in_recovery();" || echo "unknown")
if [[ "$IN_RECOVERY" == "t" ]]; then
    pass "standby is in recovery mode before drill"
else
    fail "standby is not in recovery mode before drill (got '$IN_RECOVERY')"
    exit 1
fi

echo ""
echo "========================================"
echo "Building ferrumd"
echo "========================================"
echo ""

if [[ ! -x "$REPO_ROOT/target/debug/ferrumd" ]]; then
    cargo build --features postgres --package ferrumd
fi
if [[ -x "$REPO_ROOT/target/debug/ferrumd" ]]; then
    FERRUMD_BIN="$REPO_ROOT/target/debug/ferrumd"
    pass "ferrumd binary is available"
else
    fail "ferrumd binary is unavailable after build"
    exit 1
fi

FERRUMD_PID=""
FERRUMD_LOG="$(mktemp)"

PRIMARY_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5433/ferrumgate_ha_test"
STANDBY_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5434/ferrumgate_ha_test"
FERRUMD_BIND_ADDR="127.0.0.1:19089"

cleanup_ferrumd() {
    if [[ -n "${FERRUMD_PID:-}" ]] && kill -0 "$FERRUMD_PID" 2>/dev/null; then
        echo "[INFO] Stopping ferrumd (PID $FERRUMD_PID)..."
        kill "$FERRUMD_PID" 2>/dev/null || true
        wait "$FERRUMD_PID" 2>/dev/null || true
    fi
    rm -f "$FERRUMD_LOG"
}

echo ""
echo "========================================"
echo "Starting ferrumd against PRIMARY DSN"
echo "========================================"
echo ""

export FERRUMD_STORE_DSN="$PRIMARY_DSN"
export FERRUMD_BIND_ADDR="$FERRUMD_BIND_ADDR"
export FERRUMD_AUTH_MODE="disabled"
export FERRUMD_LOG_FILTER="info"

nohup "$FERRUMD_BIN" >"$FERRUMD_LOG" 2>&1 &
FERRUMD_PID=$!
echo "[INFO] ferrumd started (PID $FERRUMD_PID), binding to $FERRUMD_BIND_ADDR"

READY=false
for _ in {1..30}; do
    if curl -sf "http://$FERRUMD_BIND_ADDR/v1/readyz/deep" >/dev/null 2>&1; then
        READY=true
        break
    fi
    sleep 1
done

if [[ "$READY" == "true" ]]; then
    pass "ferrumd readyz/deep returned 200 against primary DSN"
else
    fail "ferrumd did not become ready within 30s against primary"
    if [[ -f "$FERRUMD_LOG" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$FERRUMD_LOG" || true
    fi
    cleanup_ferrumd
    exit 1
fi

echo ""
echo "========================================"
echo "Injecting failure: stopping primary"
echo "========================================"
echo ""

FAILOVER_START=$(date +%s)
docker stop "$PRIMARY_CONTAINER" >/dev/null 2>&1 || true
pass "primary container stopped (failure injected)"

echo ""
echo "========================================"
echo "Promoting standby"
echo "========================================"
echo ""

psql_standby_at "SELECT pg_promote();"
pass "standby promotion command issued (pg_promote)"

echo "[INFO] Waiting for standby to exit recovery mode..."
PROMOTED=false
for _ in {1..30}; do
    IN_RECOVERY=$(psql_standby_at "SELECT pg_is_in_recovery();" || echo "t")
    if [[ "$IN_RECOVERY" == "f" ]]; then
        PROMOTED=true
        break
    fi
    sleep 1
done

if [[ "$PROMOTED" == "true" ]]; then
    pass "standby promoted and no longer in recovery mode"
else
    fail "standby did not exit recovery mode within 30s"
    cleanup_ferrumd
    exit 1
fi

echo ""
echo "========================================"
echo "Restarting ferrumd against STANDBY DSN"
echo "========================================"
echo ""

cleanup_ferrumd

export FERRUMD_STORE_DSN="$STANDBY_DSN"
export FERRUMD_BIND_ADDR="$FERRUMD_BIND_ADDR"
export FERRUMD_AUTH_MODE="disabled"
export FERRUMD_LOG_FILTER="info"

nohup "$FERRUMD_BIN" >"$FERRUMD_LOG" 2>&1 &
FERRUMD_PID=$!
echo "[INFO] ferrumd restarted (PID $FERRUMD_PID) against standby DSN"

RECONNECTED=false
for _ in {1..30}; do
    if curl -sf "http://$FERRUMD_BIND_ADDR/v1/readyz/deep" >/dev/null 2>&1; then
        RECONNECTED=true
        break
    fi
    sleep 1
done

FAILOVER_END=$(date +%s)
RTO_SECONDS=$((FAILOVER_END - FAILOVER_START))

if [[ "$RECONNECTED" == "true" ]]; then
    pass "ferrumd readyz/deep returned 200 after reconnect to standby DSN"
else
    fail "ferrumd did not become ready within 30s after reconnect to standby"
    if [[ -f "$FERRUMD_LOG" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$FERRUMD_LOG" || true
    fi
    cleanup_ferrumd
    exit 1
fi

echo ""
echo "========================================"
echo "Lightweight smoke request against reconnected ferrumd"
echo "========================================"
echo ""

SMOKE_STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "http://$FERRUMD_BIND_ADDR/v1/healthz" || true)
if [[ "$SMOKE_STATUS" == "200" ]]; then
    pass "lightweight smoke request (/v1/healthz) returned 200 after reconnect"
else
    fail "lightweight smoke request returned ${SMOKE_STATUS:-<failed>} after reconnect"
    cleanup_ferrumd
    exit 1
fi

pass "app-level RTO measured: ${RTO_SECONDS}s (primary stop to ferrumd ready on standby)"

echo ""
echo "========================================"
echo "HA LOCAL FERRUMD RECONNECT DRILL SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo "RTO:     ${RTO_SECONDS}s"
echo ""
echo "Boundary: local-only, Docker Compose, manual/optional."
echo "No production HA claim. No automated failover. No multi-host."
echo "This drill is a simulation for procedure rehearsal, not evidence of production readiness."
echo ""

cleanup_ferrumd

if [[ $FAIL -eq 0 ]]; then
    echo "HA LOCAL FERRUMD RECONNECT DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "HA LOCAL FERRUMD RECONNECT DRILL: SOME CHECKS FAILED"
    exit 1
fi
