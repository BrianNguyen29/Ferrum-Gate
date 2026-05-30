#!/usr/bin/env bash
# run_pg_migration_drill.sh
# Local Docker-based SQLite -> PostgreSQL migration drill with a small
# deterministic synthetic fixture.
#
# Boundary: local-only, manual/optional, no production-ready claim.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
FERRUMD_BIND_ADDR="127.0.0.1:19087"
PG_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test"
NO_CLEANUP=false
FERRUMD_PID=""
PASS=0
FAIL=0
SKIP=0

DRILL_DIR="$(mktemp -d)"
FIXTURE_DB="$DRILL_DIR/populated_fixture.db"
FIXTURE_SUMMARY="$DRILL_DIR/fixture_summary.json"
REPORT_JSON="$DRILL_DIR/migration_report.json"
FERRUMD_LOG="$DRILL_DIR/ferrumd_pg_migration.log"

CORE_TABLES=(
    intents
    proposals
    capabilities
    executions
    rollback_contracts
    approvals
    provenance_events
    provenance_edges
    ledger_entries
    policy_bundles
)

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_pg_migration_drill.sh [--no-cleanup]

Runs a local SQLite -> PostgreSQL migration drill using Docker Compose PostgreSQL,
the ferrum-migrate binary, and a deterministic synthetic SQLite fixture.

Options:
  --no-cleanup   Leave the PostgreSQL container and temp drill directory behind.
  -h, --help     Show this help and exit.

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
echo "PG Migration Drill — Preflight"
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
    --json > "$REPORT_JSON"

python3 - "$REPORT_JSON" <<'PY'
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
        print(
            f"[FAIL] {table['table']} count_match={table.get('count_match')} "
            f"hash_match={table.get('hash_match')}"
        )
        raise SystemExit(1)
    print(
        f"[INFO] {table['table']}: source={table['source_count']} "
        f"target={table['target_count']} migrated={table['migrated_count']} "
        f"count_match={table['count_match']} hash_match={table['hash_match']}"
    )
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
echo "PG MIGRATION DRILL SUMMARY"
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
    echo "PG MIGRATION DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG MIGRATION DRILL: SOME CHECKS FAILED"
    exit 1
fi
