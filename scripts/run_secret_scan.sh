#!/usr/bin/env bash
# run_secret_scan.sh — Lightweight hardcoded secrets scan for FerrumGate
# Status: Operational. Dependency-free. No external scanner required.
# Scope: Working tree (tracked files). No git history scan.
#
# Usage:
#   bash scripts/run_secret_scan.sh
#
# Exits 0 if no potential hardcoded secrets are found.
# Exits 1 if potential secrets are detected, printing path/line/pattern only
# (secret values are NEVER printed).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

FINDINGS=0

# Files to skip entirely (basename or path substring)
SKIP_PATTERNS=(
    "CHANGELOG.md"
    "SECURITY.md"
    "^.git/"
    "^target/"
    "^scripts/run_secret_scan\.sh$"
)

# Patterns that indicate a line is a known safe placeholder/test fixture.
# Shell variable references ($VAR, ${VAR}) are treated as safe because the
# actual value is injected at runtime and not hardcoded in the file.
SAFE_LINE_REGEX='CHANGE[_-]?ME|REPLACE[_-]?WITH|PLACEHOLDER|DUMMY|test-token|secret-token|valid-test-token|super-secret-token-value|None|null|=""|= .*. |REDACTED|<PG_USER>|<PG_HOST>|<PG_PORT>|<PG_DATABASE>|<REDACTED>|<SET_VIA_SECRETS_MANAGER>|GENERATED_TOKEN|your-issuer|your-domain|example\.com|example\.org|\.example/|test-issuer|file-issuer|env-issuer|wrong-issuer|<generate-with-openssl-rand-hex-32>|\$[A-Za-z_][A-Za-z0-9_]*|\$\{[^}]+\}'

# Combined detection regex (double-quoted so single quotes inside are literal).
# NOTE: grep is invoked with -- before the regex to prevent leading dashes
# in patterns (e.g. -----BEGIN) from being interpreted as options.
DETECT_REGEX="(fg_live_[a-f0-9]{16,})|(fg_test_[a-f0-9]{16,})|(-----BEGIN .*PRIVATE KEY-----)|(ghp_[A-Za-z0-9_]{36})|(gho_[A-Za-z0-9_]{36})|(ghs_[A-Za-z0-9_]{36})|(sk_live_[a-zA-Z0-9]{24,})|(sk_test_[a-zA-Z0-9]{24,})|(pk_live_[a-zA-Z0-9]{24,})|(pk_test_[a-zA-Z0-9]{24,})|(SG\.[A-Za-z0-9_-]{22,})|(bearer_token\s*[=:]\s*\"[^\"]{8,}\")|(bearer_token\s*[=:]\s*'[^']{8,}')|(api_key\s*[=:]\s*\"[^\"]{8,}\")|(api_key\s*[=:]\s*'[^']{8,}')|(api_secret\s*[=:]\s*\"[^\"]{8,}\")|(api_secret\s*[=:]\s*'[^']{8,}')|(password\s*[=:]\s*\"[^\"]{8,}\")|(password\s*[=:]\s*'[^']{8,}')"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

is_safe_line() {
    local line="$1"
    if echo "$line" | grep -qE -- "$SAFE_LINE_REGEX"; then
        return 0
    fi
    return 1
}

line_matches() {
    local line="$1"
    local pattern="$2"
    echo "$line" | grep -qE -- "$pattern"
}

should_skip_file() {
    local file="$1"
    for skip in "${SKIP_PATTERNS[@]}"; do
        if echo "$file" | grep -qE -- "$skip"; then
            return 0
        fi
    done
    return 1
}

identify_pattern() {
    local line="$1"
    local name=""

    if line_matches "$line" 'fg_live_[a-f0-9]{16,}'; then name="FERRUM_LIVE_TOKEN"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'fg_test_[a-f0-9]{16,}'; then name="FERRUM_TEST_TOKEN"; fi
    if [[ -z "$name" ]] && line_matches "$line" '-----BEGIN .*PRIVATE KEY-----'; then name="PEM_PRIVATE_KEY"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'ghp_[A-Za-z0-9_]{36}'; then name="GITHUB_PAT"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'gho_[A-Za-z0-9_]{36}'; then name="GITHUB_OAUTH"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'ghs_[A-Za-z0-9_]{36}'; then name="GITHUB_SERVER"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'sk_live_[a-zA-Z0-9]{24,}'; then name="STRIPE_LIVE_SK"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'sk_test_[a-zA-Z0-9]{24,}'; then name="STRIPE_TEST_SK"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'pk_live_[a-zA-Z0-9]{24,}'; then name="STRIPE_LIVE_PK"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'pk_test_[a-zA-Z0-9]{24,}'; then name="STRIPE_TEST_PK"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'SG\.[A-Za-z0-9_-]{22,}'; then name="SENDGRID_KEY"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'bearer_token\s*[=:]\s*"[^"]{8,}"'; then name="BEARER_ASSIGNMENT"; fi
    if [[ -z "$name" ]] && line_matches "$line" "bearer_token\s*[=:]\s*'[^']{8,}'"; then name="BEARER_ASSIGNMENT_SINGLE"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'api_key\s*[=:]\s*"[^"]{8,}"'; then name="API_KEY_ASSIGNMENT"; fi
    if [[ -z "$name" ]] && line_matches "$line" "api_key\s*[=:]\s*'[^']{8,}'"; then name="API_KEY_ASSIGNMENT_SINGLE"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'api_secret\s*[=:]\s*"[^"]{8,}"'; then name="API_SECRET_ASSIGNMENT"; fi
    if [[ -z "$name" ]] && line_matches "$line" "api_secret\s*[=:]\s*'[^']{8,}'"; then name="API_SECRET_ASSIGNMENT_SINGLE"; fi
    if [[ -z "$name" ]] && line_matches "$line" 'password\s*[=:]\s*"[^"]{8,}"'; then name="PASSWORD_ASSIGNMENT"; fi
    if [[ -z "$name" ]] && line_matches "$line" "password\s*[=:]\s*'[^']{8,}'"; then name="PASSWORD_ASSIGNMENT_SINGLE"; fi

    if [[ -z "$name" ]]; then
        name="UNKNOWN"
    fi
    echo "$name"
}

# ---------------------------------------------------------------------------
# Scan
# ---------------------------------------------------------------------------

echo "=== FerrumGate Hardcoded Secrets Scan ==="

FILES=()
if git rev-parse --git-dir &>/dev/null; then
    mapfile -t FILES < <(git ls-files)
else
    mapfile -t FILES < <(find . -type f -not -path './.git/*' -not -path './target/*')
fi

echo "Files to scan: ${#FILES[@]}"
echo ""

FILE_LIST=$(mktemp)
RAW_OUT=$(mktemp)
trap 'rm -f "$FILE_LIST" "$RAW_OUT"' EXIT

for f in "${FILES[@]}"; do
    [[ -f "$f" ]] || continue
    if file "$f" | grep -q "binary"; then
        continue
    fi
    if should_skip_file "$f"; then
        continue
    fi
    printf '%s\n' "$f" >> "$FILE_LIST"
done

# Use xargs with newline delimiter to handle filenames containing spaces safely.
xargs -d '\n' grep -nH -E -- "$DETECT_REGEX" < "$FILE_LIST" 2>/dev/null > "$RAW_OUT" || true

if [[ ! -s "$RAW_OUT" ]]; then
    echo "=== SECRET SCAN: PASS ==="
    echo "No potential hardcoded secrets detected."
    echo "Scope: tracked working-tree files only. Git history NOT scanned."
    echo "This scan does NOT constitute formal compliance or production-ready certification."
    exit 0
fi

while IFS= read -r match; do
    if is_safe_line "$match"; then
        continue
    fi

    file=$(echo "$match" | cut -d':' -f1)
    line_num=$(echo "$match" | cut -d':' -f2)
    matched_name=$(identify_pattern "$match")

    echo "[FINDING] $file:$line_num — pattern: $matched_name"
    FINDINGS=$((FINDINGS + 1))
done < "$RAW_OUT"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
if [[ $FINDINGS -eq 0 ]]; then
    echo "=== SECRET SCAN: PASS ==="
    echo "No potential hardcoded secrets detected."
    echo "Scope: tracked working-tree files only. Git history NOT scanned."
    echo "This scan does NOT constitute formal compliance or production-ready certification."
    exit 0
else
    echo "=== SECRET SCAN: FAIL ($FINDINGS finding(s)) ==="
    echo "Potential hardcoded secrets were detected."
    echo "Review the [FINDING] lines above. Secret values are redacted."
    echo "Do not mark Phase 5.5 complete until findings are remediated or verified false positives."
    exit 1
fi
