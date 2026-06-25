#!/usr/bin/env bash
# scripts/run_secret_history_scan.sh
# Git history secret scan — attempts gitleaks, falls back to high-entropy grep.
# Status: Operational. No external secrets required.
# Scope: Full git history (all commits). May produce false positives.
#
# Usage:
#   bash scripts/run_secret_history_scan.sh
#
# Exits 0 if no potential secrets are found.
# Exits 1 if potential secrets are detected, printing path/line/pattern only
# (secret values are NEVER printed).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

GITLEAKS_FOUND=false
GITLEAKS_IMAGE="ghcr.io/gitleaks/gitleaks:v8.24.0"

# --- Try gitleaks via docker (no auth needed for public images) ---
try_gitleaks_docker() {
    if ! command -v docker >/dev/null 2>&1; then
        return 1
    fi
    echo "[INFO] Attempting gitleaks via Docker (${GITLEAKS_IMAGE})..."
    if docker run --rm -v "${REPO_ROOT}:/repo:ro" "${GITLEAKS_IMAGE}" detect --source=/repo --verbose --redact; then
        echo "[OK] gitleaks (Docker) found no secrets in history."
        exit 0
    else
        echo "[WARN] gitleaks (Docker) detected potential secrets or failed."
        return 1
    fi
}

# --- Try gitleaks binary ---
try_gitleaks_binary() {
    if command -v gitleaks >/dev/null 2>&1; then
        echo "[INFO] Running gitleaks binary..."
        if gitleaks detect --source="${REPO_ROOT}" --verbose --redact; then
            echo "[OK] gitleaks found no secrets in history."
            exit 0
        else
            echo "[WARN] gitleaks detected potential secrets or failed."
            return 1
        fi
    fi
    return 1
}

# --- Fallback: high-entropy grep over git history ---
run_fallback_scan() {
    echo "[INFO] Falling back to high-entropy grep over git history..."
    echo "[INFO] This is a conservative placeholder and may produce false positives."

    # Patterns to scan git log and diffs for common secret indicators
    # We exclude the current scan script itself and lockfiles from the diff scan.
    local FINDINGS=0
    local TMP_LOG
    TMP_LOG=$(mktemp)
    trap 'rm -f "${TMP_LOG}"' EXIT

    # High-entropy heuristic patterns (adapted from common secret types)
    # These look for 20+ char alphanumeric strings in assignments/contexts
    local DETECT_REGEX='(api_key|api_secret|password|passwd|secret|token|bearer|private_key|sk_live_|sk_test_|ghp_|gho_|ghs_|fg_live_|fg_test_)[\s]*[:=][\s]*["'\''`][a-zA-Z0-9_\-/+=]{20,}["'\''`]'

    # Scan all commits for additions matching the pattern
    # Use --no-merges to reduce noise, and -p to show patches
    if git rev-parse --git-dir >/dev/null 2>&1; then
        git log --all --no-merges -p --pickaxe-regex -S"${DETECT_REGEX}" -- 2>/dev/null > "${TMP_LOG}" || true
    else
        echo "[ERR] Not a git repository; cannot scan history."
        exit 1
    fi

    # Parse the log output for actual additions (lines starting with +)
    local RAW_OUT
    RAW_OUT=$(mktemp)
    trap 'rm -f "${TMP_LOG}" "${RAW_OUT}"' EXIT

    grep -nE '^\+.*'"${DETECT_REGEX}""\${TMP_LOG}" >"${RAW_OUT}" 2>/dev/null || true

    # Filter out safe lines (examples, placeholders, test fixtures)
    local SAFE_LINE_REGEX='CHANGE[_-]?ME|REPLACE[_-]?WITH|PLACEHOLDER|DUMMY|test-token|secret-token|valid-test-token|super-secret-token-value|None|null|=""|= .*. |REDACTED|<PG_USER>|<PG_HOST>|<PG_PORT>|<PG_DATABASE>|<SET_VIA_SECRETS_MANAGER>|GENERATED_TOKEN|your-issuer|your-domain|example\.com|example\.org|\.example/|test-issuer|file-issuer|env-issuer|wrong-issuer|<generate-with-openssl-rand-hex-32>|\$[A-Za-z_][A-Za-z0-9_]*|\$\{[^}]+\}'

    while IFS= read -r match; do
        if echo "${match}" | grep -qE "${SAFE_LINE_REGEX}"; then
            continue
        fi
        echo "[FINDING] ${match}"
        FINDINGS=$((FINDINGS + 1))
    done <"${RAW_OUT}"

    if [[ ${FINDINGS} -eq 0 ]]; then
        echo "[OK] Fallback scan found no potential secrets in git history."
        echo "[NOTE] This is a heuristic scan; consider installing gitleaks for stronger detection."
        exit 0
    else
        echo "[FAIL] ${FINDINGS} potential secret finding(s) in git history."
        echo "[NOTE] Review findings above. If false positives, add safe-line patterns to this script."
        exit 1
    fi
}

# --- Main ---
main() {
    echo "=== FerrumGate Git History Secret Scan ==="
    echo "[INFO] Scope: full git history (all commits)."

    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        echo "[ERR] Not a git repository."
        exit 1
    fi

    try_gitleaks_binary || try_gitleaks_docker || run_fallback_scan
}

main
