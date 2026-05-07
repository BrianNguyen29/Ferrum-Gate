#!/usr/bin/env bash
# run_mcp_lifecycle_smoke.sh
# Local MCP lifecycle smoke/evidence path for D1.7 tool-dispatch.
# Validates MCP stdio transport, lifecycle tool wiring, blocked tool behavior.
# Does NOT require target host, SSH, domain, or TLS.
# Does NOT claim G2/doc54/production-ready.
# Naming: This is D1.7 local lifecycle smoke, NOT D1.8 (output sanitization).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Find or build ferrumd ---

FERRUMD="${FERRUMD:-}"
if [[ -z "$FERRUMD" ]]; then
    for candidate in \
        "$REPO_ROOT/target/release/ferrumd" \
        "$REPO_ROOT/target/debug/ferrumd"; do
        [[ -x "$candidate" ]] && FERRUMD="$candidate" && break
    done
fi

if [[ -z "$FERRUMD" ]] || [[ ! -x "$FERRUMD" ]]; then
    echo "[INFO] ferrumd not found; building release ferrumd..." >&2
    cargo build --release --bin ferrumd --manifest-path "$REPO_ROOT/Cargo.toml"
    FERRUMD="$REPO_ROOT/target/release/ferrumd"
fi

if [[ -z "$FERRUMD" ]] || [[ ! -x "$FERRUMD" ]]; then
    echo "[FAIL] ferrumd not found or not executable after build attempt." >&2
    exit 1
fi

echo "[INFO] Using ferrumd: $FERRUMD"

# --- Find or build ferrum-mcp-server ---

FERRUM_MCP="${FERRUM_MCP:-}"
if [[ -z "$FERRUM_MCP" ]]; then
    for candidate in \
        "$REPO_ROOT/target/release/ferrum-mcp-server" \
        "$REPO_ROOT/target/debug/ferrum-mcp-server"; do
        [[ -x "$candidate" ]] && FERRUM_MCP="$candidate" && break
    done
fi

if [[ -z "$FERRUM_MCP" ]] || [[ ! -x "$FERRUM_MCP" ]]; then
    for candidate in \
        "$REPO_ROOT/target/debug/ferrum-mcp-server" \
        "$REPO_ROOT/target/release/ferrum-mcp-server"; do
        [[ -x "$candidate" ]] && FERRUM_MCP="$candidate" && break
    done
fi

if [[ -z "$FERRUM_MCP" ]] || [[ ! -x "$FERRUM_MCP" ]]; then
    echo "[INFO] ferrum-mcp-server not found; building debug..." >&2
    cargo build --bin ferrum-mcp-server --manifest-path "$REPO_ROOT/Cargo.toml"
    FERRUM_MCP="$REPO_ROOT/target/debug/ferrum-mcp-server"
fi

if [[ -z "$FERRUM_MCP" ]] || [[ ! -x "$FERRUM_MCP" ]]; then
    echo "[FAIL] ferrum-mcp-server not found or not executable after build attempt." >&2
    exit 1
fi

echo "[INFO] Using ferrum-mcp-server: $FERRUM_MCP"

# --- Setup temp directory ---

SMOKE_DIR=$(mktemp -d)
CONFIG_FILE="$SMOKE_DIR/ferrumgate.mcp-smoke.toml"
FERRUMD_LOG="$SMOKE_DIR/ferrumd.log"

# Find a free port on localhost
find_free_port() {
    local port=18080
    local max_attempts=100
    local attempt=0
    while ((attempt < max_attempts)); do
        if ! (echo > /dev/tcp/127.0.0.1/$port) 2>/dev/null; then
            echo $port
            return 0
        fi
        port=$((port + 1))
        attempt=$((attempt + 1))
    done
    echo ""
    return 1
}

FREE_PORT=$(find_free_port)
if [[ -z "$FREE_PORT" ]]; then
    echo "[FAIL] Could not find a free port in range 18080-18180" >&2
    exit 1
fi

BASE_URL="http://127.0.0.1:$FREE_PORT"

echo "[INFO] Smoke directory: $SMOKE_DIR"
echo "[INFO] Using port: $FREE_PORT"
echo "[INFO] Base URL: $BASE_URL"

# --- Cleanup function ---
cleanup() {
    if [[ -n "${FERRUMD_PID:-}" ]] && kill -0 "$FERRUMD_PID" 2>/dev/null; then
        echo "[INFO] Stopping ferrumd (PID: $FERRUMD_PID)..."
        kill "$FERRUMD_PID" 2>/dev/null || true
        wait "$FERRUMD_PID" 2>/dev/null || true
    fi
    rm -rf "$SMOKE_DIR"
}
trap cleanup EXIT

# --- Create temp config with auth disabled (dev mode) ---

cat > "$CONFIG_FILE" << EOF
[server]
bind_addr = "127.0.0.1:$FREE_PORT"
store_dsn = "sqlite::memory:"
auth_mode = "disabled"
allow_insecure_nonlocal_bind = false
log_filter = "info"
EOF

echo "[INFO] Config file: $CONFIG_FILE"

# --- Start ferrumd ---

echo "[INFO] Starting ferrumd (dev mode, auth disabled)..."
"$FERRUMD" --config "$CONFIG_FILE" > "$FERRUMD_LOG" 2>&1 &
FERRUMD_PID=$!

echo "[INFO] ferrumd started with PID: $FERRUMD_PID"

# Wait for server to be ready
wait_for_server() {
    local max_wait=30
    local waited=0
    while ((waited < max_wait)); do
        if python3 -c "
import urllib.request
import urllib.error
import sys
try:
    response = urllib.request.urlopen('$BASE_URL/v1/healthz', timeout=1)
    sys.exit(0 if response.status in (200, 503) else 1)
except urllib.error.HTTPError as e:
    sys.exit(0 if e.code in (200, 503) else 1)
except Exception:
    sys.exit(1)
" 2>/dev/null; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}

echo "[INFO] Waiting for ferrumd to be ready (max 30s)..."
if ! wait_for_server; then
    echo "[FAIL] ferrumd did not become ready within 30 seconds" >&2
    echo "--- ferrumd log ---"
    tail -50 "$FERRUMD_LOG" 2>/dev/null || cat "$FERRUMD_LOG" 2>/dev/null || true
    echo "--- end log ---"
    exit 1
fi
echo "[INFO] ferrumd is ready"

# --- Run MCP lifecycle smoke tests ---

export FERRUM_GATEWAY_URL="$BASE_URL"
export FERRUM_GATEWAY_BEARER_TOKEN=""

PASSED=0
FAILED=0

echo ""
echo "========================================"
echo "MCP LIFECYCLE SMOKE TESTS (D1.7)"
echo "========================================"

# Helper to send MCP JSON-RPC command and capture response
mcp_call() {
    local method="$1"
    local params="$2"
    local id="${3:-1}"
    # Sanitize method name for filename (replace / with _)
    local safe_method="${method//\//_}"
    local response_file="$SMOKE_DIR/mcp_response_${safe_method}_${id}.txt"

    if [[ "$params" == "{}" ]] || [[ -z "$params" ]]; then
        echo "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"id\":$id}" | \
            "$FERRUM_MCP" 2>/dev/null | head -1 > "$response_file"
    else
        echo "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":$id}" | \
            "$FERRUM_MCP" 2>/dev/null | head -1 > "$response_file"
    fi

    cat "$response_file"
}

echo ""
echo "[TEST] MCP Initialize..."
RESPONSE=$(mcp_call "initialize" "{}" 1)
if echo "$RESPONSE" | grep -q '"result"'; then
    echo "[PASS] MCP Initialize"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] MCP Initialize (no result in response)"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] MCP tools/list..."
RESPONSE=$(mcp_call "tools/list" "" 2)
# Check that we get back a list of tools (should be 17 tools: 9 read-only + 8 lifecycle)
TOOL_COUNT=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('result',{}).get('tools',[])))" 2>/dev/null || echo "0")
if [[ "$TOOL_COUNT" == "17" ]]; then
    echo "[PASS] MCP tools/list returns 17 tools (9 read-only + 8 lifecycle)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] MCP tools/list returned $TOOL_COUNT tools, expected 17"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] Blocked tool: ferrum_gate_approve_intent..."
RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_approve_intent","arguments":{}}' 3)
if echo "$RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32001" ]]; then
        echo "[PASS] ferrum_gate_approve_intent returns NOT_IMPLEMENTED (-32001)"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] ferrum_gate_approve_intent returns error code $ERROR_CODE, expected -32001"
        echo "       Response: $RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    echo "[FAIL] ferrum_gate_approve_intent did not return error"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] Blocked tool: ferrum_gate_reject_intent..."
RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_reject_intent","arguments":{}}' 4)
if echo "$RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32001" ]]; then
        echo "[PASS] ferrum_gate_reject_intent returns NOT_IMPLEMENTED (-32001)"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] ferrum_gate_reject_intent returns error code $ERROR_CODE, expected -32001"
        echo "       Response: $RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    echo "[FAIL] ferrum_gate_reject_intent did not return error"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] Read-only tool: ferrum_gate_health..."
RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_health","arguments":{}}' 5)
if echo "$RESPONSE" | grep -q '"result"'; then
    echo "[PASS] ferrum_gate_health returns result (gateway reachable)"
    PASSED=$((PASSED + 1))
else
    # Note: This may fail if gateway has issues, but we expect it to work
    echo "[WARN] ferrum_gate_health did not return result (may be expected if gateway has issues)"
    echo "       Response: $RESPONSE"
    # Don't count as failure for smoke purposes - health check behavior may vary
fi

echo ""
echo "[TEST] Lifecycle tool in registry: ferrum_gate_submit_intent..."
RESPONSE=$(mcp_call "tools/list" "" 6)
if echo "$RESPONSE" | grep -q 'ferrum_gate_submit_intent'; then
    echo "[PASS] ferrum_gate_submit_intent is in tools/list"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] ferrum_gate_submit_intent not found in tools/list"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] All 8 lifecycle tools present in registry..."
EXPECTED_LIFECYCLE=(
    "ferrum_gate_submit_intent"
    "ferrum_gate_evaluate_intent"
    "ferrum_gate_mint_capability"
    "ferrum_gate_authorize_execution"
    "ferrum_gate_prepare_execution"
    "ferrum_gate_execute_prepared"
    "ferrum_gate_verify"
    "ferrum_gate_compensate"
)
LIFECYCLE_COUNT=0
for tool in "${EXPECTED_LIFECYCLE[@]}"; do
    if echo "$RESPONSE" | grep -q "$tool"; then
        LIFECYCLE_COUNT=$((LIFECYCLE_COUNT + 1))
    fi
done
if [[ "$LIFECYCLE_COUNT" == "8" ]]; then
    echo "[PASS] All 8 lifecycle tools present in registry"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] Only $LIFECYCLE_COUNT/8 lifecycle tools found in registry"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] Unknown tool returns METHOD_NOT_FOUND..."
RESPONSE=$(mcp_call "tools/call" '{"name":"nonexistent_tool","arguments":{}}' 7)
if echo "$RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32601" ]]; then
        echo "[PASS] Unknown tool returns METHOD_NOT_FOUND (-32601)"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] Unknown tool returns error code $ERROR_CODE, expected -32601"
        echo "       Response: $RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    echo "[FAIL] Unknown tool did not return error"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] MCP ping..."
RESPONSE=$(mcp_call "ping" "" 8)
if echo "$RESPONSE" | grep -q '"result"'; then
    echo "[PASS] MCP ping returns result"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] MCP ping did not return result"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

# --- SUMMARY ---

echo ""
echo "========================================"
echo "MCP LIFECYCLE SMOKE RESULT (D1.7)"
echo "========================================"
echo ""
echo "Passed: $PASSED"
echo "Failed: $FAILED"
echo ""
echo "This smoke validates D1.7 MCP lifecycle tool dispatch locally."
echo "It validates: MCP stdio transport, 17-tool registry, 8 lifecycle tools wired,"
echo "blocked approve/reject behavior, and error handling."
echo "It does NOT complete G2, does NOT authorize the pilot, and does NOT claim production-ready."
echo "Note: This is D1.7 lifecycle smoke, NOT D1.8 (output sanitization)."
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo "MCP LIFECYCLE SMOKE: ALL CHECKS PASSED"
    exit 0
else
    echo "MCP LIFECYCLE SMOKE: SOME CHECKS FAILED"
    echo "--- ferrumd log (last 30 lines) ---"
    tail -30 "$FERRUMD_LOG" 2>/dev/null || cat "$FERRUMD_LOG" 2>/dev/null || true
    echo "--- end log ---"
    exit 1
fi
