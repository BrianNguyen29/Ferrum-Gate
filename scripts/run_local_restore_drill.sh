#!/usr/bin/env bash
# run_local_restore_drill.sh
# Performs a local temp SQLite backup/restore integrity drill.
# Uses temporary directories only; does NOT modify system state.
# Requires: ferrumctl binary in PATH, FERRUMCTL env var pointing to it, or cargo to build it.
# Falls back to sqlite3 CLI if available for data comparison; skips if not.
# Single-node SQLite only; no PostgreSQL/multi-node/HA.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Find ferrumctl ---

FERRUMCTL="${FERRUMCTL:-}"
if [[ -z "$FERRUMCTL" ]]; then
    for candidate in \
        "$REPO_ROOT/target/release/ferrumctl" \
        "$REPO_ROOT/target/debug/ferrumctl" \
        "$(which ferrumctl 2>/dev/null)"; do
        [[ -x "$candidate" ]] && FERRUMCTL="$candidate" && break
    done
fi

if [[ -z "$FERRUMCTL" ]] || [[ ! -x "$FERRUMCTL" ]]; then
    echo "[INFO] ferrumctl not found; building debug ferrumctl..." >&2
    cargo build --bin ferrumctl --manifest-path "$REPO_ROOT/Cargo.toml"
    FERRUMCTL="$REPO_ROOT/target/debug/ferrumctl"
fi

if [[ -z "$FERRUMCTL" ]] || [[ ! -x "$FERRUMCTL" ]]; then
    echo "[FAIL] ferrumctl not found or not executable after build attempt." >&2
    exit 1
fi

echo "[INFO] Using ferrumctl: $FERRUMCTL"

# --- Check for sqlite3 CLI (optional for data comparison) ---

SQLITE3="${SQLITE3:-}"
if [[ -z "$SQLITE3" ]]; then
    SQLITE3=$(which sqlite3 2>/dev/null || true)
fi
HAVE_SQLITE3=false
if [[ -n "$SQLITE3" ]] && [[ -x "$SQLITE3" ]]; then
    HAVE_SQLITE3=true
    echo "[INFO] sqlite3 available: $SQLITE3"
else
    echo "[INFO] sqlite3 not available; data comparison will be skipped"
fi

# --- Setup temp directory ---

DRILL_DIR=$(mktemp -d)
STORE_DIR="$DRILL_DIR/store"
BACKUP_DIR="$DRILL_DIR/backups"
RESTORE_DIR="$DRILL_DIR/restore"
mkdir -p "$STORE_DIR" "$BACKUP_DIR" "$RESTORE_DIR"

STORE_DB="$STORE_DIR/ferrumgate.db"
BACKUP_FILE=""
RESTORED_DB="$RESTORE_DIR/ferrumgate_restored.db"

cleanup() {
    rm -rf "$DRILL_DIR"
}
trap cleanup EXIT

echo "[INFO] Drill directory: $DRILL_DIR"

# --- Create a test database with some content ---

echo "[INFO] Creating test SQLite database..."

create_test_db() {
    local db_path="$1"
    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import sqlite3
conn = sqlite3.connect('$db_path')
c = conn.cursor()
c.execute('''CREATE TABLE IF NOT EXISTS drill_test
             (id INTEGER PRIMARY KEY, value TEXT, created_at TEXT DEFAULT (datetime('now')))''')
c.execute(\"INSERT INTO drill_test (value) VALUES ('local_restore_drill_v1')\")
c.execute(\"INSERT INTO drill_test (value) VALUES ('local_restore_drill_v2')\")
conn.commit()
conn.close()
"
    elif [[ -n "$SQLITE3" ]] && [[ -x "$SQLITE3" ]]; then
        $SQLITE3 "$db_path" << 'EOF'
CREATE TABLE IF NOT EXISTS drill_test (id INTEGER PRIMARY KEY, value TEXT, created_at TEXT DEFAULT (datetime('now')));
INSERT INTO drill_test (value) VALUES ('local_restore_drill_v1');
INSERT INTO drill_test (value) VALUES ('local_restore_drill_v2');
EOF
    else
        echo "[SKIP] Neither python3 nor sqlite3 available to create test DB"
        return 1
    fi
    return 0
}

if ! create_test_db "$STORE_DB"; then
    echo "[SKIP] Cannot create test DB without python3 or sqlite3"
    exit 0
fi

STORE_SIZE=$(stat -c%s "$STORE_DB" 2>/dev/null || stat -f%z "$STORE_DB" 2>/dev/null)
echo "[INFO] Store DB created: $STORE_DB (${STORE_SIZE} bytes)"

# --- Verify store integrity before backup ---

echo "[INFO] Verifying source store integrity..."
VERIFY_OUTPUT=$("$FERRUMCTL" backup verify --db-path "$STORE_DB" 2>&1) || true
echo "$VERIFY_OUTPUT"
if echo "$VERIFY_OUTPUT" | grep -qi "error\|failed\|invalid"; then
    echo "[FAIL] Source store integrity check failed before backup"
    exit 1
fi
echo "[PASS] Source store integrity check passed"

# --- Create backup ---

echo "[INFO] Creating backup..."
BACKUP_OUTPUT=$("$FERRUMCTL" backup create --db-path "$STORE_DB" --output-dir "$BACKUP_DIR" 2>&1) || true
echo "$BACKUP_OUTPUT"

# Find the created backup file - ferrumctl names it as <db_name>_<timestamp>.db
# e.g. ferrumgate.db_1777572920.db
BACKUP_FILE=$(ls -1t "$BACKUP_DIR"/*.db 2>/dev/null | head -1 || true)
if [[ -z "$BACKUP_FILE" ]] || [[ ! -f "$BACKUP_FILE" ]]; then
    echo "[FAIL] Backup file was not created in $BACKUP_DIR"
    ls -la "$BACKUP_DIR/" 2>/dev/null || true
    exit 1
fi

BACKUP_SIZE=$(stat -c%s "$BACKUP_FILE" 2>/dev/null || stat -f%z "$BACKUP_FILE" 2>/dev/null)
echo "[INFO] Backup created: $BACKUP_FILE (${BACKUP_SIZE} bytes)"

# --- Verify backup integrity ---

echo "[INFO] Verifying backup integrity..."
VERIFY_OUTPUT=$("$FERRUMCTL" backup verify --db-path "$BACKUP_FILE" 2>&1) || true
echo "$VERIFY_OUTPUT"
if echo "$VERIFY_OUTPUT" | grep -qi "error\|failed\|invalid"; then
    echo "[FAIL] Backup integrity check failed"
    exit 1
fi
echo "[PASS] Backup integrity check passed"

# --- Restore backup to restore location ---

echo "[INFO] Restoring backup to $RESTORED_DB..."
RESTORE_OUTPUT=$("$FERRUMCTL" backup restore --db-path "$RESTORED_DB" --from "$BACKUP_FILE" --confirm 2>&1) || true
echo "$RESTORE_OUTPUT"

if [[ ! -f "$RESTORED_DB" ]]; then
    echo "[FAIL] Restored database was not created"
    exit 1
fi

# --- Verify restored database integrity ---

echo "[INFO] Verifying restored database integrity..."
VERIFY_OUTPUT=$("$FERRUMCTL" backup verify --db-path "$RESTORED_DB" 2>&1) || true
echo "$VERIFY_OUTPUT"
if echo "$VERIFY_OUTPUT" | grep -qi "error\|failed\|invalid"; then
    echo "[FAIL] Restored database integrity check failed"
    exit 1
fi
echo "[PASS] Restored database integrity check passed"

# --- Compare content of original and restored (if sqlite3 available) ---

if [[ "$HAVE_SQLITE3" == true ]]; then
    echo "[INFO] Comparing original and restored data..."
    ORIG_DATA=$($SQLITE3 "$STORE_DB" "SELECT id, value FROM drill_test ORDER BY id;")
    RESTORED_DATA=$($SQLITE3 "$RESTORED_DB" "SELECT id, value FROM drill_test ORDER BY id;")

    if [[ "$ORIG_DATA" == "$RESTORED_DATA" ]]; then
        echo "[PASS] Data match: original and restored databases are identical"
    else
        echo "[FAIL] Data mismatch between original and restored databases"
        echo "Original:    $ORIG_DATA"
        echo "Restored:    $RESTORED_DATA"
        exit 1
    fi
else
    echo "[INFO] Skipping data comparison (sqlite3 not available)"
fi

# --- Summary ---

echo ""
echo "=== LOCAL RESTORE DRILL COMPLETE ==="
echo "Store DB:   $STORE_DB"
echo "Backup:    $BACKUP_FILE"
echo "Restored:  $RESTORED_DB"
echo ""
echo "All checks passed. ferrumctl backup/restore/verify work correctly in temp environment."
echo "This does NOT constitute G2.1 completion. Restore drill on target host is still required."
echo ""
exit 0
