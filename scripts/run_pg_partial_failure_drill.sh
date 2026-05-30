#!/usr/bin/env bash
# run_pg_partial_failure_drill.sh
# Deterministic local partial-failure/resume migration simulation for ferrum-migrate.
#
# Boundary: local-only, manual/optional, no production-ready claim.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
SOURCE_DB="ferrumgate_p2_test"
PG_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/$SOURCE_DB"
NO_CLEANUP=false
PASS=0
FAIL=0
SKIP=0

DRILL_DIR="$(mktemp -d)"
FIXTURE_DB="$DRILL_DIR/populated_fixture.db"
FIXTURE_SUMMARY="$DRILL_DIR/fixture_summary.json"
BASELINE_REPORT_JSON="$DRILL_DIR/baseline_report.json"
RESUME_REPORT_JSON="$DRILL_DIR/resume_report.json"

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
Usage: bash scripts/run_pg_partial_failure_drill.sh [--no-cleanup]

Runs a deterministic local resume simulation for ferrum-migrate by:
- migrating a deterministic SQLite fixture into local Docker PostgreSQL
- deleting checkpoints and truncating three selected tables
- rerunning ferrum-migrate with --resume
- verifying that seven tables are skipped and three tables are re-migrated

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

psql_exec() {
    local db_name="$1"
    local sql="$2"
    docker exec -e PGDATABASE="$db_name" -e FERRUM_SQL="$sql" "$PG_CONTAINER" \
        bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -U ferrumgate_dev -d "$PGDATABASE" -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"' >/dev/null
}

psql_query() {
    local db_name="$1"
    local sql="$2"
    docker exec -e PGDATABASE="$db_name" -e FERRUM_SQL="$sql" "$PG_CONTAINER" \
        bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; psql -U ferrumgate_dev -d "$PGDATABASE" -At -v ON_ERROR_STOP=1 -c "$FERRUM_SQL"'
}

echo ""
echo "========================================"
echo "PG Partial-Failure/Resume Drill — Preflight"
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
echo "Building ferrum-migrate"
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

echo ""
echo "========================================"
echo "Preparing deterministic SQLite fixture"
echo "========================================"
echo ""

python3 "$REPO_ROOT/scripts/seed_pg_local_fixture.py" --db-path "$FIXTURE_DB" > "$FIXTURE_SUMMARY"
pass "synthetic SQLite fixture created"

echo ""
echo "========================================"
echo "Running baseline migration"
echo "========================================"
echo ""

"$FERRUM_MIGRATE" \
    --from "sqlite://$FIXTURE_DB" \
    --to "$PG_DSN" \
    --apply \
    --json > "$BASELINE_REPORT_JSON"

python3 - "$BASELINE_REPORT_JSON" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
if not report.get("applied") or not report.get("overall_success"):
    print("[FAIL] baseline migration did not complete successfully")
    raise SystemExit(1)
tables = report.get("tables", [])
if len(tables) != 10:
    print(f"[FAIL] expected 10 migrated tables, saw {len(tables)}")
    raise SystemExit(1)
for table in tables:
    if not table.get("count_match") or not table.get("hash_match"):
        print(
            f"[FAIL] baseline mismatch on {table['table']}: "
            f"count_match={table.get('count_match')} hash_match={table.get('hash_match')}"
        )
        raise SystemExit(1)
PY
pass "baseline migration created checkpoints for all 10 core tables"

echo ""
echo "========================================"
echo "Simulating partial failure state"
echo "========================================"
echo ""

psql_exec "$SOURCE_DB" "DELETE FROM _migration_checkpoints WHERE table_name IN ('approvals', 'ledger_entries', 'policy_bundles');"
psql_exec "$SOURCE_DB" "TRUNCATE TABLE approvals, ledger_entries, policy_bundles;"

CHECKPOINT_COUNT=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM _migration_checkpoints;")
echo "[INFO] Checkpoints remaining after simulation: $CHECKPOINT_COUNT"
if [[ "$CHECKPOINT_COUNT" == "7" ]]; then
    pass "checkpoint deletion left seven intact checkpoints"
else
    fail "expected 7 checkpoints after deletion, saw $CHECKPOINT_COUNT"
    exit 1
fi

for table in approvals ledger_entries policy_bundles; do
    table_count=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM $table;")
    echo "[INFO] $table truncated_count=$table_count"
    if [[ "$table_count" != "0" ]]; then
        fail "$table was not fully truncated before resume"
        exit 1
    fi
done
pass "selected tables truncated before resume"

echo ""
echo "========================================"
echo "Running resume migration"
echo "========================================"
echo ""

"$FERRUM_MIGRATE" \
    --from "sqlite://$FIXTURE_DB" \
    --to "$PG_DSN" \
    --apply \
    --resume \
    --json > "$RESUME_REPORT_JSON"

python3 - "$RESUME_REPORT_JSON" <<'PY'
import json
import sys
from pathlib import Path

expected_remigrated = {"approvals", "ledger_entries", "policy_bundles"}
expected_skipped = {
    "intents",
    "proposals",
    "capabilities",
    "executions",
    "rollback_contracts",
    "provenance_events",
    "provenance_edges",
}

raw = Path(sys.argv[1]).read_text(encoding="utf-8")
json_start = raw.find("{")
if json_start < 0:
    print("[FAIL] resume report did not contain a JSON payload")
    raise SystemExit(1)
report = json.loads(raw[json_start:])
if not report.get("applied") or not report.get("overall_success"):
    print("[FAIL] resume migration did not complete successfully")
    raise SystemExit(1)

tables = report.get("tables", [])
if len(tables) != 10:
    print(f"[FAIL] expected 10 tables in resume report, saw {len(tables)}")
    raise SystemExit(1)

seen_remigrated = set()
seen_skipped = set()

for table in tables:
    name = table["table"]
    migrated = table.get("migrated_count", 0)
    if not table.get("count_match") or not table.get("hash_match"):
        print(
            f"[FAIL] resume mismatch on {name}: count_match={table.get('count_match')} "
            f"hash_match={table.get('hash_match')}"
        )
        raise SystemExit(1)
    if name in expected_remigrated:
        if migrated <= 0:
            print(f"[FAIL] expected {name} to be re-migrated, migrated_count={migrated}")
            raise SystemExit(1)
        seen_remigrated.add(name)
        print(
            f"[INFO] re-migrated {name}: source={table['source_count']} "
            f"target={table['target_count']} migrated={migrated}"
        )
    elif name in expected_skipped:
        if migrated != 0:
            print(f"[FAIL] expected {name} to be skipped, migrated_count={migrated}")
            raise SystemExit(1)
        seen_skipped.add(name)
        print(
            f"[INFO] skipped {name}: source={table['source_count']} "
            f"target={table['target_count']} migrated={migrated}"
        )
    else:
        print(f"[FAIL] unexpected table in resume report: {name}")
        raise SystemExit(1)

if seen_remigrated != expected_remigrated:
    print(f"[FAIL] remigrated set mismatch: {seen_remigrated} != {expected_remigrated}")
    raise SystemExit(1)
if seen_skipped != expected_skipped:
    print(f"[FAIL] skipped set mismatch: {seen_skipped} != {expected_skipped}")
    raise SystemExit(1)
PY
pass "resume migration skipped 7 tables and re-migrated the expected 3 tables"

FINAL_CHECKPOINT_COUNT=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM _migration_checkpoints;")
echo "[INFO] Checkpoints after resume: $FINAL_CHECKPOINT_COUNT"
if [[ "$FINAL_CHECKPOINT_COUNT" == "10" ]]; then
    pass "all ten checkpoints restored after resume"
else
    fail "expected 10 checkpoints after resume, saw $FINAL_CHECKPOINT_COUNT"
    exit 1
fi

declare -A EXPECTED_COUNTS=(
    [intents]=1
    [proposals]=1
    [capabilities]=1
    [executions]=1
    [rollback_contracts]=1
    [approvals]=1
    [provenance_events]=2
    [provenance_edges]=1
    [ledger_entries]=2
    [policy_bundles]=1
)

echo "[INFO] Verifying final row counts against deterministic fixture..."
for table in "${CORE_TABLES[@]}"; do
    actual=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM $table;")
    expected="${EXPECTED_COUNTS[$table]}"
    echo "[INFO] $table expected=$expected actual=$actual"
    if [[ "$actual" != "$expected" ]]; then
        fail "$table row count mismatch after resume"
        exit 1
    fi
done
pass "all 10 core table counts match deterministic fixture after resume"

echo ""
echo "========================================"
echo "PG PARTIAL-FAILURE/RESUME DRILL SUMMARY"
echo "========================================"
echo "Passed:  $PASS"
echo "Failed:  $FAIL"
echo "Skipped: $SKIP"
echo "Drill dir: $DRILL_DIR"
echo ""
echo "Boundary: local-only deterministic simulation; true live process interruption is NOT tested."
echo "No production-ready claim. Block A remains WAIVED/CONDITIONAL."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "PG PARTIAL-FAILURE/RESUME DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG PARTIAL-FAILURE/RESUME DRILL: SOME CHECKS FAILED"
    exit 1
fi
