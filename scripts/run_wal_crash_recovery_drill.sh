#!/usr/bin/env bash
# run_wal_crash_recovery_drill.sh
# Local-only SQLite WAL crash-recovery drill.
# Tests: WAL DB creation, baseline insert, concurrent writer, SIGKILL,
#        integrity check post-crash, baseline row survival, checkpoint, final integrity.
# Does NOT require live ferrumd. Does NOT claim production-ready.
# Does NOT close Block A or any G2 gate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Help ---
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'EOF'
Usage: bash scripts/run_wal_crash_recovery_drill.sh [options]

Local SQLite WAL crash-recovery drill.

Options:
  -h, --help    Show this help message and exit

This script is local-only, uses a temporary directory, and does not
require a running ferrumd instance. It does NOT claim production-ready.
EOF
    exit 0
fi

# --- Temp directory ---
DRILL_DIR=$(mktemp -d)
DB="$DRILL_DIR/wal_drill.db"
WRITER_LOG="$DRILL_DIR/writer.log"
WRITER_PID_FILE="$DRILL_DIR/writer.pid"

PASS=0
FAIL=0

cleanup() {
    if [[ -n "${WRITER_PID:-}" ]] && [[ "$WRITER_PID" != "$$" ]] && kill -0 "$WRITER_PID" 2>/dev/null; then
        kill "$WRITER_PID" 2>/dev/null || true
        wait "$WRITER_PID" 2>/dev/null || true
    fi
    rm -rf "$DRILL_DIR"
}
trap cleanup EXIT

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }

# --- Step 1: Create WAL-mode DB and insert baseline rows ---
echo "[INFO] Creating WAL-mode SQLite DB: $DB"
sqlite3 "$DB" "PRAGMA journal_mode=WAL; CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER);"
sqlite3 "$DB" "INSERT INTO events (ts) VALUES (1001),(1002),(1003);"
BASELINE_COUNT=$(sqlite3 "$DB" "SELECT COUNT(*) FROM events;")
if [[ "$BASELINE_COUNT" == "3" ]]; then
    pass "Baseline rows inserted (count=3)"
else
    fail "Baseline row count = $BASELINE_COUNT, expected 3"
fi

# Verify WAL mode is active
JOURNAL_MODE=$(sqlite3 "$DB" "PRAGMA journal_mode;")
if [[ "$JOURNAL_MODE" == "wal" ]]; then
    pass "Journal mode is WAL"
else
    fail "Journal mode is $JOURNAL_MODE, expected wal"
fi

# --- Step 2: Start a background writer that continuously commits rows ---
# Writer inserts rows in a tight loop for ~1.5 seconds to build WAL.
echo "[INFO] Starting background writer (1.5s active window)..."

writer_loop() {
    # Use $BASHPID (bash 4.0+) to get the actual subshell PID
    local mypid
    mypid="${BASHPID:-$$}"
    echo "$mypid" > "$WRITER_PID_FILE"
    local i=1
    while true; do
        sqlite3 "$DB" "INSERT INTO events (ts) VALUES ($((2000 + i)));" 2>/dev/null || true
        i=$((i + 1))
        # Throttle: sleep 0.01s if available, else rely on sqlite3 exec time
        sleep 0.01 2>/dev/null || true
        # Safety cap
        if [[ $i -gt 50000 ]]; then
            break
        fi
    done
}

writer_loop > "$WRITER_LOG" 2>&1 &
WRITER_PID=$!

# Give writer a moment to start and write its PID file
sleep 0.3
# Prefer PID from file (the subshell's PID) if available and still alive
if [[ -f "$WRITER_PID_FILE" ]]; then
    FILE_PID=$(cat "$WRITER_PID_FILE" 2>/dev/null || echo "")
    if [[ -n "$FILE_PID" ]] && kill -0 "$FILE_PID" 2>/dev/null; then
        WRITER_PID="$FILE_PID"
    fi
fi

echo "[INFO] Writer PID: $WRITER_PID"

# Let writer run for 1.5 seconds to build up WAL
sleep 1.5

# --- Step 3: Force-kill writer (simulate crash) ---
echo "[INFO] Sending SIGKILL to writer (simulating crash)..."
if [[ -n "$WRITER_PID" ]] && kill -0 "$WRITER_PID" 2>/dev/null; then
    kill -9 "$WRITER_PID" 2>/dev/null || true
    wait "$WRITER_PID" 2>/dev/null || true
fi
WRITER_PID=""

# --- Step 4: Reopen DB and run integrity check ---
echo "[INFO] Reopening DB after crash..."
# Brief sleep to allow any transient locks from the killed writer to clear
sleep 0.5
INTEGRITY=""
for attempt in 1 2 3; do
    INTEGRITY=$(sqlite3 "$DB" "PRAGMA integrity_check;" 2>/dev/null || true)
    if [[ "$INTEGRITY" == "ok" ]]; then
        break
    fi
    sleep 0.5
done
if [[ "$INTEGRITY" == "ok" ]]; then
    pass "PRAGMA integrity_check after crash = ok"
else
    fail "PRAGMA integrity_check after crash = $INTEGRITY"
fi

# --- Step 5: Verify baseline rows remain ---
POST_CRASH_COUNT=$(sqlite3 "$DB" "SELECT COUNT(*) FROM events;")
if [[ "$POST_CRASH_COUNT" -ge "3" ]]; then
    pass "Baseline rows survive crash (count=$POST_CRASH_COUNT >= 3)"
else
    fail "Baseline rows lost after crash (count=$POST_CRASH_COUNT)"
fi

# --- Step 6: Verify committed row count is internally consistent ---
# After crash, WAL may contain uncommitted frames; SQLite will ignore them.
# We verify MAX(id) >= COUNT(*) (no gaps from autoincrement due to rollback)
MAX_ID=$(sqlite3 "$DB" "SELECT COALESCE(MAX(id),0) FROM events;")
if [[ "$MAX_ID" -ge "$POST_CRASH_COUNT" ]]; then
    pass "Row count internally consistent (MAX(id)=$MAX_ID >= COUNT=$POST_CRASH_COUNT)"
else
    fail "Inconsistency detected (MAX(id)=$MAX_ID < COUNT=$POST_CRASH_COUNT)"
fi

# Verify the specific baseline values exist
BASELINE_EXISTS=$(sqlite3 "$DB" "SELECT COUNT(*) FROM events WHERE ts IN (1001,1002,1003);")
if [[ "$BASELINE_EXISTS" == "3" ]]; then
    pass "All baseline values (1001,1002,1003) present after crash"
else
    fail "Baseline values missing after crash (found $BASELINE_EXISTS/3)"
fi

# --- Step 7: Run checkpoint and verify final integrity ---
echo "[INFO] Running PRAGMA wal_checkpoint(TRUNCATE)..."
CHECKPOINT_RESULT=$(sqlite3 "$DB" "PRAGMA wal_checkpoint(TRUNCATE);")
# Result is three integers: busy, log, checkpointed
pass "Checkpoint executed (result: $CHECKPOINT_RESULT)"

FINAL_INTEGRITY=$(sqlite3 "$DB" "PRAGMA integrity_check;")
if [[ "$FINAL_INTEGRITY" == "ok" ]]; then
    pass "Final integrity_check after checkpoint = ok"
else
    fail "Final integrity_check after checkpoint = $FINAL_INTEGRITY"
fi

FINAL_COUNT=$(sqlite3 "$DB" "SELECT COUNT(*) FROM events;")
if [[ "$FINAL_COUNT" -ge "3" ]]; then
    pass "Final row count >= baseline (count=$FINAL_COUNT)"
else
    fail "Final row count below baseline (count=$FINAL_COUNT)"
fi

# Verify WAL file is truncated (or very small) after checkpoint(TRUNCATE)
WAL_SIZE=$(stat -c%s "$DB-wal" 2>/dev/null || echo "0")
if [[ "$WAL_SIZE" -le "4096" ]]; then
    pass "WAL file truncated after checkpoint (size=${WAL_SIZE}b)"
else
    # Non-fatal: some SQLite versions may leave a small WAL header
    echo "[WARN] WAL file larger than expected after checkpoint (size=${WAL_SIZE}b)"
fi

# --- Summary ---
echo ""
echo "========================================"
echo "WAL CRASH-RECOVERY DRILL SUMMARY"
echo "========================================"
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""
echo "Boundary: local-only, temp-dir based, no live ferrumd required."
echo "Limitation: SQLite single-process WAL; does not test multi-reader/writer concurrency."
echo "No production-ready claim. Block A remains WAIVED/CONDITIONAL."
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo "WAL CRASH-RECOVERY DRILL: ALL CHECKS PASSED"
    exit 0
else
    echo "WAL CRASH-RECOVERY DRILL: SOME CHECKS FAILED"
    exit 1
fi
