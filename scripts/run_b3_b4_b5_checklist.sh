#!/usr/bin/env bash
# run_b3_b4_b5_checklist.sh
# Composes safe B3/B4/B5 checks for FerrumGate v1 single-node SQLite.
# Runs local retention pruning test (B3) and optionally the TLS/auth verifier (B4/B5)
# if --base-url is provided. Prints evidence checklist and blockers.
# Does NOT require target host, SSH, domain, or real secrets by default.
# Does NOT claim G2/pilot/production-ready.
#
# Usage:
#   bash scripts/run_b3_b4_b5_checklist.sh
#   bash scripts/run_b3_b4_b5_checklist.sh --base-url https://localhost:18080
#   bash scripts/run_b3_b4_b5_checklist.sh --base-url URL --token-env VAR --insecure
#   bash scripts/run_b3_b4_b5_checklist.sh --help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BASE_URL=""
TOKEN_ENV="${TOKEN_ENV:-FERRUMGATE_BEARER_TOKEN}"
INSECURE=false

usage() {
    cat << 'EOF'
B3/B4/B5 Checklist Runner

Usage:
  bash scripts/run_b3_b4_b5_checklist.sh [options]

Options:
  --base-url URL       Target base URL for B4/B5 verifier (optional)
  --token-env VAR      Environment variable name for bearer token
                       (default: FERRUMGATE_BEARER_TOKEN)
  --insecure           Allow self-signed/nonprod TLS certificates
  --help               Show this help message and exit

Description:
  This script composes safe local checks for B3/B4/B5 preparation:

  B3 — Local retention pruning test (always run):
       Creates a temp SQLite DB, seeds old backups, runs ferrumctl backup create
       with --retention-days, and asserts prune/keep/preserve behavior.

  B4/B5 — TLS and auth verifier (optional, only if --base-url provided):
       Tests public endpoints (/v1/healthz, /v1/readyz, /v1/readyz/deep)
       and protected endpoint behavior (no token -> 401, correct token -> 200).

  The checklist output distinguishes between local evidence gathered and
  operator-owned blockers that remain before pilot readiness.
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

PASSED=0
FAILED=0
SKIPPED=0

log_pass() { echo "[PASS] $*"; PASSED=$((PASSED + 1)); }
log_fail() { echo "[FAIL] $*"; FAILED=$((FAILED + 1)); }
log_skip() { echo "[SKIP] $*"; SKIPPED=$((SKIPPED + 1)); }
log_info() { echo "[INFO] $*"; }

# --- Header ---
echo ""
echo "========================================"
echo "B3/B4/B5 Checklist Runner"
echo "========================================"
echo ""
echo "IMPORTANT: This script is a SAFE LOCAL COMPOSER ONLY."
echo "  - Does NOT complete G2 gates"
echo "  - Does NOT authorize the pilot"
echo "  - Does NOT claim production-ready"
echo "  - Does NOT run target/GCP/SSH commands"
echo ""

# --- B3: Local retention pruning test ---
echo "========================================"
echo "B3 — Local Retention Pruning Test"
echo "========================================"
echo ""

B3_FAILED=0
if [[ -x "$SCRIPT_DIR/test_retention_pruning_locally.sh" ]]; then
    if bash "$SCRIPT_DIR/test_retention_pruning_locally.sh" >/dev/null 2>&1; then
        log_pass "B3 local retention pruning test"
    else
        log_fail "B3 local retention pruning test"
        B3_FAILED=1
    fi
else
    log_fail "B3 script not found or not executable: $SCRIPT_DIR/test_retention_pruning_locally.sh"
    B3_FAILED=1
fi

# --- B4/B5: TLS and auth verifier ---
echo ""
echo "========================================"
echo "B4/B5 — TLS and Auth Verifier"
echo "========================================"
echo ""

B4B5_FAILED=0
if [[ -z "$BASE_URL" ]]; then
    log_skip "B4/B5 verifier — no --base-url provided (provide one to run target checks)"
else
    if [[ -x "$SCRIPT_DIR/verify_tls_and_auth.sh" ]]; then
        VERIFIER_ARGS=(--base-url "$BASE_URL" --token-env "$TOKEN_ENV")
        if [[ "$INSECURE" == true ]]; then
            VERIFIER_ARGS+=(--insecure)
        fi
        VERIFIER_OUTPUT=$(bash "$SCRIPT_DIR/verify_tls_and_auth.sh" "${VERIFIER_ARGS[@]}" 2>&1) || B4B5_FAILED=1
        if [[ $B4B5_FAILED -eq 0 ]]; then
            log_pass "B4/B5 TLS and auth verifier"
        else
            log_fail "B4/B5 TLS and auth verifier"
        fi
    else
        log_fail "B4/B5 script not found or not executable: $SCRIPT_DIR/verify_tls_and_auth.sh"
        B4B5_FAILED=1
    fi
fi

# --- Evidence Checklist ---
echo ""
echo "========================================"
echo "B3/B4/B5 Evidence Checklist"
echo "========================================"
echo ""

# --- Determine dynamic statuses ---
B3_2_STATUS="LOCAL"
if [[ $B3_FAILED -ne 0 ]]; then
    B3_2_STATUS="FAIL"
fi

B4_1_STATUS="PENDING"
B4_2_STATUS="PENDING"
B5_1_STATUS="PENDING"
B5_2_STATUS="PENDING"

if [[ -n "$BASE_URL" ]]; then
    # Parse individual results regardless of overall verifier exit code.
    # Partial successes must be preserved even if the verifier exits nonzero
    # (e.g., because the with-token check failed or was skipped).
    if echo "$VERIFIER_OUTPUT" | grep -q '\[PASS\] GET /v1/healthz'; then
        B4_1_STATUS="TARGET/PARTIAL"
    elif echo "$VERIFIER_OUTPUT" | grep -q '\[FAIL\] GET /v1/healthz'; then
        B4_1_STATUS="FAIL"
    fi

    if echo "$VERIFIER_OUTPUT" | grep -q '\[PASS\] GET /v1/approvals (no token)'; then
        B5_1_STATUS="TARGET/PARTIAL"
    elif echo "$VERIFIER_OUTPUT" | grep -q '\[FAIL\] GET /v1/approvals (no token)'; then
        B5_1_STATUS="FAIL"
    fi

    if echo "$VERIFIER_OUTPUT" | grep -q '\[PASS\] GET /v1/approvals (with token)'; then
        B5_2_STATUS="TARGET"
    elif echo "$VERIFIER_OUTPUT" | grep -q '\[FAIL\] GET /v1/approvals (with token)'; then
        B5_2_STATUS="FAIL"
    elif echo "$VERIFIER_OUTPUT" | grep -q '\[SKIP\] GET /v1/approvals (with token)'; then
        B5_2_STATUS="PENDING"
    fi
fi

cat << EOF
| Check | Status | Evidence Source |
|-------|--------|-----------------|
| B3.1 — Retention policy configured | PENDING | Operator confirms --retention-days set in backup automation |
| B3.2 — Pruning behavior verified    | $B3_2_STATUS   | test_retention_pruning_locally.sh result (above) |
| B4.1 — Public endpoints reachable   | $B4_1_STATUS | verify_tls_and_auth.sh against target (if --base-url provided) |
| B4.2 — TLS configured               | $B4_2_STATUS | Operator confirms reverse proxy TLS termination |
| B5.1 — No token returns 401         | $B5_1_STATUS | verify_tls_and_auth.sh against target (if --base-url provided) |
| B5.2 — Correct token returns 200    | $B5_2_STATUS | verify_tls_and_auth.sh against target (if --base-url provided) |
| B5.3 — Token never logged/exposed   | LOCAL   | verify_tls_and_auth.sh never echoes token value |

EOF

if [[ -n "$BASE_URL" ]]; then
    echo "[INFO] B4/B5 checks were run against target: $BASE_URL"
    if [[ $B4B5_FAILED -eq 0 ]]; then
        echo "       Public endpoints and no-token auth verified. With-token check status: $B5_2_STATUS"
    elif [[ "$B4_1_STATUS" == "TARGET/PARTIAL" && "$B5_1_STATUS" == "TARGET/PARTIAL" ]]; then
        echo "       Partial checks passed (public endpoints, no-token auth). With-token check status: $B5_2_STATUS. Review verifier output above."
    else
        echo "       Some B4/B5 checks FAILED. Review verifier output above."
    fi
else
    echo "[INFO] B4/B5 target checks were SKIPPED (no --base-url)."
    echo "       Re-run with --base-url <URL> to populate B4/B5 evidence."
fi

# --- Blockers ---
echo ""
echo "========================================"
echo "Remaining Operator-Owned Blockers"
echo "========================================"
echo ""

cat << 'EOF'
The following remain pending until operator action is taken:

1. Backup automation scheduler must be configured (systemd timer or cron)
   and must include --retention-days in the backup command.

2. B4/B5 target evidence requires running verify_tls_and_auth.sh against
   the actual deployed target with real TLS and bearer token.

3. doc59 (G2.1–G2.8) and doc54 are signed for conditional single-node
   SQLite pilot scope only. They do not authorize broader/full pilot
   or production-ready status.

4. B3/B4/B5 target evidence remains required before any stronger/full
   pilot readiness or production claim.

5. This script does not complete G2, authorize the pilot, or claim
   production-ready. FerrumGate v1 remains RC-ready/conditional.
EOF

# --- Summary ---
echo ""
echo "========================================"
echo "B3/B4/B5 CHECKLIST RESULT"
echo "========================================"
echo ""
echo "Passed:  $PASSED"
echo "Failed:  $FAILED"
echo "Skipped: $SKIPPED"
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo "B3/B4/B5: ALL APPLICABLE CHECKS PASSED"
    echo ""
    echo "Local evidence gathered. Target evidence still required."
    echo "Operator signoff (doc59/doc54) completed for conditional single-node SQLite pilot only."
    echo "No production-ready claim. FerrumGate v1 remains RC-ready/conditional."
    echo ""
    exit 0
else
    echo "B3/B4/B5: SOME CHECKS FAILED"
    echo ""
    exit 1
fi
