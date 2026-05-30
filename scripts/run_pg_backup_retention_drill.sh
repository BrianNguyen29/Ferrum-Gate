#!/usr/bin/env bash
# run_pg_backup_retention_drill.sh
# Local Docker-based PostgreSQL backup/retention/offsite drill.
#
# Boundary: local-only, manual/optional, no production-ready claim.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMPOSE_FILE="$REPO_ROOT/docker-compose.postgres.yml"
PG_CONTAINER="ferrumgate_postgres_p2"
PG_SERVICE="postgres_p2"
SOURCE_DB="ferrumgate_p2_test"
RESTORE_DB="ferrumgate_pg_offsite_restore_drill"
PG_DSN="postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/$SOURCE_DB"
RETENTION_DAYS=4
NO_CLEANUP=false
PASS=0
FAIL=0
SKIP=0

DRILL_DIR="$(mktemp -d)"
FIXTURE_DB="$DRILL_DIR/populated_fixture.db"
FIXTURE_SUMMARY="$DRILL_DIR/fixture_summary.json"
MIGRATION_REPORT_JSON="$DRILL_DIR/migration_report.json"
BACKUP_DIR="$DRILL_DIR/backups"
OFFSITE_DIR="$DRILL_DIR/offsite"
mkdir -p "$BACKUP_DIR" "$OFFSITE_DIR"

BACKUP_BASENAME="ferrumgate_local_$(date -u +%Y%m%dT%H%M%SZ).dump"
BACKUP_FILE="$BACKUP_DIR/$BACKUP_BASENAME"
OFFSITE_FILE="$OFFSITE_DIR/$BACKUP_BASENAME"
CONTAINER_BACKUP="/tmp/$BACKUP_BASENAME"
CONTAINER_OFFSITE="/tmp/offsite_$BACKUP_BASENAME"
OLD_MATCHING="$BACKUP_DIR/ferrumgate_old_20260501.dump"
OLD_NONMATCHING="$BACKUP_DIR/other_service_20260501.dump"
TOC_FILE="$DRILL_DIR/backup_toc.txt"

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
Usage: bash scripts/run_pg_backup_retention_drill.sh [--no-cleanup]

Runs a local PostgreSQL backup automation wrapper that:
- seeds and migrates a deterministic SQLite fixture into local Docker PostgreSQL
- creates a pg_dump backup archive
- simulates retention pruning for old matching dump files
- simulates offsite sync by copying the current dump locally and verifying hash integrity
- restores from the simulated offsite copy into a fresh drill database

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
echo "PG Backup/Retention/Offsite Drill — Preflight"
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
echo "Seeding retention simulation files"
echo "========================================"
echo ""

python3 - "$OLD_MATCHING" "$OLD_NONMATCHING" <<'PY'
import os
import sys
import time
from pathlib import Path

old_epoch = int(time.time()) - (30 * 24 * 60 * 60)
for path_str in sys.argv[1:]:
    path = Path(path_str)
    path.write_bytes(b"local-pg-retention-simulation\n")
    os.utime(path, (old_epoch, old_epoch))
    print(f"[INFO] Seeded old backup: {path}")
PY
pass "old retention simulation files created"

echo ""
echo "========================================"
echo "Creating pg_dump backup"
echo "========================================"
echo ""

docker exec -e CONTAINER_BACKUP="$CONTAINER_BACKUP" "$PG_CONTAINER" \
    bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; rm -f "$CONTAINER_BACKUP"; pg_dump -U ferrumgate_dev -d ferrumgate_p2_test -Fc --no-owner --no-privileges -f "$CONTAINER_BACKUP"'
docker cp "$PG_CONTAINER:$CONTAINER_BACKUP" "$BACKUP_FILE"

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

docker exec -e CONTAINER_BACKUP="$CONTAINER_BACKUP" "$PG_CONTAINER" \
    bash -lc 'pg_restore -l "$CONTAINER_BACKUP"' > "$TOC_FILE"
if grep -q 'TABLE public intents' "$TOC_FILE" && grep -q 'TABLE public policy_bundles' "$TOC_FILE"; then
    pass "pg_restore -l lists expected FerrumGate tables"
else
    fail "pg_restore -l did not list expected FerrumGate tables"
    exit 1
fi

echo ""
echo "========================================"
echo "Running retention pruning simulation"
echo "========================================"
echo ""

find "$BACKUP_DIR" -name 'ferrumgate_*.dump' -mtime +$RETENTION_DAYS -delete

if [[ ! -f "$OLD_MATCHING" ]]; then
    pass "old matching dump file was pruned"
else
    fail "old matching dump file was not pruned"
    exit 1
fi

if [[ -f "$OLD_NONMATCHING" ]]; then
    pass "nonmatching old dump file was preserved"
else
    fail "nonmatching old dump file was incorrectly removed"
    exit 1
fi

if [[ -f "$BACKUP_FILE" ]]; then
    pass "current backup file was preserved"
else
    fail "current backup file was incorrectly removed"
    exit 1
fi

echo ""
echo "========================================"
echo "Simulating offsite sync"
echo "========================================"
echo ""

cp "$BACKUP_FILE" "$OFFSITE_FILE"
pass "backup copied to simulated offsite directory"

python3 - "$BACKUP_FILE" "$OFFSITE_FILE" <<'PY'
import hashlib
import sys
from pathlib import Path

source = Path(sys.argv[1])
offsite = Path(sys.argv[2])
source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
offsite_hash = hashlib.sha256(offsite.read_bytes()).hexdigest()
print(f"[INFO] Source sha256:  {source_hash}")
print(f"[INFO] Offsite sha256: {offsite_hash}")
if source_hash != offsite_hash:
    print("[FAIL] offsite hash mismatch")
    raise SystemExit(1)
PY
pass "offsite copy hash matches local backup"

echo ""
echo "========================================"
echo "Restoring from simulated offsite copy"
echo "========================================"
echo ""

docker exec -e CONTAINER_OFFSITE="$CONTAINER_OFFSITE" "$PG_CONTAINER" \
    bash -lc 'rm -f "$CONTAINER_OFFSITE"'
docker cp "$OFFSITE_FILE" "$PG_CONTAINER:$CONTAINER_OFFSITE"

psql_exec postgres "DROP DATABASE IF EXISTS $RESTORE_DB;"
psql_exec postgres "CREATE DATABASE $RESTORE_DB;"
pass "fresh restore drill database created"

docker exec -e RESTORE_DB="$RESTORE_DB" -e CONTAINER_OFFSITE="$CONTAINER_OFFSITE" "$PG_CONTAINER" \
    bash -lc 'export PGPASSWORD="$POSTGRES_PASSWORD"; pg_restore -U ferrumgate_dev -d "$RESTORE_DB" --clean --if-exists "$CONTAINER_OFFSITE"' >/dev/null
pass "offsite copy restored successfully"

RESTORED_TABLE_COUNT=$(psql_query "$RESTORE_DB" "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';")
echo "[INFO] Restored public table count: $RESTORED_TABLE_COUNT"
if [[ "$RESTORED_TABLE_COUNT" -ge 11 ]]; then
    pass "restored offsite drill database exposes expected public tables"
else
    fail "restored offsite drill database has fewer than 11 public tables"
    exit 1
fi

echo "[INFO] Comparing source vs restored row counts..."
for table in "${CORE_TABLES[@]}"; do
    source_count=$(psql_query "$SOURCE_DB" "SELECT COUNT(*) FROM $table;")
    restored_count=$(psql_query "$RESTORE_DB" "SELECT COUNT(*) FROM $table;")
    echo "[INFO] $table source=$source_count restored=$restored_count"
    if [[ "$source_count" != "$restored_count" ]]; then
        fail "$table row count mismatch after offsite restore"
        exit 1
    fi
done
pass "all 10 core table row counts match after offsite restore"

echo ""
echo "========================================"
echo "PG BACKUP/RETENTION/OFFSITE DRILL SUMMARY"
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
    echo "PG BACKUP/RETENTION/OFFSITE DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "PG BACKUP/RETENTION/OFFSITE DRILL: SOME CHECKS FAILED"
    exit 1
fi
