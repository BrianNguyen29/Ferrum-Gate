#!/usr/bin/env bash
# operator_preflight.sh
# Operator-facing preflight check script for FerrumGate v1 single-node SQLite.
# Validates target-environment readiness prerequisites before Phase 2 execution.
# Operator aid only; does NOT complete G2, does NOT authorize pilot, does NOT claim production-ready.
#
# Usage:
#   bash scripts/operator_preflight.sh [--help]
#   bash scripts/operator_preflight.sh [--dry-run] [--base-url URL] [--bearer-token TOKEN]
#              [--config-path PATH] [--store-dsn DSN] [--backup-dir DIR]
#              [--db-path PATH] [--ferrumctl PATH]
#
# Environment variables (alternative to flags):
#   FERRUMD_BASE_URL      Target base URL (e.g., http://localhost:18080)
#   FERRUMD_BEARER_TOKEN  Bearer token for protected endpoints
#   FERRUMD_CONFIG        Config file path for ferrumd syntax check
#   FERRUMD_STORE_DSN    Store DSN to validate (warn if in-memory or /tmp)
#   FERRUMD_BACKUP_DIR    Backup directory to check (must exist and be writable)
#   FERRUMCTL             Path to ferrumctl binary
#
# All checks are read-only/non-destructive. No secrets are required for dry-run mode.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Defaults ---
DRY_RUN="${DRY_RUN:-false}"
BASE_URL="${FERRUMD_BASE_URL:-}"
BEARER_TOKEN="${FERRUMD_BEARER_TOKEN:-}"
CONFIG_PATH="${FERRUMD_CONFIG:-}"
STORE_DSN="${FERRUMD_STORE_DSN:-}"
BACKUP_DIR="${FERRUMD_BACKUP_DIR:-}"
DB_PATH="${FERRUMD_DB_PATH:-}"
FERRUMCTL_BIN="${FERRUMCTL:-}"

FAILED=0
SKIPPED=0
WARNED=0

# --- Usage ---
usage() {
    cat << 'EOF'
FerrumGate v1 Operator Preflight Check

Usage:
  bash scripts/operator_preflight.sh [options]

Options:
  --help              Show this help message and exit
  --dry-run           Run in dry-run mode (no target contact, safe checks only)

  --base-url URL      Target base URL (e.g., http://localhost:18080)
  --bearer-token TOKEN Bearer token for protected endpoints
  --config-path PATH  Config file path for syntax validation
  --store-dsn DSN     Store DSN to validate
  --backup-dir DIR    Backup directory to check (must exist and be writable)
  --db-path PATH      Path to SQLite database file (for backup verify)
  --ferrumctl PATH    Path to ferrumctl binary

Environment variables (alternative to flags):
  FERRUMD_BASE_URL, FERRUMD_BEARER_TOKEN, FERRUMD_CONFIG,
  FERRUMD_STORE_DSN, FERRUMD_BACKUP_DIR, FERRUMD_DB_PATH, FERRUMCTL

Description:
  This script performs preflight readiness checks for the target environment.
  It is an OPERATOR AID ONLY and does NOT:

  - Complete G2 gates
  - Authorize the pilot
  - Claim production-ready status
  - Perform any destructive operations

  All checks are read-only or static. Real secrets are not required for
  dry-run mode. Checks that require secrets will report [SKIP] if not provided.

Exit codes:
  0   All applicable checks passed
  1   One or more checks failed
  2   Usage error
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            usage
            exit 0
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --base-url)
            BASE_URL="${2:-}"
            if [[ -z "$BASE_URL" ]]; then
                echo "[ERROR] --base-url requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --bearer-token)
            BEARER_TOKEN="${2:-}"
            if [[ -z "$BEARER_TOKEN" ]]; then
                echo "[ERROR] --bearer-token requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --config-path)
            CONFIG_PATH="${2:-}"
            if [[ -z "$CONFIG_PATH" ]]; then
                echo "[ERROR] --config-path requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --store-dsn)
            STORE_DSN="${2:-}"
            if [[ -z "$STORE_DSN" ]]; then
                echo "[ERROR] --store-dsn requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --backup-dir)
            BACKUP_DIR="${2:-}"
            if [[ -z "$BACKUP_DIR" ]]; then
                echo "[ERROR] --backup-dir requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --db-path)
            DB_PATH="${2:-}"
            if [[ -z "$DB_PATH" ]]; then
                echo "[ERROR] --db-path requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        --ferrumctl)
            FERRUMCTL_BIN="${2:-}"
            if [[ -z "$FERRUMCTL_BIN" ]]; then
                echo "[ERROR] --ferrumctl requires a value" >&2
                exit 2
            fi
            shift 2
            ;;
        *)
            echo "[ERROR] Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# --- Helper functions ---

log_pass() {
    echo "[PASS] $*"
}

log_fail() {
    echo "[FAIL] $*"
    FAILED=$((FAILED + 1))
}

log_skip() {
    echo "[SKIP] $*"
    SKIPPED=$((SKIPPED + 1))
}

log_warn() {
    echo "[WARN] $*"
    WARNED=$((WARNED + 1))
}

log_info() {
    echo "[INFO] $*"
}

is_placeholder_token() {
    local token="$1"
    # Heuristic: tokens that look like examples, placeholders, or obviously fake
    [[ -z "$token" ]] && return 0
    [[ "$token" =~ ^(changeme|your-token|placeholder|example|test-token|secret)$
        || "$token" =~ ^openssl\ rand\ -hex\ 32$
        || "${#token}" -lt 16 ]] && return 0
    return 1
}

check_curl() {
    command -v curl >/dev/null 2>&1
}

http_get() {
    local url="$1"
    local token="${2:-}"
    local extra_args=("-s" "-o" "/dev/null" "-w" "%{http_code}")
    if [[ -n "$token" ]]; then
        curl "${extra_args[@]}" -H "Authorization: Bearer $token" "$url" 2>/dev/null
    else
        curl "${extra_args[@]}" "$url" 2>/dev/null
    fi
}

http_get_body() {
    local url="$1"
    local token="${2:-}"
    if [[ -n "$token" ]]; then
        curl -s -H "Authorization: Bearer $token" "$url" 2>/dev/null
    else
        curl -s "$url" 2>/dev/null
    fi
}

# --- Header ---
echo ""
echo "========================================"
echo "FerrumGate v1 Operator Preflight Check"
echo "========================================"
echo ""
echo "IMPORTANT: This script is an OPERATOR AID ONLY."
echo "  - Does NOT complete G2 gates"
echo "  - Does NOT authorize the pilot"
echo "  - Does NOT claim production-ready"
echo ""
echo "All checks are read-only/non-destructive."
echo ""

if [[ "$DRY_RUN" == "true" ]]; then
    echo "[INFO] Running in DRY-RUN mode (no target contact)"
    echo ""
fi

# --- Check 1: Config file path readability and basic TOML syntax ---
echo "========================================"
echo "CHECK: Config file readability"
echo "========================================"

if [[ -z "$CONFIG_PATH" ]]; then
    log_skip "Config path not provided (use --config-path or FERRUMD_CONFIG)"
else
    if [[ ! -f "$CONFIG_PATH" ]]; then
        log_fail "Config file does not exist: $CONFIG_PATH"
    elif [[ ! -r "$CONFIG_PATH" ]]; then
        log_fail "Config file not readable: $CONFIG_PATH"
    else
        log_pass "Config file exists and is readable: $CONFIG_PATH"

        # Basic TOML syntax check (look for common errors)
        # This is a lightweight static check, not a full parser
        if command -v python3 >/dev/null 2>&1; then
            if python3 -c "
import re
try:
    with open('$CONFIG_PATH', 'r') as f:
        content = f.read()
    # Check for basic TOML issues
    lines = content.split('\n')
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        # Skip comments and empty lines
        if not stripped or stripped.startswith('#'):
            continue
        # Check for unquoted table names
        if re.match(r'^\[[a-zA-Z0-9_-]+\]$', stripped):
            pass
        # Check for unquoted keys with = sign
        elif '=' in stripped and not re.search(r'^[^=]+\s*=\s*[\"\']', stripped):
            # Heuristic: bare key = value might be ok in TOML but flag if suspicious
            pass
    print('ok')
" 2>/dev/null | grep -q "ok"; then
                log_pass "Config file basic TOML syntax looks valid"
            else
                log_warn "Config file may have TOML syntax issues (review recommended)"
            fi
        else
            log_info "TOML validation skipped (python3 not available)"
        fi
    fi
fi
echo ""

# --- Check 2: Target base URL health/readiness ---
echo "========================================"
echo "CHECK: Target base URL readiness"
echo "========================================"

if [[ -z "$BASE_URL" ]]; then
    log_skip "Base URL not provided (use --base-url or FERRUMD_BASE_URL)"
else
    # Trim trailing slash
    BASE_URL="${BASE_URL%/}"

    if [[ "$DRY_RUN" == "true" ]]; then
        log_skip "Dry-run mode: skipping HTTP checks against $BASE_URL"
    else
        log_info "Checking target: $BASE_URL"

        # Check 2a: /v1/healthz (should always be 200)
        HTTP_CODE=$(http_get "$BASE_URL/v1/healthz")
        if [[ "$HTTP_CODE" == "200" ]]; then
            log_pass "GET /v1/healthz returned $HTTP_CODE (target is reachable)"
        else
            log_fail "GET /v1/healthz returned $HTTP_CODE (expected 200)"
        fi

        # Check 2b: /v1/readyz (should always be 200)
        HTTP_CODE=$(http_get "$BASE_URL/v1/readyz")
        if [[ "$HTTP_CODE" == "200" ]]; then
            log_pass "GET /v1/readyz returned $HTTP_CODE"
        else
            log_fail "GET /v1/readyz returned $HTTP_CODE (expected 200)"
        fi

        # Check 2c: /v1/readyz/deep (protected when gateway auth is enabled)
        if [[ -z "$BEARER_TOKEN" ]]; then
            log_skip "Skipping /v1/readyz/deep; provide --bearer-token for protected deep readiness"
        else
            HTTP_CODE=$(http_get "$BASE_URL/v1/readyz/deep" "$BEARER_TOKEN")
            if [[ "$HTTP_CODE" == "200" ]]; then
                log_pass "GET /v1/readyz/deep returned $HTTP_CODE (store healthy)"
            elif [[ "$HTTP_CODE" == "503" ]]; then
                log_warn "GET /v1/readyz/deep returned 503 (store unhealthy or write queue high)"
            else
                log_fail "GET /v1/readyz/deep returned $HTTP_CODE (expected 200 or 503)"
            fi
        fi

        # Check 2d: /v1/metrics (protected when gateway auth is enabled)
        if [[ -z "$BEARER_TOKEN" ]]; then
            log_skip "Skipping /v1/metrics; provide --bearer-token for protected metrics"
        else
            HTTP_CODE=$(http_get "$BASE_URL/v1/metrics" "$BEARER_TOKEN")
            if [[ "$HTTP_CODE" == "200" ]]; then
                log_pass "GET /v1/metrics returned $HTTP_CODE (observability endpoint available)"

                # Check for governance metrics presence
                METRICS_BODY=$(http_get_body "$BASE_URL/v1/metrics" "$BEARER_TOKEN")
                if echo "$METRICS_BODY" | grep -q "ferrumgate_governance_errors_total"; then
                    log_pass "Governance error counters present in /v1/metrics"
                else
                    log_warn "Governance error counters not found in /v1/metrics"
                fi
                if echo "$METRICS_BODY" | grep -q "ferrumgate_governance_success_total"; then
                    log_pass "Governance success counters present in /v1/metrics"
                else
                    log_warn "Governance success counters not found in /v1/metrics"
                fi
            else
                log_fail "GET /v1/metrics returned $HTTP_CODE (expected 200)"
            fi
        fi
    fi
fi
echo ""

# --- Check 3: Bearer token validation ---
echo "========================================"
echo "CHECK: Bearer token validation"
echo "========================================"

if [[ -z "$BEARER_TOKEN" ]]; then
    log_skip "Bearer token not provided (use --bearer-token or FERRUMD_BEARER_TOKEN)"
elif is_placeholder_token "$BEARER_TOKEN"; then
    log_warn "Bearer token appears to be a placeholder/example value"
    log_info "  Provide a real token via --bearer-token or FERRUMD_BEARER_TOKEN"
    if [[ -n "$BASE_URL" && "$DRY_RUN" != "true" ]]; then
        log_info "  Skipping protected endpoint checks until real token provided"
    fi
elif [[ -z "$BASE_URL" ]]; then
    log_info "Bearer token provided but base URL not set; cannot test protected endpoints"
    log_pass "Bearer token format looks valid (non-placeholder)"
elif [[ "$DRY_RUN" == "true" ]]; then
    log_skip "Dry-run mode: skipping protected endpoint check with token"
else
    # Test protected endpoint with provided token
    HTTP_CODE=$(http_get "$BASE_URL/v1/approvals" "$BEARER_TOKEN")
    if [[ "$HTTP_CODE" == "200" ]]; then
        log_pass "Protected endpoint /v1/approvals accepted token (HTTP $HTTP_CODE)"
    elif [[ "$HTTP_CODE" == "401" ]]; then
        log_fail "Protected endpoint /v1/approvals rejected valid-looking token (HTTP 401)"
    else
        log_warn "Protected endpoint /v1/approvals returned unexpected HTTP $HTTP_CODE"
    fi
fi
echo ""

# --- Check 4: Store DSN validation ---
echo "========================================"
echo "CHECK: Store DSN validation"
echo "========================================"

if [[ -z "$STORE_DSN" ]]; then
    log_skip "Store DSN not provided (use --store-dsn or FERRUMD_STORE_DSN)"
else
    # Check for in-memory or /tmp DSN (not suitable for production persistence)
    if [[ "$STORE_DSN" == *"memory"* ]] || [[ "$STORE_DSN" == *":memory:"* ]]; then
        log_warn "Store DSN is in-memory: $STORE_DSN"
        log_info "  In-memory stores are NOT suitable for production"
        log_info "  Use a persistent path (e.g., sqlite:///var/lib/ferrumgate/ferrumgate.db)"
    elif [[ "$STORE_DSN" == *"/tmp/"* ]] || [[ "$STORE_DSN" == *"/tmp"* ]]; then
        log_warn "Store DSN uses /tmp path: $STORE_DSN"
        log_info "  /tmp paths are NOT suitable for production (temporary filesystem)"
        log_info "  Use a persistent path outside /tmp"
    else
        log_pass "Store DSN does not appear to be in-memory or /tmp: $STORE_DSN"

        # If it's a file path, check if parent directory exists
        if [[ "$STORE_DSN" == "sqlite://"* ]]; then
            DB_PATH="${STORE_DSN#sqlite://}"
            DB_PATH="/${DB_PATH}"  # Ensure absolute path
            DB_DIR="$(dirname "$DB_PATH")"
            if [[ -d "$DB_DIR" ]]; then
                log_pass "Store database directory exists: $DB_DIR"
            else
                log_warn "Store database directory does not exist: $DB_DIR"
                log_info "  Create directory before starting ferrumd"
            fi
        fi
    fi
fi
echo ""

# --- Check 5: Backup directory validation ---
echo "========================================"
echo "CHECK: Backup directory validation"
echo "========================================"

if [[ -z "$BACKUP_DIR" ]]; then
    log_skip "Backup directory not provided (use --backup-dir or FERRUMD_BACKUP_DIR)"
else
    if [[ ! -d "$BACKUP_DIR" ]]; then
        log_fail "Backup directory does not exist: $BACKUP_DIR"
        log_info "  Create the backup directory before configuring backup automation"
    elif [[ ! -w "$BACKUP_DIR" ]]; then
        log_fail "Backup directory is not writable: $BACKUP_DIR"
    else
        log_pass "Backup directory exists and is writable: $BACKUP_DIR"

        # Check for existing backup files (optional, just informational)
        BACKUP_COUNT=$(find "$BACKUP_DIR" -maxdepth 1 -name "ferrumgate-backup-*.db" 2>/dev/null | wc -l || echo "0")
        if [[ "$BACKUP_COUNT" -gt 0 ]]; then
            log_info "Found $BACKUP_COUNT existing backup(s) in $BACKUP_DIR"
        else
            log_info "No existing backups found in $BACKUP_DIR (this is OK for initial setup)"
        fi
    fi
fi
echo ""

# --- Check 6: Systemd timer/cron availability hints ---
echo "========================================"
echo "CHECK: Scheduler availability hints"
echo "========================================"

SYSTEMD_AVAILABLE=false
CRON_AVAILABLE=false

if command -v systemctl >/dev/null 2>&1; then
    if systemctl --version >/dev/null 2>&1; then
        SYSTEMD_AVAILABLE=true
        log_info "systemd is available on this host"
    fi
fi

if command -v crontab >/dev/null 2>&1; then
    CRON_AVAILABLE=true
    log_info "cron is available on this host"
fi

if [[ "$SYSTEMD_AVAILABLE" == "true" ]]; then
    log_pass "systemd detected; systemd timer configuration is applicable"
    log_info "  Example timer config: configs/examples/ferrumgate-backup.timer"
    log_info "  Example service: configs/examples/ferrumgate-backup.service"
elif [[ "$CRON_AVAILABLE" == "true" ]]; then
    log_pass "cron detected; cron-based backup configuration is applicable"
    log_info "  Example cron config: configs/examples/ferrumgate-backup.cron"
else
    log_warn "Neither systemd nor cron detected on this host"
    log_info "  Backup automation requires a scheduler (systemd timer or cron)"
fi
echo ""

# --- Check 7: ferrumctl availability and basic commands ---
echo "========================================"
echo "CHECK: ferrumctl availability"
echo "========================================"

FERRUMCTL_BIN="${FERRUMCTL_BIN:-}"
if [[ -z "$FERRUMCTL_BIN" ]]; then
    # Try to find ferrumctl
    for candidate in \
        "$REPO_ROOT/target/release/ferrumctl" \
        "$REPO_ROOT/target/debug/ferrumctl" \
        "$(command -v ferrumctl 2>/dev/null || true)"; do
        if [[ -n "$candidate" ]] && [[ -x "$candidate" ]]; then
            FERRUMCTL_BIN="$candidate"
            break
        fi
    done
fi

if [[ -z "$FERRUMCTL_BIN" ]] || [[ ! -x "$FERRUMCTL_BIN" ]]; then
    log_warn "ferrumctl not found or not executable"
    log_info "  Build with: cargo build --release -p ferrumctl"
    log_info "  ferrumctl is needed for readiness and backup verification"
else
    log_pass "ferrumctl found: $FERRUMCTL_BIN"

    # Check ferrumctl version (if supported)
    if "$FERRUMCTL_BIN" --version >/dev/null 2>&1; then
        VERSION=$("$FERRUMCTL_BIN" --version 2>&1 | head -1 || echo "unknown")
        log_info "ferrumctl version: $VERSION"
    fi

    # Check ferrumctl readiness (requires target to be running)
    if [[ "$DRY_RUN" == "true" ]]; then
        log_skip "Dry-run mode: skipping ferrumctl readiness check"
    elif [[ -z "$BASE_URL" ]]; then
        log_skip "Base URL not provided; skipping ferrumctl readiness check"
    else
        log_info "Checking ferrumctl server readiness --deep against $BASE_URL..."
        READINESS_CMD=("$FERRUMCTL_BIN" --server-url "$BASE_URL")
        if [[ -n "$BEARER_TOKEN" ]]; then
            READINESS_CMD+=(--bearer-token "$BEARER_TOKEN")
        fi
        READINESS_CMD+=(server readiness --deep)
        if "${READINESS_CMD[@]}" >/dev/null 2>&1; then
            log_pass "ferrumctl server readiness --deep passed"
        else
            log_warn "ferrumctl server readiness --deep returned non-zero"
            log_info "  Ensure ferrumd is running at $BASE_URL"
        fi
    fi

    # Check ferrumctl backup verify (requires explicit --db-path)
    if [[ -z "$DB_PATH" ]]; then
        log_skip "DB path not provided; skipping ferrumctl backup verify (use --db-path or FERRUMD_DB_PATH)"
    elif [[ "$DRY_RUN" == "true" ]]; then
        log_skip "Dry-run mode: skipping ferrumctl backup verify"
    else
        if [[ ! -f "$DB_PATH" ]]; then
            log_fail "DB path does not exist: $DB_PATH"
        elif [[ ! -r "$DB_PATH" ]]; then
            log_fail "DB path is not readable: $DB_PATH"
        else
            log_info "Checking ferrumctl backup verify --db-path $DB_PATH..."
            if "$FERRUMCTL_BIN" backup verify --db-path "$DB_PATH" >/dev/null 2>&1; then
                log_pass "ferrumctl backup verify passed"
            else
                log_warn "ferrumctl backup verify returned non-zero"
                log_info "  Verify the DB file is a valid SQLite database"
            fi
        fi
    fi
fi
echo ""

# --- Check 8: Example configs presence ---
echo "========================================"
echo "CHECK: Example configs presence"
echo "========================================"

EXAMPLE_CONFIGS=(
    "configs/examples/ferrumd.service"
    "configs/examples/ferrumgate-backup.service"
    "configs/examples/ferrumgate-backup.timer"
    "configs/examples/ferrumgate-backup.cron"
)

ALL_PRESENT=true
for cfg in "${EXAMPLE_CONFIGS[@]}"; do
    if [[ -f "$REPO_ROOT/$cfg" ]]; then
        log_pass "Example config present: $cfg"
    else
        log_fail "Example config missing: $cfg"
        ALL_PRESENT=false
    fi
done

if [[ "$ALL_PRESENT" == "true" ]]; then
    log_pass "All required example configs are present"
fi
echo ""

# --- Summary ---
echo "========================================"
echo "PREFLIGHT RESULT"
echo "========================================"
echo ""
echo "Failed:  $FAILED"
echo "Skipped: $SKIPPED"
echo "Warnings: $WARNED"
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo "PREFLIGHT: ALL APPLICABLE CHECKS PASSED"
    echo ""
    echo "NEXT STEPS (operator-owned):"
    echo "  1. Ensure target host meets requirements in doc 63"
    echo "  2. Generate real bearer token: openssl rand -hex 32"
    echo "  3. Adapt example configs to target environment"
    echo "  4. Deploy ferrumd.service and backup service to target"
    echo "  5. Run Phase 2 probes and D1-D6 drills on target"
    echo "  6. Complete doc 59 G2.1-G2.8 and sign doc 54"
    echo ""
    echo "G2 gates remain PENDING until operator signs doc 59 and doc 54."
    echo "No production-ready claim. FerrumGate v1 remains RC-ready/conditional."
    echo ""
    exit 0
else
    echo "PREFLIGHT: SOME CHECKS FAILED"
    echo ""
    echo "Fix the failed checks before proceeding to Phase 2."
    echo "If ferrumctl was skipped, build with: cargo build --release -p ferrumctl"
    echo ""
    exit 1
fi
