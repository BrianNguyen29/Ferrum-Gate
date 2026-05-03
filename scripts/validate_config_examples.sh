#!/usr/bin/env bash
# validate_config_examples.sh
# Validates config/example invariants for FerrumGate v1 single-node SQLite pilot.
# Does NOT require target host, SSH, or real secrets.
# Single-node only; no PostgreSQL/multi-node/HA.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLES_DIR="$REPO_ROOT/configs/examples"
CONFIG_DIR="$REPO_ROOT/configs"
DOCS_DIR="$REPO_ROOT/docs/implementation-path"
FAILED=0

# --- Helpers ---

warn() { echo "[WARN] $*" >&2; }
fail() { echo "[FAIL] $*" >&2; FAILED=1; }
pass() { echo "[PASS] $*"; }
info() { echo "[INFO] $*"; }

# --- 1. Check for real-secrets patterns in all config/example files ---

info "=== Checking for real-secrets patterns ==="

REAL_SECRET_PATTERNS=(
    'fg_live_[a-f0-9]{32,}'   # looks like a real live token
    '-----BEGIN.*PRIVATE KEY-----'
    'password\s*=\s*["'\''][^$<>]{8,}["'\'']'
)

# Exception: ferrumd.env.example has REPLACE_WITH_GENERATED_TOKEN placeholder which is fine.
for file in "$EXAMPLES_DIR"/* "$CONFIG_DIR"/*.toml; do
    [[ -f "$file" ]] || continue
    # Skip .env.example — it has a safe placeholder
    if [[ "$file" == *.env.example ]]; then
        continue
    fi

    for pattern in "${REAL_SECRET_PATTERNS[@]}"; do
        if grep -E --quiet "$pattern" "$file" 2>/dev/null; then
            fail "$file contains a pattern that looks like a real secret: $pattern"
        fi
    done
done
pass "No real-secret patterns detected in configs"

# --- 2. ferrumd.env.example must have safe placeholder ---

info "=== Checking ferrumd.env.example ==="

ENV_EXAMPLE="$EXAMPLES_DIR/ferrumd.env.example"
if [[ ! -f "$ENV_EXAMPLE" ]]; then
    fail "$ENV_EXAMPLE does not exist"
else
    if grep -q 'REPLACE_WITH_GENERATED_TOKEN' "$ENV_EXAMPLE"; then
        pass "ferrumd.env.example has safe placeholder (REPLACE_WITH_GENERATED_TOKEN)"
    else
        fail "ferrumd.env.example does not contain expected placeholder REPLACE_WITH_GENERATED_TOKEN"
    fi

    # Must not contain an actual hex token
    if grep -E 'fg_live_[a-f0-9]{32,}' "$ENV_EXAMPLE" >/dev/null 2>&1; then
        fail "ferrumd.env.example contains what looks like a real token"
    else
        pass "ferrumd.env.example does not contain a real token"
    fi
fi

# --- 3. nginx Authorization forwarding must NOT prepend "Bearer " ---

info "=== Checking nginx Authorization forwarding ==="

NGINX_CONF="$EXAMPLES_DIR/nginx-ferrumgate.conf"
if [[ -f "$NGINX_CONF" ]]; then
    # Wrong: proxy_set_header Authorization "Bearer $http_authorization";
    # Right: proxy_set_header Authorization $http_authorization;
    if grep -E 'proxy_set_header\s+Authorization\s+"Bearer\s+\$http_authorization"' "$NGINX_CONF" >/dev/null 2>&1; then
        fail "$NGINX_CONF: Authorization header incorrectly prepends 'Bearer '"
    else
        pass "$NGINX_CONF: Authorization header forwarding is correct (no double Bearer)"
    fi

    # Must have the correct forwarding line
    if grep -E 'proxy_set_header\s+Authorization\s+\$http_authorization' "$NGINX_CONF" >/dev/null 2>&1; then
        pass "$NGINX_CONF: proxy_set_header Authorization \$http_authorization found"
    else
        warn "$NGINX_CONF: proxy_set_header Authorization \$http_authorization NOT found"
    fi
else
    warn "$NGINX_CONF not found, skipping"
fi

# --- 4. systemd ferrumd.service must reference EnvironmentFile for token ---

info "=== Checking ferrumd.service EnvironmentFile ==="

FERRUMD_SERVICE="$EXAMPLES_DIR/ferrumd.service"
if [[ -f "$FERRUMD_SERVICE" ]]; then
    if grep -q 'EnvironmentFile=' "$FERRUMD_SERVICE"; then
        pass "$FERRUMD_SERVICE: EnvironmentFile reference found"
    else
        warn "$FERRUMD_SERVICE: EnvironmentFile reference not found (token loaded another way)"
    fi

    # Must not have a hardcoded bearer_token
    if grep -E 'bearer_token\s*=' "$FERRUMD_SERVICE" >/dev/null 2>&1; then
        fail "$FERRUMD_SERVICE: contains hardcoded bearer_token"
    else
        pass "$FERRUMD_SERVICE: no hardcoded bearer_token"
    fi
else
    warn "$FERRUMD_SERVICE not found, skipping"
fi

# --- 5. Config examples must NOT contain PostgreSQL/multi-node references in wrong contexts ---

info "=== Checking for PostgreSQL/multi-node claims in config examples ==="

for file in "$EXAMPLES_DIR"/* "$CONFIG_DIR"/*.toml; do
    [[ -f "$file" ]] || continue
    # Skip files that explicitly mention PostgreSQL as NOT implemented
    if grep -E 'NOT implemented|not implemented|postgresql.*NOT|postgres.*not' "$file" >/dev/null 2>&1; then
        continue
    fi
    # Wrong: having store_dsn = "postgres://..." in a single-node example
    if grep -E 'store_dsn\s*=\s*["'\'']postgres' "$file" >/dev/null 2>&1; then
        fail "$file: contains PostgreSQL store_dsn (not supported in Phase 1)"
    fi
done
pass "No PostgreSQL store_dsn found in single-node config examples"

# --- 6. Check that no example config claims production-ready ---

info "=== Checking for production-ready claims in examples ==="

# We look for AFFIRMATIVE claims only (not disclaimers like "No production-ready claim").
# An affirmative claim contains "production ready" or "production-ready" or "go live"
# WITHOUT the word "No" on the same line.
AFFIRMATIVE_PATTERN='production.?ready|go.?live'

for file in "$EXAMPLES_DIR"/* "$CONFIG_DIR"/*.toml; do
    [[ -f "$file" ]] || continue
    # Find lines with the affirmative pattern
    AFFIRMATIVE_LINES=$(grep -E "$AFFIRMATIVE_PATTERN" "$file" 2>/dev/null || true)
    if [[ -z "$AFFIRMATIVE_LINES" ]]; then
        continue
    fi
    # Filter out disclaimer lines (those containing "No" and the pattern)
    CLAIMS=$(echo "$AFFIRMATIVE_LINES" | grep -vE 'No.*production|No production|No.*go.?live' || true)
    if [[ -n "$CLAIMS" ]]; then
        fail "$file: contains affirmative production-ready claim: $CLAIMS"
    fi
done
pass "No affirmative production-ready claims in config examples"

# --- 7. validate_repo_layout.sh must still pass ---

info "=== Running repo layout validation ==="
if [[ -f "$REPO_ROOT/scripts/validate_repo_layout.sh" ]]; then
    if bash "$REPO_ROOT/scripts/validate_repo_layout.sh" >/dev/null 2>&1; then
        pass "repo layout validation passed"
    else
        fail "repo layout validation failed"
    fi
else
    warn "scripts/validate_repo_layout.sh not found or not executable, skipping"
fi

# --- 8. check_contract_consistency.py must still pass ---

info "=== Running contract consistency check ==="
if [[ -f "$REPO_ROOT/scripts/check_contract_consistency.py" ]]; then
    if python3 "$REPO_ROOT/scripts/check_contract_consistency.py" >/dev/null 2>&1; then
        pass "contract consistency check passed"
    else
        fail "contract consistency check failed"
    fi
else
    warn "scripts/check_contract_consistency.py not found or not executable, skipping"
fi

# --- Summary ---

echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "=== ALL CHECKS PASSED ==="
    exit 0
else
    echo "=== SOME CHECKS FAILED ==="
    exit 1
fi
