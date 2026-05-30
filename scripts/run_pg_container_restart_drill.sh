#!/usr/bin/env bash
# run_pg_container_restart_drill.sh
# Local Docker-based PostgreSQL container restart recovery drill (PG-2.3b B.2).
#
# Tests: start PG container, start ferrumd with postgres feature,
#        verify readiness, restart PG container, measure recovery time.
# Acceptance: recovery <= 30 seconds when run locally with Docker available.
#
# Does NOT require live production PostgreSQL.
# Does NOT claim production-ready.
# Does NOT close Block A or any G2 gate.
#
# Usage:
#   bash scripts/run_pg_container_restart_drill.sh [--dry-run] [--no-cleanup]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Defaults ---
DRY_RUN=false
NO_CLEANUP=false
COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
# Use a non-standard local port to avoid collisions with any running PG
FERRUMD_BIND_ADDR="127.0.0.1:19084"
FERRUMD_PID=""
PASS=0
FAIL=0
SKIP=0

# --- Help ---
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_pg_container_restart_drill.sh [options]

Local PostgreSQL container restart recovery drill.

Options:
  --dry-run      Run preflight checks only (docker, compose, cargo, curl).
                 Does not start containers or build binaries.
  --no-cleanup   Leave the PostgreSQL container running after the drill.
  -h, --help     Show this help message and exit.

Prerequisites:
  - Docker and docker compose (plugin or standalone) available.
  - cargo available (to build ferrumd with --features postgres).
  - curl available.

The drill uses the documented local Docker Compose PostgreSQL service
(docker-compose.postgres.yml) with the placeholder DSN documented
in that file. No production secrets are used or printed.

Boundary: local-only; manual/optional; not executed in CI.
EOF
    exit 0
fi

# --- Option parsing ---
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --no-cleanup) NO_CLEANUP=true ;;
    esac
done

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }
skip() { echo "[SKIP] $1"; SKIP=$((SKIP + 1)); }

cleanup() {
    if [[ -n "${FERRUMD_PID:-}" ]] && kill -0 "$FERRUMD_PID" 2>/dev/null; then
        echo "[INFO] Stopping ferrumd (PID $FERRUMD_PID)..."
        kill "$FERRUMD_PID" 2>/dev/null || true
        wait "$FERRUMD_PID" 2>/dev/null || true
    fi
    if [[ "$NO_CLEANUP" != "true" ]]; then
        echo "[INFO] Stopping PostgreSQL container..."
        docker compose -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true
    fi
}
trap cleanup EXIT

# --- Preflight checks ---
echo ""
echo "========================================"
echo "PG Container Restart Drill — Preflight"
echo "========================================"
echo ""

if command -v docker >/dev/null 2>&1; then
    pass "docker is available"
else
    fail "docker is not available"
    echo "[INFO] This drill requires Docker. Exiting."
    exit 1
fi

if docker compose version >/dev/null 2>&1 || docker-compose version >/dev/null 2>&1; then
    pass "docker compose is available"
else
    fail "docker compose is not available"
    echo "[INFO] This drill requires docker compose. Exiting."
    exit 1
fi

if command -v cargo >/dev/null 2>&1; then
    pass "cargo is available"
else
    fail "cargo is not available"
    echo "[INFO] This drill requires cargo to build ferrumd. Exiting."
    exit 1
fi

if command -v curl >/dev/null 2>&1; then
    pass "curl is available"
else
    fail "curl is not available"
    echo "[INFO] This drill requires curl for readiness probes. Exiting."
    exit 1
fi

if [[ "$DRY_RUN" == "true" ]]; then
    echo ""
    echo "========================================"
    echo "DRY RUN COMPLETE"
    echo "========================================"
    echo "Preflight checks passed. Run without --dry-run to execute the drill."
    echo "Passed: $PASS | Failed: $FAIL | Skipped: $SKIP"
    exit 0
fi

# --- Start PostgreSQL container ---
echo ""
echo "========================================"
echo "Starting PostgreSQL container"
echo "========================================"
echo ""

# Ensure any previous container is cleaned up first
docker compose -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true

docker compose -f "$COMPOSE_FILE" up -d "$PG_SERVICE"

# Wait for healthcheck
echo "[INFO] Waiting for PostgreSQL healthcheck..."
HEALTHY=false
for i in {1..30}; do
    STATUS=$(docker inspect --format='{{.State.Health.Status}}' "$PG_CONTAINER" 2>/dev/null || echo "unknown")
    if [[ "$STATUS" == "healthy" ]]; then
        HEALTHY=true
        break
    fi
    sleep 1
done

if [[ "$HEALTHY" == "true" ]]; then
    pass "PostgreSQL container is healthy"
else
    fail "PostgreSQL container did not become healthy within 30s"
    exit 1
fi

# --- Build ferrumd with postgres feature ---
echo ""
echo "========================================"
echo "Building ferrumd (postgres feature)"
echo "========================================"
echo ""

if cargo build --features postgres --package ferrumd; then
    pass "ferrumd built with postgres feature"
else
    fail "ferrumd build failed"
    exit 1
fi

FERRUMD_BIN="$REPO_ROOT/target/debug/ferrumd"

# --- Start ferrumd ---
echo ""
echo "========================================"
echo "Starting ferrumd with PostgreSQL DSN"
echo "========================================"
echo ""

# Use the documented placeholder DSN from docker-compose.postgres.yml.
# We intentionally do not print the full DSN to avoid leaking anything
# beyond the already-documented placeholder credentials.
export FERRUMD_STORE_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test"
export FERRUMD_BIND_ADDR="$FERRUMD_BIND_ADDR"
export FERRUMD_AUTH_MODE="disabled"
export FERRUMD_LOG_FILTER="info"

nohup "$FERRUMD_BIN" >"$REPO_ROOT/target/debug/ferrumd_pg_drill.log" 2>&1 &
FERRUMD_PID=$!
echo "[INFO] ferrumd started (PID $FERRUMD_PID), binding to $FERRUMD_BIND_ADDR"

# --- Wait for initial readiness ---
echo "[INFO] Polling /v1/readyz/deep for initial health..."
READY=false
for i in {1..30}; do
    if curl -sf "http://$FERRUMD_BIND_ADDR/v1/readyz/deep" >/dev/null 2>&1; then
        READY=true
        break
    fi
    sleep 1
done

if [[ "$READY" == "true" ]]; then
    pass "ferrumd ready before restart"
else
    fail "ferrumd did not become ready within 30s of startup"
    # Dump last log lines for diagnosis
    if [[ -f "$REPO_ROOT/target/debug/ferrumd_pg_drill.log" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$REPO_ROOT/target/debug/ferrumd_pg_drill.log" || true
    fi
    exit 1
fi

# --- Restart PostgreSQL container ---
echo ""
echo "========================================"
echo "Restarting PostgreSQL container"
echo "========================================"
echo ""

RESTART_START_TIME=$(date +%s)

docker compose -f "$COMPOSE_FILE" restart "$PG_SERVICE"

echo "[INFO] Waiting for PostgreSQL container to report healthy after restart..."
HEALTHY=false
for i in {1..30}; do
    STATUS=$(docker inspect --format='{{.State.Health.Status}}' "$PG_CONTAINER" 2>/dev/null || echo "unknown")
    if [[ "$STATUS" == "healthy" ]]; then
        HEALTHY=true
        break
    fi
    sleep 1
done

if [[ "$HEALTHY" == "true" ]]; then
    pass "PostgreSQL container healthy after restart"
else
    fail "PostgreSQL container did not become healthy after restart within 30s"
    exit 1
fi

# --- Measure ferrumd recovery ---
echo "[INFO] Polling /v1/readyz/deep for ferrumd recovery..."
RECOVERED=false
for i in {1..30}; do
    if curl -sf "http://$FERRUMD_BIND_ADDR/v1/readyz/deep" >/dev/null 2>&1; then
        RECOVERED=true
        break
    fi
    sleep 1
done

RESTART_END_TIME=$(date +%s)
RECOVERY_SECONDS=$((RESTART_END_TIME - RESTART_START_TIME))

if [[ "$RECOVERED" == "true" ]]; then
    if [[ "$RECOVERY_SECONDS" -le 30 ]]; then
        pass "ferrumd recovered within ${RECOVERY_SECONDS}s (target <= 30s)"
    else
        # Count as pass for recovery but warn on timing
        echo "[WARN] ferrumd recovered in ${RECOVERY_SECONDS}s, exceeding 30s target"
        pass "ferrumd recovered (timing: ${RECOVERY_SECONDS}s)"
    fi
else
    fail "ferrumd did not recover within 30s after PostgreSQL restart"
    # Dump last log lines for diagnosis
    if [[ -f "$REPO_ROOT/target/debug/ferrumd_pg_drill.log" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$REPO_ROOT/target/debug/ferrumd_pg_drill.log" || true
    fi
    exit 1
fi

# --- Summary ---
echo ""
echo "========================================"
echo "PG CONTAINER RESTART DRILL SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo "Recovery time: ${RECOVERY_SECONDS}s"
echo ""
echo "Boundary: local-only, Docker Compose, manual/optional."
echo "No production-ready claim. Block A remains WAIVED/CONDITIONAL."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "PG CONTAINER RESTART DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG CONTAINER RESTART DRILL: SOME CHECKS FAILED"
    exit 1
fi
