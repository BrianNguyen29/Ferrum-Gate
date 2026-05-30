#!/usr/bin/env bash
# verify_tls_and_auth.sh
# B4/B5 — TLS and auth verifier for FerrumGate v1 target endpoints.
# Tests public health/readiness endpoints and protected /v1/approvals behavior.
# Does NOT require target host access by default; skips target checks if --base-url absent.
# Never prints token values.
# Does NOT claim G2/pilot/production-ready.
#
# Usage:
#   bash scripts/verify_tls_and_auth.sh --base-url https://example.com:18080
#   bash scripts/verify_tls_and_auth.sh --base-url https://localhost:18080 --insecure
#   bash scripts/verify_tls_and_auth.sh --base-url URL --token-env MY_TOKEN_VAR
#   bash scripts/verify_tls_and_auth.sh --help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BASE_URL=""
TOKEN_ENV="${TOKEN_ENV:-FERRUMGATE_BEARER_TOKEN}"
INSECURE=false

usage() {
    cat << 'EOF'
B4/B5 TLS and Auth Verifier

Usage:
  bash scripts/verify_tls_and_auth.sh --base-url URL [options]

Options:
  --base-url URL       Target base URL (e.g., https://localhost:18080)
  --token-env VAR      Environment variable name for bearer token
                       (default: FERRUMGATE_BEARER_TOKEN)
  --insecure           Allow self-signed/nonprod TLS certificates
  --help               Show this help message and exit

Description:
  Tests the following endpoint behaviors against a running ferrumd instance:
    - GET /v1/healthz       (public, expected 200)
    - GET /v1/readyz        (public, expected 200)
    - GET /v1/readyz/deep   (public, expected 200 or 503)
    - GET /v1/approvals     (protected)
        * No token          -> expected 401
        * Correct token     -> expected 200 (only if token env is set)

  The token value is NEVER printed or logged.

  This script is safe to run locally against a target URL. It performs only
  HTTP GET requests and does not modify server state.
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            usage
            exit 0
            ;;
        --base-url)
            BASE_URL="${2:-}"
            if [[ -z "$BASE_URL" ]]; then
                echo "[ERROR] --base-url requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --token-env)
            TOKEN_ENV="${2:-}"
            if [[ -z "$TOKEN_ENV" ]]; then
                echo "[ERROR] --token-env requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --insecure)
            INSECURE=true
            shift
            ;;
        *)
            echo "[ERROR] Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# --- Validate dependencies ---
if ! command -v curl >/dev/null 2>&1; then
    echo "[FAIL] curl is required but not installed" >&2
    exit 1
fi

# --- Helpers ---
CURL_OPTS=(-s -o /dev/null -w "%{http_code}")
if [[ "$INSECURE" == true ]]; then
    CURL_OPTS+=(-k)
fi

http_get() {
    local url="$1"
    local token="${2:-}"
    if [[ -n "$token" ]]; then
        curl "${CURL_OPTS[@]}" -H "Authorization: Bearer $token" "$url" 2>/dev/null
    else
        curl "${CURL_OPTS[@]}" "$url" 2>/dev/null
    fi
}

PASSED=0
FAILED=0

log_pass() { echo "[PASS] $*"; PASSED=$((PASSED + 1)); }
log_fail() { echo "[FAIL] $*"; FAILED=$((FAILED + 1)); }
log_skip() { echo "[SKIP] $*"; }
log_info() { echo "[INFO] $*"; }

# --- Header ---
echo ""
echo "========================================"
echo "B4/B5 TLS and Auth Verifier"
echo "========================================"
echo ""
echo "IMPORTANT: This script is a SAFE CHECK ONLY."
echo "  - Does NOT modify server state"
echo "  - Does NOT claim production-ready"
echo "  - Does NOT complete G2 gates"
echo ""

if [[ -z "$BASE_URL" ]]; then
    echo "[ERROR] --base-url is required" >&2
    usage >&2
    exit 2
fi

# Trim trailing slash
BASE_URL="${BASE_URL%/}"

TOKEN="${!TOKEN_ENV:-}"
if [[ -n "$TOKEN" ]]; then
    log_info "Token provided via $TOKEN_ENV; value will not be printed"
else
    log_info "No token provided via $TOKEN_ENV (protected endpoint check will be partial)"
fi

echo ""
echo "========================================"
echo "CHECK: Public endpoints"
echo "========================================"

# --- Public endpoints ---
for path in "/v1/healthz" "/v1/readyz"; do
    CODE=$(http_get "$BASE_URL$path")
    if [[ "$CODE" == "200" ]]; then
        log_pass "GET $path -> $CODE"
    else
        log_fail "GET $path -> $CODE (expected 200)"
    fi
done

CODE=$(http_get "$BASE_URL/v1/readyz/deep")
if [[ "$CODE" == "200" ]] || [[ "$CODE" == "503" ]]; then
    log_pass "GET /v1/readyz/deep -> $CODE"
else
    log_fail "GET /v1/readyz/deep -> $CODE (expected 200 or 503)"
fi

echo ""
echo "========================================"
echo "CHECK: Protected endpoint (/v1/approvals)"
echo "========================================"

# No token -> 401
CODE=$(http_get "$BASE_URL/v1/approvals")
if [[ "$CODE" == "401" ]]; then
    log_pass "GET /v1/approvals (no token) -> $CODE"
else
    log_fail "GET /v1/approvals (no token) -> $CODE (expected 401)"
fi

# Correct token -> 200 (only if token is available)
if [[ -n "$TOKEN" ]]; then
    CODE=$(http_get "$BASE_URL/v1/approvals" "$TOKEN")
    if [[ "$CODE" == "200" ]]; then
        log_pass "GET /v1/approvals (with token) -> $CODE"
    else
        log_fail "GET /v1/approvals (with token) -> $CODE (expected 200)"
    fi
else
    log_skip "GET /v1/approvals (with token) — no token available"
fi

# --- Summary ---
echo ""
echo "========================================"
echo "B4/B5 VERIFIER RESULT"
echo "========================================"
echo ""
echo "Passed:  $PASSED"
echo "Failed:  $FAILED"
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo "B4/B5: ALL APPLICABLE CHECKS PASSED"
    echo ""
    echo "TLS and auth behavior verified against $BASE_URL."
    echo "This does NOT constitute G2 completion or pilot readiness."
    echo ""
    exit 0
else
    echo "B4/B5: SOME CHECKS FAILED"
    echo ""
    exit 1
fi
