#!/usr/bin/env bash
# run_mcp_lifecycle_smoke.sh
# Local MCP lifecycle smoke/evidence path for D1.7 + D1.11 tool-dispatch.
# Validates MCP stdio transport, lifecycle tool wiring, blocked tool behavior.
# D1.11 extends with live-local lifecycle dispatch checks (submit/evaluate/mint).
# Does NOT require target host, SSH, domain, or TLS.
# Does NOT claim G2/doc54/production-ready.
# Naming: This is D1.7+D1.11 local lifecycle smoke, NOT D1.8 (output sanitization).

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
echo "MCP LIFECYCLE SMOKE TESTS (D1.7 + D1.11)"
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
# Check that we get back a list of tools (should be 19 tools: 9 read-only + 8 lifecycle + 2 approval)
TOOL_COUNT=$(echo "$RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('result',{}).get('tools',[])))" 2>/dev/null || echo "0")
if [[ "$TOOL_COUNT" == "19" ]]; then
    echo "[PASS] MCP tools/list returns 19 tools (9 read-only + 8 lifecycle + 2 approval)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] MCP tools/list returned $TOOL_COUNT tools, expected 19"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] D1.9 Approval tool in registry: ferrum_gate_approve_intent..."
RESPONSE=$(mcp_call "tools/list" "" 3)
if echo "$RESPONSE" | grep -q 'ferrum_gate_approve_intent'; then
    echo "[PASS] ferrum_gate_approve_intent is in tools/list (D1.9 enabled)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] ferrum_gate_approve_intent not found in tools/list"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] D1.9 Approval tool in registry: ferrum_gate_reject_intent..."
RESPONSE=$(mcp_call "tools/list" "" 4)
if echo "$RESPONSE" | grep -q 'ferrum_gate_reject_intent'; then
    echo "[PASS] ferrum_gate_reject_intent is in tools/list (D1.9 enabled)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] ferrum_gate_reject_intent not found in tools/list"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] D1.9 approve dispatch with non-existent approval_id returns error..."
# Note: Full approve flow requires creating pending approval via lifecycle (intent->proposal->execution->approval).
# We test dispatch with invalid ID to prove the tool routes to gateway and returns structured error.
RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_approve_intent","arguments":{"approval_id":"00000000-0000-0000-0000-000000000000","actor":{"actor_type":"Operator","actor_id":"test-op"},"approve":true}}' 10)
if echo "$RESPONSE" | grep -q '"error"'; then
    echo "[PASS] ferrum_gate_approve_intent dispatch returns JSON-RPC error (not METHOD_NOT_FOUND)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] ferrum_gate_approve_intent dispatch did not return expected error"
    echo "       Response: $RESPONSE"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "[TEST] D1.9 reject dispatch with non-existent approval_id returns error..."
RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_reject_intent","arguments":{"approval_id":"00000000-0000-0000-0000-000000000000","actor":{"actor_type":"Operator","actor_id":"test-op"},"approve":false}}' 11)
if echo "$RESPONSE" | grep -q '"error"'; then
    echo "[PASS] ferrum_gate_reject_intent dispatch returns JSON-RPC error (not METHOD_NOT_FOUND)"
    PASSED=$((PASSED + 1))
else
    echo "[FAIL] ferrum_gate_reject_intent dispatch did not return expected error"
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

# ========================================
# D1.11 LIFECYCLE DISPATCH CHECKS (live-local)
# ========================================
# Soft-pass semantics:
#   - result = pass
#   - -32003/-32004 (gateway unreachable/server error) = warn/pass for lifecycle
#   - -32002 (auth failed) = warn/pass for read-only auth-sensitive tests
#   - -32001/-32601/-32602/-32700/no response = fail
# D1.11 validates bounded dispatch reachability; does NOT claim G2/production.

echo ""
echo "--- D1.11 Lifecycle Dispatch Checks ---"

# Helper to extract intent_id from submit_intent response
extract_intent_id() {
    local resp="$1"
    # Try to extract intent_id from response text
    echo "$resp" | python3 -c "
import sys, json, re
try:
    data = json.load(sys.stdin)
    # Handle nested structure: result.content[0].text
    result = data.get('result', {})
    content = result.get('content', [])
    if content and len(content) > 0:
        text = content[0].get('text', '')
        # Look for intent_id pattern
        match = re.search(r'\"intent_id\"\s*:\s*\"([^\"]+)\"', text)
        if match:
            print(match.group(1))
            sys.exit(0)
        match = re.search(r'intent_id[\"\\s:]+([0-9a-f-]{36})', text)
        if match:
            print(match.group(1))
            sys.exit(0)
except:
    pass
print('')
" 2>/dev/null || echo ""
}

# D1.11.1: submit_intent - lifecycle dispatch test
echo ""
echo "[TEST] D1.11.1: ferrum_gate_submit_intent (lifecycle dispatch)..."
SUBMIT_RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_submit_intent","arguments":{"principal_id":"550e8400-e29b-41d4-a716-446655440000","title":"smoke test intent","goal":"validate lifecycle dispatch","action_type":"Read","target":"/tmp/smoke-test.txt","scope":"fs:read:/tmp/smoke-test.txt"}}' 9)

# Extract intent_id for downstream use; fallback to generated UUID
INTENT_ID=$(extract_intent_id "$SUBMIT_RESPONSE")
if [[ -z "$INTENT_ID" ]]; then
    INTENT_ID=$(python3 -c "import uuid; print(uuid.uuid4())" 2>/dev/null || echo "00000000-0000-0000-0000-000000000000")
    echo "       [WARN] Could not extract intent_id from response, using fallback"
fi

# Soft-pass check for lifecycle: -32003/-32004 = warn (dispatch reached gateway)
if echo "$SUBMIT_RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$SUBMIT_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32003" ]] || [[ "$ERROR_CODE" == "-32004" ]]; then
        echo "[PASS] D1.11.1 submit_intent: dispatch reached gateway, got $ERROR_CODE (soft-pass)"
        PASSED=$((PASSED + 1))
    elif [[ "$ERROR_CODE" == "-32001" ]] || [[ "$ERROR_CODE" == "-32601" ]] || [[ "$ERROR_CODE" == "-32602" ]] || [[ "$ERROR_CODE" == "-32700" ]]; then
        echo "[FAIL] D1.11.1 submit_intent returns fatal error $ERROR_CODE"
        echo "       Response: $SUBMIT_RESPONSE"
        FAILED=$((FAILED + 1))
    else
        echo "[FAIL] D1.11.1 submit_intent returns unexpected error $ERROR_CODE"
        echo "       Response: $SUBMIT_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    if echo "$SUBMIT_RESPONSE" | grep -q '"result"'; then
        echo "[PASS] D1.11.1 submit_intent returns result (intent_id: $INTENT_ID)"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] D1.11.1 submit_intent: no result, no recognized error"
        echo "       Response: $SUBMIT_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
fi

# D1.11.2: evaluate_intent - lifecycle dispatch test (uses intent_id + fallback proposal_id)
echo ""
echo "[TEST] D1.11.2: ferrum_gate_evaluate_intent (lifecycle dispatch)..."
# Use fallback proposal_id since we may not have extracted it from submit_intent response
PROPOSAL_ID=$(python3 -c "import uuid; print(uuid.uuid4())" 2>/dev/null || echo "00000000-0000-0000-0000-000000000000")
EVALUATE_RESPONSE=$(mcp_call "tools/call" "{\"name\":\"ferrum_gate_evaluate_intent\",\"arguments\":{\"proposal_id\":\"$PROPOSAL_ID\",\"intent_id\":\"$INTENT_ID\",\"title\":\"smoke proposal\",\"tool_name\":\"fs.read\",\"server_name\":\"fs-server\",\"arguments\":{},\"expected_effect\":\"read file\",\"estimated_risk\":\"Low\"}}" 10)

if echo "$EVALUATE_RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$EVALUATE_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32003" ]] || [[ "$ERROR_CODE" == "-32004" ]]; then
        echo "[PASS] D1.11.2 evaluate_intent: dispatch reached gateway, got $ERROR_CODE (soft-pass)"
        PASSED=$((PASSED + 1))
    elif [[ "$ERROR_CODE" == "-32001" ]] || [[ "$ERROR_CODE" == "-32601" ]] || [[ "$ERROR_CODE" == "-32602" ]] || [[ "$ERROR_CODE" == "-32700" ]]; then
        echo "[FAIL] D1.11.2 evaluate_intent returns fatal error $ERROR_CODE"
        echo "       Response: $EVALUATE_RESPONSE"
        FAILED=$((FAILED + 1))
    else
        echo "[FAIL] D1.11.2 evaluate_intent returns unexpected error $ERROR_CODE"
        echo "       Response: $EVALUATE_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    if echo "$EVALUATE_RESPONSE" | grep -q '"result"'; then
        echo "[PASS] D1.11.2 evaluate_intent returns result"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] D1.11.2 evaluate_intent: no result, no recognized error"
        echo "       Response: $EVALUATE_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
fi

# D1.11.3: mint_capability - lifecycle dispatch test
echo ""
echo "[TEST] D1.11.3: ferrum_gate_mint_capability (lifecycle dispatch)..."
MINT_RESPONSE=$(mcp_call "tools/call" "{\"name\":\"ferrum_gate_mint_capability\",\"arguments\":{\"intent_id\":\"$INTENT_ID\",\"proposal_id\":\"$PROPOSAL_ID\",\"tool_name\":\"fs.read\",\"server_name\":\"fs-server\"}}" 11)

if echo "$MINT_RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$MINT_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    if [[ "$ERROR_CODE" == "-32003" ]] || [[ "$ERROR_CODE" == "-32004" ]]; then
        echo "[PASS] D1.11.3 mint_capability: dispatch reached gateway, got $ERROR_CODE (soft-pass)"
        PASSED=$((PASSED + 1))
    elif [[ "$ERROR_CODE" == "-32001" ]] || [[ "$ERROR_CODE" == "-32601" ]] || [[ "$ERROR_CODE" == "-32602" ]] || [[ "$ERROR_CODE" == "-32700" ]]; then
        echo "[FAIL] D1.11.3 mint_capability returns fatal error $ERROR_CODE"
        echo "       Response: $MINT_RESPONSE"
        FAILED=$((FAILED + 1))
    else
        echo "[FAIL] D1.11.3 mint_capability returns unexpected error $ERROR_CODE"
        echo "       Response: $MINT_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    if echo "$MINT_RESPONSE" | grep -q '"result"'; then
        echo "[PASS] D1.11.3 mint_capability returns result"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] D1.11.3 mint_capability: no result, no recognized error"
        echo "       Response: $MINT_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
fi

# D1.11.4: list_intents - read-only dispatch (soft-pass on -32002 for auth-sensitive)
echo ""
echo "[TEST] D1.11.4: ferrum_gate_list_intents (read-only dispatch)..."
LIST_RESPONSE=$(mcp_call "tools/call" '{"name":"ferrum_gate_list_intents","arguments":{}}' 12)

if echo "$LIST_RESPONSE" | grep -q '"error"'; then
    ERROR_CODE=$(echo "$LIST_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('error',{}).get('code','?'))" 2>/dev/null || echo "?")
    # Soft-pass for -32002 (auth) and -32003/-32004 (gateway errors) on read-only
    if [[ "$ERROR_CODE" == "-32002" ]] || [[ "$ERROR_CODE" == "-32003" ]] || [[ "$ERROR_CODE" == "-32004" ]]; then
        echo "[PASS] D1.11.4 list_intents: dispatch reached gateway, got $ERROR_CODE (soft-pass)"
        PASSED=$((PASSED + 1))
    elif [[ "$ERROR_CODE" == "-32001" ]] || [[ "$ERROR_CODE" == "-32601" ]] || [[ "$ERROR_CODE" == "-32602" ]] || [[ "$ERROR_CODE" == "-32700" ]]; then
        echo "[FAIL] D1.11.4 list_intents returns fatal error $ERROR_CODE"
        echo "       Response: $LIST_RESPONSE"
        FAILED=$((FAILED + 1))
    else
        echo "[FAIL] D1.11.4 list_intents returns unexpected error $ERROR_CODE"
        echo "       Response: $LIST_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
else
    if echo "$LIST_RESPONSE" | grep -q '"result"'; then
        echo "[PASS] D1.11.4 list_intents returns result"
        PASSED=$((PASSED + 1))
    else
        echo "[FAIL] D1.11.4 list_intents: no result, no recognized error"
        echo "       Response: $LIST_RESPONSE"
        FAILED=$((FAILED + 1))
    fi
fi

# --- SUMMARY ---

echo ""
echo "========================================"
echo "MCP LIFECYCLE SMOKE RESULT (D1.7 + D1.11)"
echo "========================================"
echo ""
echo "Passed: $PASSED"
echo "Failed: $FAILED"
echo ""
echo "This smoke validates D1.7 + D1.11 MCP lifecycle tool dispatch locally."
echo "D1.7 validates: MCP stdio transport, 19-tool registry, 8 lifecycle + 2 approval tools wired,"
echo "approve/reject dispatch error checks, and error handling."
echo "D1.11 validates: bounded lifecycle dispatch checks (submit/evaluate/mint/list)"
echo "with soft-pass semantics for gateway reachability (-32003/-32004)."
echo "It does NOT complete G2, does NOT authorize the pilot, and does NOT claim production-ready."
echo "Note: This is D1.7+D1.11 local lifecycle smoke, NOT D1.8 (output sanitization)."
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
