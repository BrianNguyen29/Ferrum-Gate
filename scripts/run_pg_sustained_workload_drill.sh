#!/usr/bin/env bash
# run_pg_sustained_workload_drill.sh
# Local Docker-based PostgreSQL sustained workload drill.
#
# Boundary: local-only, manual/optional, no production-ready claim.
# This drill starts a local Docker PostgreSQL, seeds a deterministic SQLite
# fixture, migrates it into PostgreSQL, starts ferrumd against PG, runs a
# short sustained request workload, and verifies readiness + PG pool metrics.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
FERRUMD_BIND_ADDR="127.0.0.1:19088"
PG_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test"
NO_CLEANUP=false
FERRUMD_PID=""
PASS=0
FAIL=0
SKIP=0

DRILL_DIR="$(mktemp -d)"
FIXTURE_DB="$DRILL_DIR/populated_fixture.db"
FIXTURE_SUMMARY="$DRILL_DIR/fixture_summary.json"
MIGRATE_REPORT="$DRILL_DIR/migration_report.json"
FERRUMD_LOG="$DRILL_DIR/ferrumd.log"
WORKLOAD_OUT="$DRILL_DIR/workload"

# Short default workload; override with env for longer runs
DEFAULT_PHASES='[{"name":"sustained","duration_sec":30,"rate_rps":1.0}]'
SUSTAINED_PHASES="${SUSTAINED_PHASES:-$DEFAULT_PHASES}"

# Exclude http (external) and git (needs repo init) for offline safety
DEFAULT_ADAPTER_MIX='{"fs":{"weight":34,"intent_type":"FileWrite","tool_name":"fs_write"},"sqlite":{"weight":33,"intent_type":"SqliteMutation","tool_name":"sql_mutate"},"maildraft":{"weight":33,"intent_type":"MailDraftCreate","tool_name":"maildraft_create"}}'
SUSTAINED_ADAPTER_MIX="${SUSTAINED_ADAPTER_MIX:-$DEFAULT_ADAPTER_MIX}"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_pg_sustained_workload_drill.sh [--no-cleanup]

Runs a local PostgreSQL sustained workload drill using Docker Compose,
ferrum-migrate, ferrumd, and the real workload generator with very short
phases.

Options:
  --no-cleanup   Leave the PostgreSQL container and temp drill directory behind.
  -h, --help     Show this help and exit.

Environment:
  SUSTAINED_PHASES       JSON phases list (default: 30s @ 1 rps)
  SUSTAINED_ADAPTER_MIX  JSON adapter mix (default: fs/sqlite/maildraft)

Boundary: local-only, manual/optional, not executed in CI.
EOF
    exit 0
fi

for arg in "$@"; do
    case "$arg" in
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
    if [[ "$NO_CLEANUP" == "true" ]]; then
        echo "[INFO] Retained PostgreSQL container and drill directory: $DRILL_DIR"
        return
    fi
    if declare -p COMPOSE_CMD >/dev/null 2>&1; then
        "${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true
    fi
    rm -rf "$DRILL_DIR"
}
trap cleanup EXIT

echo ""
echo "========================================"
echo "PG Sustained Workload Drill — Preflight"
echo "========================================"
echo ""

if command -v docker >/dev/null 2>&1; then
    pass "docker is available"
else
    fail "docker is not available"
    exit 1
fi

if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose)
    pass "docker compose plugin is available"
elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose)
    pass "docker-compose is available"
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

if command -v python3 >/dev/null 2>&1; then
    pass "python3 is available"
else
    fail "python3 is not available"
    exit 1
fi

if command -v curl >/dev/null 2>&1; then
    pass "curl is available"
else
    fail "curl is not available"
    exit 1
fi

echo ""
echo "========================================"
echo "Starting PostgreSQL container"
echo "========================================"
echo ""

"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" down --volumes 2>/dev/null || true
"${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" up -d "$PG_SERVICE"

echo "[INFO] Waiting for PostgreSQL healthcheck..."
HEALTHY=false
for _ in {1..30}; do
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

echo ""
echo "========================================"
echo "Building drill binaries"
echo "========================================"
echo ""

if [[ ! -x "$REPO_ROOT/target/debug/ferrum-migrate" ]]; then
    cargo build --features postgres --package ferrum-migrate
fi
if [[ -x "$REPO_ROOT/target/debug/ferrum-migrate" ]]; then
    FERRUM_MIGRATE="$REPO_ROOT/target/debug/ferrum-migrate"
    pass "ferrum-migrate binary is available"
else
    fail "ferrum-migrate binary is unavailable after build"
    exit 1
fi

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

echo ""
echo "========================================"
echo "Preparing local SQLite fixture"
echo "========================================"
echo ""

python3 "$REPO_ROOT/scripts/seed_pg_local_fixture.py" --db-path "$FIXTURE_DB" > "$FIXTURE_SUMMARY"
python3 - "$FIXTURE_SUMMARY" <<'PY'
import json
import sys
summary = json.load(open(sys.argv[1], encoding="utf-8"))
print(f"[INFO] Fixture DB: {summary['db_path']}")
for table, count in summary["counts"].items():
    print(f"[INFO] source_count {table}={count}")
PY
pass "synthetic SQLite fixture created"

echo ""
echo "========================================"
echo "Running SQLite -> PostgreSQL migration"
echo "========================================"
echo ""

"$FERRUM_MIGRATE" \
    --from "sqlite://$FIXTURE_DB" \
    --to "$PG_DSN" \
    --apply \
    --json > "$MIGRATE_REPORT"

python3 - "$MIGRATE_REPORT" <<'PY'
import json
import sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
if not report.get("applied"):
    print("[FAIL] migration report says apply=false")
    raise SystemExit(1)
if not report.get("overall_success"):
    print("[FAIL] migration report says overall_success=false")
    raise SystemExit(1)
tables = report.get("tables", [])
if len(tables) != 10:
    print(f"[FAIL] expected 10 migrated tables, saw {len(tables)}")
    raise SystemExit(1)
for table in tables:
    if not table.get("count_match") or not table.get("hash_match"):
        print(f"[FAIL] {table['table']} count_match={table.get('count_match')} hash_match={table.get('hash_match')}")
        raise SystemExit(1)
    print(f"[INFO] {table['table']}: source={table['source_count']} target={table['target_count']} migrated={table['migrated_count']} count_match={table['count_match']} hash_match={table['hash_match']}")
PY
pass "ferrum-migrate apply completed with 10/10 count+hash matches"

echo ""
echo "========================================"
echo "Starting ferrumd against migrated PostgreSQL"
echo "========================================"
echo ""

export FERRUMD_STORE_DSN="$PG_DSN"
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
    pass "ferrumd readyz/deep returned 200 against migrated PostgreSQL"
else
    fail "ferrumd did not become ready within 30s"
    if [[ -f "$FERRUMD_LOG" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$FERRUMD_LOG" || true
    fi
    exit 1
fi

echo ""
echo "========================================"
echo "Preparing workload prerequisites"
echo "========================================"
echo ""

# Pre-create SQLite table for workload generator's sqlite adapter
python3 -c "import sqlite3; conn = sqlite3.connect('/tmp/ferrum_g36.db'); conn.execute('CREATE TABLE IF NOT EXISTS g36_table (id INTEGER PRIMARY KEY, data TEXT)'); conn.commit(); conn.close()"
pass "sqlite prerequisite table ready"

echo ""
echo "========================================"
echo "Running sustained workload"
echo "========================================"
echo ""

mkdir -p "$WORKLOAD_OUT"

python3 "$REPO_ROOT/scripts/run_real_workload_generator.py" \
    --execute \
    --server-url "http://$FERRUMD_BIND_ADDR" \
    --bearer-token "dummy" \
    --phases "$SUSTAINED_PHASES" \
    --adapter-mix "$SUSTAINED_ADAPTER_MIX" \
    --output-dir "$WORKLOAD_OUT" \
    --readyz-probes 3 \
    --readyz-interval 5 \
    --readyz-probe-phase-interval 10 \
    --no-simulate-client-ips \
    --no-capture-connections

pass "sustained workload generator completed"

echo ""
echo "========================================"
echo "Post-workload probes and verification"
echo "========================================"
echo ""

# readyz/deep
RZ_STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "http://$FERRUMD_BIND_ADDR/v1/readyz/deep" || true)
if [[ "$RZ_STATUS" == "200" ]]; then
    pass "Post-workload readyz/deep returned 200"
else
    fail "Post-workload readyz/deep returned ${RZ_STATUS:-<failed>}"
fi

# metrics
METRICS_BODY=$(curl -sf "http://$FERRUMD_BIND_ADDR/v1/metrics" || true)
if [[ -n "$METRICS_BODY" ]]; then
    pass "Post-workload /v1/metrics returned body"
else
    fail "Post-workload /v1/metrics returned empty or failed"
fi

if echo "$METRICS_BODY" | grep -q "ferrumgate_store_pg_pool_size"; then
    pass "PG pool metrics present in /v1/metrics"
else
    fail "PG pool metrics missing in /v1/metrics"
fi

# Verify workload results: no errors, all 2xx
python3 - "$WORKLOAD_OUT/workload_results.json" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        data = json.load(f)
except Exception as e:
    print(f"[FAIL] Could not parse workload results: {e}")
    sys.exit(1)

total = data.get("summary", {}).get("total_requests", 0)
statuses = data.get("summary", {}).get("status_distribution", {})
errors = sum(len(p.get("errors", [])) for p in data.get("phases", []))

print(f"[INFO] Total requests: {total}")
print(f"[INFO] Status distribution: {statuses}")
print(f"[INFO] Phase errors: {errors}")

if errors > 0:
    print("[FAIL] Errors found in workload results")
    sys.exit(1)

for code, count in statuses.items():
    if code not in ("200", "202", "204"):
        print(f"[FAIL] Non-2xx status {code}: {count}")
        sys.exit(1)

print("[PASS] Workload results show no errors and all 2xx")
PY
pass "workload result verification passed"

echo ""
echo "========================================"
echo "PG SUSTAINED WORKLOAD DRILL SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo "Drill dir: $DRILL_DIR"
echo ""
echo "Boundary: local-only, Docker Compose, manual/optional."
echo "No production-ready claim. Block A remains WAIVED/CONDITIONAL."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "PG SUSTAINED WORKLOAD DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG SUSTAINED WORKLOAD DRILL: SOME CHECKS FAILED"
    exit 1
fi
