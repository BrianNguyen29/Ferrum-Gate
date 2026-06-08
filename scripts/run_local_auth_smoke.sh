#!/usr/bin/env bash
# run_local_auth_smoke.sh
# Local bearer-auth smoke check for FerrumGate v1.
# Validates that bearer auth works correctly on a temporary local instance.
# Does NOT require target host, SSH, domain, or TLS.
# Does NOT claim G2/doc54/production-ready.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Find ferrumd binary ---

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

# --- Find curl (optional) ---

CURL="$(command -v curl 2>/dev/null || true)"
HAVE_CURL=false
if [[ -n "$CURL" ]] && [[ -x "$CURL" ]]; then
    HAVE_CURL=true
    echo "[INFO] curl available: $CURL"
else
    echo "[INFO] curl not available; will use python3 urllib"
fi

# --- Setup temp directory and port ---

SMOKE_DIR=$(mktemp -d)
CONFIG_FILE="$SMOKE_DIR/ferrumgate.auth-smoke.toml"
STORE_DB="$SMOKE_DIR/ferrumgate.auth-smoke.db"

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
SMOKE_TOKEN="smoke-test-token-$(openssl rand -hex 16 2>/dev/null || python3 -c 'import secrets; print(secrets.token_hex(16))')"

echo "[INFO] Smoke directory: $SMOKE_DIR"
echo "[INFO] Using port: $FREE_PORT"
echo "[INFO] Base URL: $BASE_URL"

# Cleanup function
cleanup() {
    if [[ -n "${FERRUMD_PID:-}" ]] && kill -0 "$FERRUMD_PID" 2>/dev/null; then
        echo "[INFO] Stopping ferrumd (PID: $FERRUMD_PID)..."
        kill "$FERRUMD_PID" 2>/dev/null || true
        wait "$FERRUMD_PID" 2>/dev/null || true
    fi
    rm -rf "$SMOKE_DIR"
}
trap cleanup EXIT

# --- Create temp config with bearer auth ---

cat > "$CONFIG_FILE" << EOF
[server]
bind_addr = "127.0.0.1:$FREE_PORT"
store_dsn = "sqlite::memory:"
auth_mode = "bearer"
bearer_token = "$SMOKE_TOKEN"
allow_insecure_nonlocal_bind = false
log_filter = "info"
EOF

echo "[INFO] Config file: $CONFIG_FILE"

# --- Start ferrumd ---

echo "[INFO] Starting ferrumd with bearer auth..."
FERRUMD_LOG="$SMOKE_DIR/ferrumd.log"
FERRUMD_AUTH_MODE=bearer \
FERRUMD_BEARER_TOKEN="$SMOKE_TOKEN" \
"$FERRUMD" --config "$CONFIG_FILE" > "$FERRUMD_LOG" 2>&1 &
FERRUMD_PID=$!

echo "[INFO] ferrumd started with PID: $FERRUMD_PID"

# Wait for server to be ready
wait_for_server() {
    local max_wait=30
    local waited=0
    while ((waited < max_wait)); do
        if [[ "$HAVE_CURL" == true ]]; then
            if curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/v1/healthz" 2>/dev/null | grep -q "200\|503"; then
                return 0
            fi
        fi
        # Also check via python if curl is not available.
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

# --- Helper function for HTTP checks ---

check_endpoint() {
    local method="$1"
    local path="$2"
    local expected_code="$3"
    local description="$4"
    local auth_header="${5:-}"

    local http_code
    local response_file="$SMOKE_DIR/response_${method}_${path//\//_}.txt"

    if [[ "$HAVE_CURL" == true ]]; then
        if [[ -n "$auth_header" ]]; then
            http_code=$(curl -s -o "$response_file" -w "%{http_code}" \
                -X "$method" \
                -H "Authorization: $auth_header" \
                "$BASE_URL$path" 2>/dev/null)
        else
            http_code=$(curl -s -o "$response_file" -w "%{http_code}" \
                -X "$method" \
                "$BASE_URL$path" 2>/dev/null)
        fi
    else
        # Use python urllib
        if [[ -n "$auth_header" ]]; then
            http_code=$(python3 -c "
import urllib.request
import urllib.error
try:
    req = urllib.request.Request('$BASE_URL$path', method='$method')
    req.add_header('Authorization', '$auth_header')
    urllib.request.urlopen(req, timeout=5)
    print('200')
except urllib.error.HTTPError as e:
    print(e.code)
except Exception as e:
    print('000')
" 2>/dev/null)
        else
            http_code=$(python3 -c "
import urllib.request
import urllib.error
try:
    req = urllib.request.Request('$BASE_URL$path', method='$method')
    urllib.request.urlopen(req, timeout=5)
    print('200')
except urllib.error.HTTPError as e:
    print(e.code)
except Exception as e:
    print('000')
" 2>/dev/null)
        fi
    fi

    if [[ "$http_code" == "$expected_code" ]]; then
        echo "[PASS] $description (got $http_code)"
        return 0
    else
        echo "[FAIL] $description (expected $expected_code, got $http_code)"
        echo "       Response: $(cat "$response_file" 2>/dev/null | head -5 || echo 'N/A')"
        return 1
    fi
}

# --- Test counters ---

PASSED=0
FAILED=0

# --- PUBLIC ENDPOINTS (should work without auth) ---

echo ""
echo "========================================"
echo "TESTING PUBLIC ENDPOINTS (no auth)"
echo "========================================"

if check_endpoint "GET" "/v1/healthz" "200" "GET /v1/healthz (no auth)"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

if check_endpoint "GET" "/v1/readyz" "200" "GET /v1/readyz (no auth)"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

if check_endpoint "GET" "/v1/readyz/deep" "401" "GET /v1/readyz/deep (no auth)"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

if check_endpoint "GET" "/v1/metrics" "401" "GET /v1/metrics (no auth)"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

if check_endpoint "GET" "/v1/readyz/deep" "200" "GET /v1/readyz/deep (correct token)" "Bearer $SMOKE_TOKEN"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

if check_endpoint "GET" "/v1/metrics" "200" "GET /v1/metrics (correct token)" "Bearer $SMOKE_TOKEN"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

# --- PROTECTED ENDPOINT /v1/approvals ---

echo ""
echo "========================================"
echo "TESTING PROTECTED ENDPOINT (/v1/approvals)"
echo "========================================"

# Test 1: No token (should get 401)
if check_endpoint "GET" "/v1/approvals" "401" "GET /v1/approvals (no token)"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

# Test 2: Wrong token (should get 401)
if check_endpoint "GET" "/v1/approvals" "401" "GET /v1/approvals (wrong token)" "Bearer wrong-token-12345"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

# Test 3: Correct token (should get 200)
if check_endpoint "GET" "/v1/approvals" "200" "GET /v1/approvals (correct token)" "Bearer $SMOKE_TOKEN"; then
    PASSED=$((PASSED + 1))
else
    FAILED=$((FAILED + 1))
fi

# --- SUMMARY ---

echo ""
echo "========================================"
echo "AUTH SMOKE RESULT"
echo "========================================"
echo ""
echo "Passed: $PASSED"
echo "Failed: $FAILED"
echo ""
echo "This smoke validates bearer auth locally in a temp environment."
echo "It does NOT complete G2, does NOT authorize the pilot, and does NOT claim production-ready."
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo "AUTH SMOKE: ALL CHECKS PASSED"
    exit 0
else
    echo "AUTH SMOKE: SOME CHECKS FAILED"
    echo "--- ferrumd log (last 30 lines) ---"
    tail -30 "$FERRUMD_LOG" 2>/dev/null || cat "$FERRUMD_LOG" 2>/dev/null || true
    echo "--- end log ---"
    exit 1
fi
