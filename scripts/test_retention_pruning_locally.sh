#!/usr/bin/env bash
# test_retention_pruning_locally.sh
# B3 — Local retention pruning test for FerrumGate v1 backup subsystem.
# Creates a temporary SQLite DB and backup directory, seeds old backups,
# runs ferrumctl backup create with --retention-days, and asserts pruning behavior.
# Does NOT require target host, SSH, domain, TLS, or real secrets.
# Does NOT claim G2/pilot/production-ready.
#
# Usage:
#   bash scripts/test_retention_pruning_locally.sh [--help]
#   bash scripts/test_retention_pruning_locally.sh [--retention-days N] [--ferrumctl PATH]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RETENTION_DAYS="${RETENTION_DAYS:-7}"
FERRUMCTL_BIN="${FERRUMCTL:-}"

usage() {
    cat << 'EOF'
B3 Local Retention Pruning Test

Usage:
  bash scripts/test_retention_pruning_locally.sh [options]

Options:
  --retention-days N   Retention days for pruning test (default: 7)
  --ferrumctl PATH     Path to ferrumctl binary
  --help               Show this help message and exit

Description:
  This script creates a temporary SQLite database and backup directory,
  seeds an old matching backup file and a non-matching old file, runs
  ferrumctl backup create with --retention-days, and asserts that:
    - The old matching backup is pruned
    - The new backup is created and kept
    - The non-matching old file is preserved

  All operations are local and temporary. No target paths are touched.
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            usage
            exit 0
            ;;
        --retention-days)
            RETENTION_DAYS="${2:-}"
            if [[ -z "$RETENTION_DAYS" ]]; then
                echo "[ERROR] --retention-days requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --ferrumctl)
            FERRUMCTL_BIN="${2:-}"
            if [[ -z "$FERRUMCTL_BIN" ]]; then
                echo "[ERROR] --ferrumctl requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        *)
            echo "[ERROR] Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# --- Find ferrumctl ---
if [[ -z "$FERRUMCTL_BIN" ]]; then
    for candidate in \
        "$REPO_ROOT/target/release/ferrumctl" \
        "$REPO_ROOT/target/debug/ferrumctl" \
        "$(command -v ferrumctl 2>/dev/null || true)"; do
        if [[ -n "$candidate" ]] && [[ -x "$candidate" ]]; then
            FERRUMCTL_BIN="$candidate"
            break
        fi
    done
fi

if [[ -z "$FERRUMCTL_BIN" ]] || [[ ! -x "$FERRUMCTL_BIN" ]]; then
    echo "[INFO] ferrumctl not found; building release ferrumctl..." >&2
    if cargo build --release --bin ferrumctl --manifest-path "$REPO_ROOT/Cargo.toml" >/dev/null 2>&1; then
        FERRUMCTL_BIN="$REPO_ROOT/target/release/ferrumctl"
    else
        echo "[FAIL] Failed to build ferrumctl. Build manually with:" >&2
        echo "       cargo build --release --bin ferrumctl" >&2
        exit 1
    fi
fi

if [[ -z "$FERRUMCTL_BIN" ]] || [[ ! -x "$FERRUMCTL_BIN" ]]; then
    echo "[FAIL] ferrumctl not found or not executable after build attempt." >&2
    exit 1
fi

echo "[INFO] Using ferrumctl: $FERRUMCTL_BIN"

# --- Setup temp directory ---
TEST_DIR=$(mktemp -d)
BACKUP_DIR="$TEST_DIR/backups"
mkdir -p "$BACKUP_DIR"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "[INFO] Test directory: $TEST_DIR"

# --- Create a test SQLite database ---
DB_PATH="$TEST_DIR/ferrumgate_test.db"

if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import sqlite3
conn = sqlite3.connect('$DB_PATH')
c = conn.cursor()
c.execute('CREATE TABLE IF NOT EXISTS retention_test (id INTEGER PRIMARY KEY)')
conn.commit()
conn.close()
"
elif command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 "$DB_PATH" "CREATE TABLE IF NOT EXISTS retention_test (id INTEGER PRIMARY KEY);"
else
    echo "[FAIL] Neither python3 nor sqlite3 available to create test DB" >&2
    exit 1
fi

echo "[INFO] Test DB created: $DB_PATH"

# --- Seed old backup files ---
DB_BASENAME=$(basename "$DB_PATH")
OLD_TIMESTAMP=$(date -d '30 days ago' +%s 2>/dev/null || date -v -30d +%s 2>/dev/null || echo "1700000000")
MATCHING_OLD="$BACKUP_DIR/${DB_BASENAME}_${OLD_TIMESTAMP}.db"
NONMATCHING_OLD="$BACKUP_DIR/other_db_${OLD_TIMESTAMP}.db"

# Create valid-looking SQLite files for the old backups with old mtime
OLD_MTIME_SECONDS=$(date -d '30 days ago' +%s 2>/dev/null || date -v -30d +%s 2>/dev/null || echo "1700000000")
if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import sqlite3
import os
import time
for path in ['$MATCHING_OLD', '$NONMATCHING_OLD']:
    conn = sqlite3.connect(path)
    c = conn.cursor()
    c.execute('CREATE TABLE IF NOT EXISTS seed (id INTEGER PRIMARY KEY)')
    conn.commit()
    conn.close()
    # Set mtime to older than retention period
    mtime = $OLD_MTIME_SECONDS
    os.utime(path, (mtime, mtime))
"
else
    sqlite3 "$MATCHING_OLD" "CREATE TABLE IF NOT EXISTS seed (id INTEGER PRIMARY KEY);"
    sqlite3 "$NONMATCHING_OLD" "CREATE TABLE IF NOT EXISTS seed (id INTEGER PRIMARY KEY);"
    # Set mtime using touch -d (GNU) or touch -t (BSD)
    OLD_TOUCH_DATE=$(date -d '30 days ago' '+%Y%m%d%H%M.%S' 2>/dev/null || date -v -30d '+%Y%m%d%H%M.%S' 2>/dev/null || echo "")
    if [[ -n "$OLD_TOUCH_DATE" ]]; then
        touch -t "$OLD_TOUCH_DATE" "$MATCHING_OLD" "$NONMATCHING_OLD" 2>/dev/null || true
    fi
fi

echo "[INFO] Seeded old matching backup:   $MATCHING_OLD"
echo "[INFO] Seeded old nonmatching backup: $NONMATCHING_OLD"

# --- Run ferrumctl backup create with retention ---
echo "[INFO] Running: ferrumctl backup create --db-path <DB> --output-dir <DIR> --retention-days $RETENTION_DAYS"
CREATE_OUTPUT=$("$FERRUMCTL_BIN" backup create --db-path "$DB_PATH" --output-dir "$BACKUP_DIR" --retention-days "$RETENTION_DAYS" 2>&1) || true
echo "$CREATE_OUTPUT"

# Find the newly created backup
NEW_BACKUP=$(ls -1t "$BACKUP_DIR"/*.db 2>/dev/null | head -1 || true)
if [[ -z "$NEW_BACKUP" ]] || [[ ! -f "$NEW_BACKUP" ]]; then
    echo "[FAIL] New backup file was not created" >&2
    ls -la "$BACKUP_DIR" || true
    exit 1
fi

echo "[INFO] New backup created: $NEW_BACKUP"

# --- Assertions ---
FAILED=0

if [[ -f "$MATCHING_OLD" ]]; then
    echo "[FAIL] Old matching backup was NOT pruned: $MATCHING_OLD"
    FAILED=$((FAILED + 1))
else
    echo "[PASS] Old matching backup was pruned"
fi

if [[ -f "$NONMATCHING_OLD" ]]; then
    echo "[PASS] Nonmatching old backup was preserved"
else
    echo "[FAIL] Nonmatching old backup was incorrectly removed: $NONMATCHING_OLD"
    FAILED=$((FAILED + 1))
fi

if [[ -f "$NEW_BACKUP" ]]; then
    echo "[PASS] New backup was kept"
else
    echo "[FAIL] New backup was incorrectly removed"
    FAILED=$((FAILED + 1))
fi

# --- Summary ---
echo ""
echo "========================================"
echo "B3 RETENTION PRUNING RESULT"
echo "========================================"
echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "B3: ALL CHECKS PASSED"
    echo ""
    echo "Retention pruning works correctly in a temporary local environment."
    echo "This does NOT constitute G2 completion or pilot readiness."
    echo ""
    exit 0
else
    echo "B3: SOME CHECKS FAILED ($FAILED)"
    echo ""
    exit 1
fi
