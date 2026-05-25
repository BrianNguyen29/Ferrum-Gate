#!/usr/bin/env bash
# run_pg_restore_drill.sh
# Local Docker-based PostgreSQL backup/restore drill using a deterministic
# SQLite fixture migrated into the local PostgreSQL target first.
#
# Boundary: local-only, manual/optional, no production-ready claim.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
SOURCE_DB="ferrumgate_p2_test"
RESTORE_DB="ferrumgate_pg_restore_drill"
FERRUMD_BIND_ADDR="127.0.0.1:19086"
PG_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/$SOURCE_DB"
RESTORE_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/$RESTORE_DB"
NO_CLEANUP=false
FERRUMD_PID=""
PASS=0
FAIL=0
SKIP=0

DRILL_DIR="$(mktemp -d)"
FIXTURE_DB="$DRILL_DIR/populated_fixture.db"
FIXTURE_SUMMARY="$DRILL_DIR/fixture_summary.json"
MIGRATION_REPORT_JSON="$DRILL_DIR/migration_report.json"
BACKUP_FILE="$DRILL_DIR/ferrumgate_pg_restore_drill.dump"
TOC_FILE="$DRILL_DIR/ferrumgate_pg_restore_drill.toc.txt"
FERRUMD_LOG="$DRILL_DIR/ferrumd_pg_restore.log"

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
Usage: bash scripts/run_pg_restore_drill.sh [--no-cleanup]

Runs a local PostgreSQL backup/restore drill by migrating a deterministic
SQLite fixture into the local Docker PostgreSQL target, creating a pg_dump
archive, restoring into a fresh drill database, and validating readyz.

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
echo "PG Restore Drill — Preflight"
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
echo "Preparing populated PostgreSQL source"
echo "========================================"
echo ""

python3 "$REPO_ROOT/scripts/seed_pg_local_fixture.py" --db-path "$FIXTURE_DB" > "$FIXTURE_SUMMARY"
pass "synthetic SQLite fixture created"

"$FERRUM_MIGRATE" \
    --from "sqlite://$FIXTURE_DB" \
    --to "$PG_DSN" \
    --apply \
    --json > "$MIGRATION_REPORT_JSON"

python3 - "$MIGRATION_REPORT_JSON" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
if not report.get("applied") or not report.get("overall_success"):
    print("[FAIL] source preparation migration did not complete successfully")
    raise SystemExit(1)
tables = report.get("tables", [])
if len(tables) != 10:
    print(f"[FAIL] expected 10 migrated tables, saw {len(tables)}")
    raise SystemExit(1)
for table in tables:
    if not table.get("count_match") or not table.get("hash_match"):
        print(
            f"[FAIL] source preparation mismatch on {table['table']}: "
            f"count_match={table.get('count_match')} hash_match={table.get('hash_match')}"
        )
        raise SystemExit(1)
PY
pass "source PostgreSQL database populated via ferrum-migrate"

echo ""
echo "========================================"
echo "Creating pg_dump archive"
echo "========================================"
echo ""

docker exec "$PG_CONTAINER" bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; rm -f /tmp/ferrumgate_pg_restore_drill.dump; pg_dump -U ferrumgate_dev -d ferrumgate_p2_test -Fc --no-owner --no-privileges -f /tmp/ferrumgate_pg_restore_drill.dump'
docker cp "$PG_CONTAINER:/tmp/ferrumgate_pg_restore_drill.dump" "$BACKUP_FILE"
if [[ -s "$BACKUP_FILE" ]]; then
    pass "pg_dump archive created and copied to host temp path"
else
    fail "pg_dump archive missing or empty"
    exit 1
fi

python3 - "$BACKUP_FILE" <<'PY'
import hashlib
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = path.read_bytes()
print(f"[INFO] Backup artifact: {path}")
print(f"[INFO] Backup size bytes: {len(data)}")
print(f"[INFO] Backup sha256: {hashlib.sha256(data).hexdigest()}")
PY

docker exec "$PG_CONTAINER" bash -lc 'pg_restore -l /tmp/ferrumgate_pg_restore_drill.dump' > "$TOC_FILE"
if grep -q 'TABLE public intents' "$TOC_FILE" && grep -q 'TABLE public policy_bundles' "$TOC_FILE"; then
    pass "pg_restore -l lists expected FerrumGate tables"
else
    fail "pg_restore -l did not list expected FerrumGate tables"
    exit 1
fi

echo ""
echo "========================================"
echo "Restoring into fresh drill database"
echo "========================================"
echo ""

psql_exec postgres "DROP DATABASE IF EXISTS $RESTORE_DB;"
psql_exec postgres "CREATE DATABASE $RESTORE_DB;"
pass "fresh restore drill database created"

docker exec -e RESTORE_DB="$RESTORE_DB" "$PG_CONTAINER" \
    bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; pg_restore -U ferrumgate_dev -d "$RESTORE_DB" --clean --if-exists /tmp/ferrumgate_pg_restore_drill.dump' >/dev/null
pass "pg_restore completed successfully"

TABLE_COUNT=$(psql_query "$RESTORE_DB" "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';")
echo "[INFO] Restored public table count: $TABLE_COUNT"
if [[ "$TABLE_COUNT" -ge 11 ]]; then
    pass "restored drill database exposes expected public tables"
else
    fail "restored drill database has fewer than 11 public tables"
    exit 1
fi

echo "[INFO] Comparing source vs restored row counts..."
for table in "${CORE_TABLES[@]}"; do
    source_count=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM $table;")
    restored_count=$(psql_query "$RESTORE_DB" "SELECT COUNT(*) FROM $table;")
    echo "[INFO] $table source=$source_count restored=$restored_count"
    if [[ "$source_count" != "$restored_count" ]]; then
        fail "$table row count mismatch after restore"
        exit 1
    fi
done
pass "all 10 core table row counts match after restore"

echo ""
echo "========================================"
echo "Starting ferrumd against restored database"
echo "========================================"
echo ""

export FERRUMD_STORE_DSN="$RESTORE_DSN"
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
    pass "ferrumd readyz/deep returned 200 against restored PostgreSQL"
else
    fail "ferrumd did not become ready against restored database within 30s"
    if [[ -f "$FERRUMD_LOG" ]]; then
        echo "[INFO] Last 20 lines of ferrumd log:"
        tail -n 20 "$FERRUMD_LOG" || true
    fi
    exit 1
fi

echo ""
echo "========================================"
echo "PG RESTORE DRILL SUMMARY"
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
    echo "PG RESTORE DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG RESTORE DRILL: SOME CHECKS FAILED"
    exit 1
fi
