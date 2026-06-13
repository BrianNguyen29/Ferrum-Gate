#!/usr/bin/env bash
#
# G3.6 D1b Rehearsal Script — Target Side
#
# Purpose: Safely apply D1b test-window rate-limit config, run a short rehearsal
# workload via the robust wrapper, and revert config on completion or failure.
#
# Design:
#   - Backs up /etc/ferrumgate/env before any change.
#   - Removes existing FERRUMD_RATE_LIMIT_PER_SECOND/BURST lines.
#   - Appends D1b values (5/100).
#   - Restarts ferrumgate.service.
#   - Outer trap reverts from backup on unexpected failure BEFORE wrapper starts.
#   - Invokes wrapper with --revert-command that uses the exact backup path.
#   - Does not print secrets.
#   - Conservative defaults for rehearsal only.
#
# Usage (on target host as root/sudo):
#   sudo ./run_g36_d1b_rehearsal_target.sh
#
#   # Custom phases
#   sudo ./run_g36_d1b_rehearsal_target.sh --phases '[{"name":"baseline","duration_sec":10,"rate_rps":0},{"name":"target","duration_sec":60,"rate_rps":1.0}]'
#
# Constraints:
#   - Intended to run on target host as root or with passwordless sudo.
#   - No secrets in output.
#   - This is a REHEARSAL, not G3.6 acceptance.
#   - G3.6 remains NOT ACCEPTED.

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_URL="http://127.0.0.1:19080"
RATE_LIMIT_PS=5
RATE_LIMIT_BURST=100
ENV_FILE="/etc/ferrumgate/env"
OUTPUT_DIR="/tmp/ferrum-g36-d1b-rehearsal-$(date +%Y%m%d_%H%M%S)"
PHASES_JSON='[{"name":"baseline","duration_sec":5,"rate_rps":0},{"name":"target","duration_sec":20,"rate_rps":1.0},{"name":"cooldown","duration_sec":5,"rate_rps":0}]'
WRAPPER_STARTED=0
BACKUP_FILE=""
CONFIRM=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log() {
    printf '[%s] %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*"
}

fail() {
    log "FAIL: $*"
    exit 1
}

usage() {
    cat <<EOF
Usage: ${BASH_SOURCE[0]} [OPTIONS]

Target-side D1b rehearsal script. Applies test-window rate-limit config,
runs a short workload, and reverts config on completion or failure.

Optional:
  --confirm                 REQUIRED. Confirm this script may mutate system config and restart the service.
  --server-url URL          FerrumGate server base URL (default: ${SERVER_URL})
  --rate-limit-ps N         D1b rate_limit_per_second (default: ${RATE_LIMIT_PS})
  --rate-limit-burst N      D1b rate_limit_burst (default: ${RATE_LIMIT_BURST})
  --env-file PATH           Path to env file (default: ${ENV_FILE})
  --output-dir DIR          Output directory (default: ${OUTPUT_DIR})
  --phases JSON             Phase definition JSON (default: short rehearsal)
  --help                    Show this help
EOF
}

# Parse CLI args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --confirm) CONFIRM=1; shift ;;
        --server-url) SERVER_URL="$2"; shift 2 ;;
        --rate-limit-ps) RATE_LIMIT_PS="$2"; shift 2 ;;
        --rate-limit-burst) RATE_LIMIT_BURST="$2"; shift 2 ;;
        --env-file) ENV_FILE="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --phases) PHASES_JSON="$2"; shift 2 ;;
        --help) usage; exit 0 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

# ---------------------------------------------------------------------------
# Safety confirmation guard
# ---------------------------------------------------------------------------
if [[ ${CONFIRM} -eq 0 ]]; then
    log "ERROR: This script mutates ${ENV_FILE} and restarts ferrumgate.service."
    log "Re-run with --confirm to proceed."
    exit 2
fi

# ---------------------------------------------------------------------------
# Outer trap: revert config if script fails BEFORE wrapper takes over
# ---------------------------------------------------------------------------
_cleanup_on_exit() {
    local rc=$?
    if [[ ${WRAPPER_STARTED} -eq 0 && -n "${BACKUP_FILE}" && -f "${BACKUP_FILE}" ]]; then
        log "Outer trap: reverting config from backup (script rc=${rc})"
        if cp "${BACKUP_FILE}" "${ENV_FILE}"; then
            systemctl restart ferrumgate.service || log "WARNING: service restart failed during outer trap revert"
            log "Outer trap: config reverted from ${BACKUP_FILE}"
        else
            log "CRITICAL: outer trap revert failed (cp ${BACKUP_FILE} ${ENV_FILE})"
        fi
    fi
}
trap '_cleanup_on_exit' EXIT

# ---------------------------------------------------------------------------
# Validate prerequisites
# ---------------------------------------------------------------------------
log "=== D1b Rehearsal Starting ==="
log "  server_url: ${SERVER_URL}"
log "  rate_limit: ${RATE_LIMIT_PS}/${RATE_LIMIT_BURST}"
log "  env_file: ${ENV_FILE}"
log "  output_dir: ${OUTPUT_DIR}"

if [[ ! -f "${ENV_FILE}" ]]; then
    fail "Env file not found: ${ENV_FILE}"
fi

# Verify service exists
if ! systemctl status ferrumgate.service &>/dev/null; then
    log "WARNING: ferrumgate.service status check failed; proceeding anyway"
fi

# ---------------------------------------------------------------------------
# Backup env file
# ---------------------------------------------------------------------------
BACKUP_FILE="${ENV_FILE}.backup.$(date +%Y%m%d_%H%M%S)"
cp "${ENV_FILE}" "${BACKUP_FILE}"
log "Env file backed up to: ${BACKUP_FILE}"

# ---------------------------------------------------------------------------
# Apply D1b config
# ---------------------------------------------------------------------------
log "Applying D1b rate-limit config..."

# Remove existing rate-limit lines, then append new values
grep -v '^FERRUMD_RATE_LIMIT_PER_SECOND=' "${ENV_FILE}" | \
grep -v '^FERRUMD_RATE_LIMIT_BURST=' > "${ENV_FILE}.tmp"

printf 'FERRUMD_RATE_LIMIT_PER_SECOND=%s\n' "${RATE_LIMIT_PS}" >> "${ENV_FILE}.tmp"
printf 'FERRUMD_RATE_LIMIT_BURST=%s\n' "${RATE_LIMIT_BURST}" >> "${ENV_FILE}.tmp"

mv "${ENV_FILE}.tmp" "${ENV_FILE}"

log "Env file updated with D1b values"

# ---------------------------------------------------------------------------
# Restart service
# ---------------------------------------------------------------------------
log "Restarting ferrumgate.service..."
if systemctl restart ferrumgate.service; then
    log "Service restarted successfully"
else
    fail "Failed to restart ferrumgate.service"
fi

log "Waiting 3s for service to stabilize..."
sleep 3

# ---------------------------------------------------------------------------
# Source bearer token from env file (do not print)
# ---------------------------------------------------------------------------
# Unset any existing token first to avoid leakage from caller env
unset FERRUMD_BEARER_TOKEN 2>/dev/null || true
unset FERRUM_BEARER_TOKEN 2>/dev/null || true

set -a
# shellcheck source=/dev/null
source "${ENV_FILE}"
set +a

if [[ -z "${FERRUMD_BEARER_TOKEN:-}" ]]; then
    fail "FERRUMD_BEARER_TOKEN not found in ${ENV_FILE}"
fi

export FERRUM_BEARER_TOKEN="${FERRUMD_BEARER_TOKEN}"
log "Bearer token loaded from env file (redacted)"

# ---------------------------------------------------------------------------
# Build revert command for wrapper
# ---------------------------------------------------------------------------
# The wrapper will execute this after the generator exits.
REVERT_CMD="cp ${BACKUP_FILE} ${ENV_FILE} && systemctl restart ferrumgate.service && sleep 3"

# ---------------------------------------------------------------------------
# Build wrapper command
# ---------------------------------------------------------------------------
WRAPPER_CMD=(
    bash
    "${SCRIPT_DIR}/run_g36_workload_wrapper.sh"
    --server-url "${SERVER_URL}"
    --rate-limit-ps "${RATE_LIMIT_PS}"
    --rate-limit-burst "${RATE_LIMIT_BURST}"
    --output-dir "${OUTPUT_DIR}"
    --phases "${PHASES_JSON}"
    --revert-command "${REVERT_CMD}"
    --require-revert-command
)

log "=== Starting wrapper ==="
log "  output_dir: ${OUTPUT_DIR}"

# Mark that wrapper is starting; outer trap should no longer revert
WRAPPER_STARTED=1

mkdir -p "${OUTPUT_DIR}"
WRAPPER_RC=0
"${WRAPPER_CMD[@]}" > "${OUTPUT_DIR}/wrapper_stdout.log" 2> "${OUTPUT_DIR}/wrapper_stderr.log" || WRAPPER_RC=$?

log "Wrapper exited with code: ${WRAPPER_RC}"

# ---------------------------------------------------------------------------
# Post-wrapper status check
# ---------------------------------------------------------------------------
log "Checking service status..."
systemctl status ferrumgate.service --no-pager || log "WARNING: service status check returned non-zero"

# Verify effective config from metrics (lightweight)
log "Verifying effective rate-limit config post-run..."
METRICS_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -H "Authorization: Bearer ${FERRUM_BEARER_TOKEN}" \
    "${SERVER_URL}/v1/metrics" 2>/dev/null || true)
log "  /v1/metrics HTTP status: ${METRICS_STATUS}"

log "=== Rehearsal Complete ==="
log "  Output directory: ${OUTPUT_DIR}"
log "  Wrapper exit code: ${WRAPPER_RC}"
log "  Sentinel: ${OUTPUT_DIR}/sentinel/"

# Print concise summary to stdout
printf '\n--- D1b Rehearsal Summary ---\n'
printf 'Output directory: %s\n' "${OUTPUT_DIR}"
printf 'Wrapper exit code: %d\n' "${WRAPPER_RC}"
printf 'Service status: %s\n' "$(systemctl is-active ferrumgate.service 2>/dev/null || echo 'unknown')"
printf 'Metrics endpoint: HTTP %s\n' "${METRICS_STATUS}"
printf 'NOTE: This is a rehearsal. G3.6 remains NOT ACCEPTED.\n'

exit ${WRAPPER_RC}
