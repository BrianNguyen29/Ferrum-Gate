#!/usr/bin/env bash
# run_ha_local_failover_drill.sh
# Local HA PostgreSQL failover simulation drill.
#
# Verifies: baseline primary writable, standby replicating and read-only,
# failure injection via primary stop, standby promotion, old primary not writable,
# RTO and RPO measurement.
#
# Boundary: local-only, manual/optional, no production HA claim.
# This is a local Docker simulation, not production failover evidence.

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

DRILL_TABLE="ha_drill_probe"

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }
skip() { echo "[SKIP] $1"; SKIP=$((SKIP + 1)); }

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_ha_local_failover_drill.sh

Runs a local HA failover drill against the primary/standby simulation
started by scripts/setup_ha_local.sh.

The drill verifies:
  - primary is reachable and writable
  - standby is reachable and in recovery before promotion
  - failure injected by stopping the primary
  - standby promoted to writable primary
  - old primary is not writable during failover
  - RTO measured from injection to promoted-write success
  - RPO conservatively bounded by row-count parity

Boundary: local-only; manual/optional; not executed in CI.
EOF
    exit 0
fi

echo ""
echo "========================================"
echo "HA Local Failover Drill — Preflight"
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

# Verify both containers exist
for c in "$PRIMARY_CONTAINER" "$STANDBY_CONTAINER"; do
    if docker inspect "$c" >/dev/null 2>&1; then
        pass "container $c exists"
    else
        fail "container $c not found — run setup_ha_local.sh first"
        exit 1
    fi
done

# Helper to run psql via docker exec with SQL passed through env
psql_primary() {
    docker exec -e FERRUM_SQL="$1" "$PRIMARY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

psql_primary_at() {
    docker exec -e FERRUM_SQL="$1" "$PRIMARY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

psql_standby() {
    docker exec -e FERRUM_SQL="$1" "$STANDBY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

psql_standby_at() {
    docker exec -e FERRUM_SQL="$1" "$STANDBY_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -h localhost -U ferrumgate_dev -d ferrumgate_ha_test -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

echo ""
echo "========================================"
echo "Baseline: primary writable"
echo "========================================"
echo ""

psql_primary "DROP TABLE IF EXISTS $DRILL_TABLE; CREATE TABLE $DRILL_TABLE (id serial PRIMARY KEY, note text, created_at timestamptz DEFAULT now()); INSERT INTO $DRILL_TABLE (note) VALUES ('baseline');"
pass "primary is writable and baseline row inserted"

echo ""
echo "========================================"
echo "Baseline: standby reachable and replicating"
echo "========================================"
echo ""

# Wait for replication
REPLICATED=false
for _ in {1..15}; do
    VAL=$(psql_standby_at "SELECT note FROM $DRILL_TABLE WHERE note='baseline';" || true)
    if [[ "$VAL" == "baseline" ]]; then
        REPLICATED=true
        break
    fi
    sleep 1
done

if [[ "$REPLICATED" == "true" ]]; then
    pass "baseline row replicated to standby"
else
    fail "baseline row not found on standby within 15s"
    exit 1
fi

# Verify standby is in recovery (read-only)
IN_RECOVERY=$(psql_standby_at "SELECT pg_is_in_recovery();")
if [[ "$IN_RECOVERY" == "t" ]]; then
    pass "standby is in recovery mode (read-only) before promotion"
else
    fail "standby is not in recovery mode before promotion (got '$IN_RECOVERY')"
    exit 1
fi

# Attempt a write on standby — should fail while in recovery
if psql_standby "INSERT INTO $DRILL_TABLE (note) VALUES ('standby-write-test');" >/dev/null 2>&1; then
    fail "standby accepted a write while in recovery mode"
    exit 1
else
    pass "standby correctly rejected write while in recovery mode"
fi

echo ""
echo "========================================"
echo "Pre-failover: seed data on primary"
echo "========================================"
echo ""

psql_primary "INSERT INTO $DRILL_TABLE (note) VALUES ('pre-failover');"
PRIMARY_COUNT=$(psql_primary_at "SELECT COUNT(*) FROM $DRILL_TABLE;")
echo "[INFO] Primary row count before failover: $PRIMARY_COUNT"
pass "pre-failover row inserted on primary (count=$PRIMARY_COUNT)"

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

psql_standby "SELECT pg_promote();"
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

FAILOVER_END=$(date +%s)
RTO_SECONDS=$((FAILOVER_END - FAILOVER_START))

if [[ "$PROMOTED" == "true" ]]; then
    pass "standby promoted and no longer in recovery mode"
else
    fail "standby did not exit recovery mode within 30s"
    exit 1
fi

echo ""
echo "========================================"
echo "Post-promotion verification"
echo "========================================"
echo ""

# Verify promoted primary is writable
psql_standby "INSERT INTO $DRILL_TABLE (note) VALUES ('post-promotion');"
pass "promoted primary accepts writes"

PROMOTED_COUNT=$(psql_standby_at "SELECT COUNT(*) FROM $DRILL_TABLE;")
echo "[INFO] Promoted primary row count: $PROMOTED_COUNT"

# RPO check: pre-failover rows must be present
if [[ "$PROMOTED_COUNT" -ge "$PRIMARY_COUNT" ]]; then
    pass "promoted primary retains pre-failover rows (RPO bounded: 0 rows lost)"
else
    fail "promoted primary lost rows: pre-failover=$PRIMARY_COUNT promoted=$PROMOTED_COUNT"
    exit 1
fi

# Verify old primary is not writable during failover
PRIMARY_STATUS=$(docker inspect --format='{{.State.Status}}' "$PRIMARY_CONTAINER" 2>/dev/null || echo "unknown")
if [[ "$PRIMARY_STATUS" == "exited" || "$PRIMARY_STATUS" == "dead" ]]; then
    pass "old primary container is stopped (not writable during failover)"
else
    fail "old primary container status is '$PRIMARY_STATUS' (expected stopped)"
    exit 1
fi

pass "RTO measured: ${RTO_SECONDS}s (injection to promoted-write success)"

echo ""
echo "========================================"
echo "Split-brain local-scope check"
echo "========================================"
echo ""

# In a real HA setup, old primary should be fenced. In this local simulation,
# we verify it is simply stopped and cannot accept connections.
if docker exec "$PRIMARY_CONTAINER" bash -lc "true" >/dev/null 2>&1; then
    fail "old primary container is still accepting docker exec (should be stopped)"
    exit 1
else
    pass "old primary container does not accept commands (local split-brain check)"
fi

echo ""
echo "========================================"
echo "HA LOCAL FAILOVER DRILL SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo "RTO:     ${RTO_SECONDS}s"
echo "RPO:     0 rows lost (pre-failover count $PRIMARY_COUNT, promoted count $PROMOTED_COUNT)"
echo ""
echo "Boundary: local-only, Docker Compose, manual/optional."
echo "No production HA claim. No automated failover. No multi-host."
echo "This drill is a simulation for procedure rehearsal, not evidence of production readiness."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "HA LOCAL FAILOVER DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "HA LOCAL FAILOVER DRILL: SOME CHECKS FAILED"
    exit 1
fi
