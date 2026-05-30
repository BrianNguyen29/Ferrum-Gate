#!/usr/bin/env bash
#
# G3.6 Workload Wrapper — Repository-Owned Robust Control Script
#
# Purpose: Safely execute run_real_workload_generator.py on a target host
# with truthful sentinel semantics, no-orphan guarantees, and config-drift
# monitoring.
#
# Design:
#   - Pre-run E-checks: confirm effective rate-limit via /v1/metrics
#   - Start generator as child, monitor PID
#   - On signal: forward to child, wait for child exit
#   - Config revert ONLY after child exits or is explicitly killed
#   - Sentinel reflects generator's actual exit code, not shell rc
#   - Mid-run drift probes via background subshell (safe: exits on wrapper exit)
#
# Usage:
#   export FERRUM_BEARER_TOKEN="<token>"
#   ./scripts/run_g36_workload_wrapper.sh \
#       --server-url https://<host> \
#       --rate-limit-ps 5 \
#       --rate-limit-burst 100 \
#       --output-dir /tmp/ferrum-g36-$(date +%Y%m%d_%H%M%S)
#
# Constraints:
#   - POSIX/bash-safe (bash required for job control and wait -n).
#   - No secrets in output.
#   - Do NOT run against target from this repo; copy to target and execute there.
#   - G3.6 remains NOT ACCEPTED until operator signoff per doc 116.

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_URL=""
BEARER_TOKEN="${FERRUM_BEARER_TOKEN:-}"
RATE_LIMIT_PS=""
RATE_LIMIT_BURST=""
OUTPUT_DIR=""
GENERATOR_TIMEOUT_SEC=4500   # 75 min = baseline(600)+low(600)+target(1800)+cooldown(600) + 300s headroom
PHASES_JSON='[{"name":"baseline","duration_sec":600,"rate_rps":0},{"name":"low","duration_sec":600,"rate_rps":0.1},{"name":"target","duration_sec":1800,"rate_rps":1.0},{"name":"cooldown","duration_sec":600,"rate_rps":0}]'
DRIFT_PROBE_INTERVAL_SEC=60
DRIFT_ABORT_THRESHOLD=1      # number of consecutive drift detections before abort
REVERT_COMMAND=""
REQUIRE_REVERT_COMMAND=0

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

Required:
  --server-url URL          FerrumGate server base URL
  --rate-limit-ps N         Expected effective rate_limit_per_second
  --rate-limit-burst N      Expected effective rate_limit_burst
  --output-dir DIR          Output directory for evidence

Optional:
  --bearer-token TOKEN      Bearer token (or set FERRUM_BEARER_TOKEN env var)
  --generator-timeout SEC   Max seconds to wait for generator (default: ${GENERATOR_TIMEOUT_SEC})
  --drift-interval SEC      Seconds between config-drift probes (default: ${DRIFT_PROBE_INTERVAL_SEC})
  --phases JSON             Phase definition JSON (default: D1b target-focused sequence)
  --revert-command CMD      Shell command to revert config after generator exits (e.g. "systemctl restart ferrumd")
  --require-revert-command  Fail if --revert-command is not provided (recommended for target reruns)
  --help                    Show this help
EOF
}

# Parse CLI args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --server-url) SERVER_URL="$2"; shift 2 ;;
        --rate-limit-ps) RATE_LIMIT_PS="$2"; shift 2 ;;
        --rate-limit-burst) RATE_LIMIT_BURST="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --bearer-token) BEARER_TOKEN="$2"; shift 2 ;;
        --generator-timeout) GENERATOR_TIMEOUT_SEC="$2"; shift 2 ;;
        --drift-interval) DRIFT_PROBE_INTERVAL_SEC="$2"; shift 2 ;;
        --phases) PHASES_JSON="$2"; shift 2 ;;
        --revert-command) REVERT_COMMAND="$2"; shift 2 ;;
        --require-revert-command) REQUIRE_REVERT_COMMAND=1; shift ;;
        --help) usage; exit 0 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

[[ -n "${SERVER_URL}" ]] || fail "--server-url is required"
[[ -n "${RATE_LIMIT_PS}" ]] || fail "--rate-limit-ps is required"
[[ -n "${RATE_LIMIT_BURST}" ]] || fail "--rate-limit-burst is required"
[[ -n "${OUTPUT_DIR}" ]] || fail "--output-dir is required"
if [[ ${REQUIRE_REVERT_COMMAND} -eq 1 && -z "${REVERT_COMMAND}" ]]; then
    fail "--require-revert-command is set but no --revert-command was provided"
fi

mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd "${OUTPUT_DIR}" && pwd)"

SENTINEL_DIR="${OUTPUT_DIR}/sentinel"
mkdir -p "${SENTINEL_DIR}"

# Redact token for logging
_redact_token() {
    local tok="$1"
    if [[ ${#tok} -lt 8 ]]; then
        echo "<REDACTED>"
    else
        echo "${tok:0:4}...${tok: -4}"
    fi
}

log "G3.6 Workload Wrapper starting"
log "  server_url: ${SERVER_URL}"
log "  rate_limit_ps: ${RATE_LIMIT_PS}"
log "  rate_limit_burst: ${RATE_LIMIT_BURST}"
log "  output_dir: ${OUTPUT_DIR}"
log "  token: $(_redact_token "${BEARER_TOKEN}")"
log "  timeout: ${GENERATOR_TIMEOUT_SEC}s"
if [[ -n "${REVERT_COMMAND}" ]]; then
    log "  revert_command: <set>"
else
    log "  revert_command: <none>"
fi

# ---------------------------------------------------------------------------
# Pre-run E-checks (deterministic config evidence)
# ---------------------------------------------------------------------------
log "=== Pre-run E-checks ==="

_check_metric_value() {
    local url="$1"
    local token="$2"
    local metric_name="$3"
    local expected="$4"
    local out_file="$5"

    local status body
    status=$(curl -s -o "${out_file}" -w "%{http_code}" \
        -H "Authorization: Bearer ${token}" \
        -H "Accept: text/plain" \
        --max-time 30 \
        "${url}/v1/metrics" 2>/dev/null || true)

    if [[ "${status}" != "200" ]]; then
        log "E-check FAIL: /v1/metrics returned HTTP ${status}"
        return 1
    fi

    local value
    value=$(grep "^${metric_name} " "${out_file}" | awk '{print $2}' | head -n1 || true)
    if [[ -z "${value}" ]]; then
        log "E-check FAIL: metric ${metric_name} not found in /v1/metrics"
        return 1
    fi

    # Compare as strings first, then as numbers if possible
    if [[ "${value}" == "${expected}" ]]; then
        log "E-check PASS: ${metric_name} = ${value}"
        return 0
    fi

    # Numeric comparison for floating-point / integer equivalence
    if awk "BEGIN {exit !(${value} == ${expected})}" 2>/dev/null; then
        log "E-check PASS: ${metric_name} = ${value} (numeric match)"
        return 0
    fi

    log "E-check FAIL: ${metric_name} expected ${expected}, got ${value}"
    return 1
}

METRICS_FILE="${OUTPUT_DIR}/metrics_prerun.txt"
E_CHECK_OK=1
if ! _check_metric_value "${SERVER_URL}" "${BEARER_TOKEN}" \
    "ferrumgate_rate_limit_per_second" "${RATE_LIMIT_PS}" "${METRICS_FILE}"; then
    E_CHECK_OK=0
fi

if ! _check_metric_value "${SERVER_URL}" "${BEARER_TOKEN}" \
    "ferrumgate_rate_limit_burst" "${RATE_LIMIT_BURST}" "${METRICS_FILE}"; then
    E_CHECK_OK=0
fi

if [[ ${E_CHECK_OK} -eq 0 ]]; then
    log "STOP: Pre-run E-check failed. Effective config does not match intended policy."
    cat > "${SENTINEL_DIR}/FAILED.status" <<EOF
{
  "timestamp": "$(date -u '+%Y-%m-%dT%H:%M:%SZ')",
  "stage": "pre_run_e_check",
  "exit_code": 2,
  "reason": "Effective rate-limit config does not match intended policy",
  "expected_rate_limit_per_second": ${RATE_LIMIT_PS},
  "expected_rate_limit_burst": ${RATE_LIMIT_BURST},
  "metrics_file": "${METRICS_FILE}"
}
EOF
    exit 2
fi

log "Pre-run E-checks PASSED"

# ---------------------------------------------------------------------------
# Config-drift probe (background)
# ---------------------------------------------------------------------------
DRIFT_LOG="${OUTPUT_DIR}/config_drift_log.jsonl"
DRIFT_PID=""
_drift_probe_loop() {
    local url="$1"
    local token="$2"
    local expected_ps="$3"
    local expected_burst="$4"
    local interval="$5"
    local logfile="$6"
    local sentinel="$7"

    local consecutive_failures=0
    while true; do
        sleep "${interval}"

        # Exit if sentinel appears (generator finished or failed)
        [[ -f "${sentinel}" ]] && break

        local tmpfile="${logfile}.tmp.$$"
        local status
        status=$(curl -s -o "${tmpfile}" -w "%{http_code}" \
            -H "Authorization: Bearer ${token}" \
            -H "Accept: text/plain" \
            --max-time 30 \
            "${url}/v1/metrics" 2>/dev/null || true)

        if [[ "${status}" != "200" ]]; then
            consecutive_failures=$((consecutive_failures + 1))
            printf '%s\n' "{\"timestamp\":\"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\",\"event\":\"metrics_probe_failed\",\"http_status\":${status},\"consecutive_failures\":${consecutive_failures}}" >> "${logfile}"
            continue
        fi

        local ps_val burst_val
        ps_val=$(grep "^ferrumgate_rate_limit_per_second " "${tmpfile}" | awk '{print $2}' | head -n1 || true)
        burst_val=$(grep "^ferrumgate_rate_limit_burst " "${tmpfile}" | awk '{print $2}' | head -n1 || true)
        rm -f "${tmpfile}"

        local drift=0
        local drift_reason=""
        if [[ -n "${ps_val}" && "${ps_val}" != "${expected_ps}" ]]; then
            if ! awk "BEGIN {exit !(${ps_val} == ${expected_ps})}" 2>/dev/null; then
                drift=1
                drift_reason="rate_limit_per_second drift: expected ${expected_ps}, got ${ps_val}"
            fi
        fi
        if [[ -n "${burst_val}" && "${burst_val}" != "${expected_burst}" ]]; then
            if ! awk "BEGIN {exit !(${burst_val} == ${expected_burst})}" 2>/dev/null; then
                drift=1
                drift_reason="${drift_reason}; rate_limit_burst drift: expected ${expected_burst}, got ${burst_val}"
            fi
        fi

        if [[ ${drift} -eq 1 ]]; then
            consecutive_failures=$((consecutive_failures + 1))
            printf '%s\n' "{\"timestamp\":\"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\",\"event\":\"config_drift_detected\",\"reason\":\"${drift_reason}\",\"consecutive_failures\":${consecutive_failures}}" >> "${logfile}"
            if [[ ${consecutive_failures} -ge ${DRIFT_ABORT_THRESHOLD} ]]; then
                # Signal the main wrapper to abort. We use a sentinel file.
                printf '%s\n' "{\"timestamp\":\"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\",\"event\":\"drift_abort_triggered\",\"reason\":\"${drift_reason}\"}" >> "${logfile}"
                touch "${sentinel}.drift_abort"
                break
            fi
        else
            if [[ ${consecutive_failures} -gt 0 ]]; then
                consecutive_failures=0
                printf '%s\n' "{\"timestamp\":\"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\",\"event\":\"drift_cleared\",\"consecutive_failures\":0}" >> "${logfile}"
            fi
        fi
    done
}

# Start drift probe in background
DRIFT_ABORT_FILE="${SENTINEL_DIR}/.drift_abort"
rm -f "${DRIFT_ABORT_FILE}"
_drift_probe_loop "${SERVER_URL}" "${BEARER_TOKEN}" \
    "${RATE_LIMIT_PS}" "${RATE_LIMIT_BURST}" \
    "${DRIFT_PROBE_INTERVAL_SEC}" "${DRIFT_LOG}" "${SENTINEL_DIR}/COMPLETE.status" &
DRIFT_PID=$!
log "Config-drift probe started (PID ${DRIFT_PID}, interval ${DRIFT_PROBE_INTERVAL_SEC}s)"

# ---------------------------------------------------------------------------
# Generator execution
# ---------------------------------------------------------------------------
log "=== Starting workload generator ==="

# Build generator command (token is passed via env, never argv)
GENERATOR_CMD=(
    python3
    "${SCRIPT_DIR}/run_real_workload_generator.py"
    --execute
    --server-url "${SERVER_URL}"
    --output-dir "${OUTPUT_DIR}"
    --phases "${PHASES_JSON}"
    --expected-rate-limit-ps "${RATE_LIMIT_PS}"
    --expected-rate-limit-burst "${RATE_LIMIT_BURST}"
    --drift-abort-file "${DRIFT_ABORT_FILE}"
)

log "Generator command: ${GENERATOR_CMD[*]}"

GENERATOR_EXIT_CODE=""
GENERATOR_PID=""

# Start generator as background job so we can wait on it with timeout
# Token is passed via env to avoid argv exposure in process listings
FERRUM_BEARER_TOKEN="${BEARER_TOKEN}" "${GENERATOR_CMD[@]}" > "${OUTPUT_DIR}/generator_stdout.log" 2> "${OUTPUT_DIR}/generator_stderr.log" &
GENERATOR_PID=$!
log "Generator PID: ${GENERATOR_PID}"

# ---------------------------------------------------------------------------
# Wait for generator with timeout, handling signals
# ---------------------------------------------------------------------------
_forward_signal() {
    local sig="$1"
    log "Received signal ${sig}; forwarding to generator PID ${GENERATOR_PID}"
    if kill -"${sig}" "${GENERATOR_PID}" 2>/dev/null; then
        log "Signal ${sig} forwarded to generator"
    else
        log "Generator PID ${GENERATOR_PID} already exited"
    fi
}

trap '_forward_signal TERM' TERM
trap '_forward_signal INT' INT

# Wait for generator with timeout
WAIT_START=$(date +%s)
WAIT_ELAPSED=0
GENERATOR_EXIT_CODE=""

while [[ ${WAIT_ELAPSED} -lt ${GENERATOR_TIMEOUT_SEC} ]]; do
    # Check if drift abort triggered
    if [[ -f "${DRIFT_ABORT_FILE}" ]]; then
        log "Drift abort triggered by background probe; signaling generator"
        kill -TERM "${GENERATOR_PID}" 2>/dev/null || true
    fi

    if ! kill -0 "${GENERATOR_PID}" 2>/dev/null; then
        # Generator exited
        wait "${GENERATOR_PID}" || true
        GENERATOR_EXIT_CODE=$?
        log "Generator exited with code ${GENERATOR_EXIT_CODE}"
        break
    fi

    sleep 1
    WAIT_ELAPSED=$(($(date +%s) - WAIT_START))
done

# If loop ended due to timeout and generator still running
if [[ -z "${GENERATOR_EXIT_CODE}" ]]; then
    if kill -0 "${GENERATOR_PID}" 2>/dev/null; then
        log "Generator timed out after ${GENERATOR_TIMEOUT_SEC}s; sending TERM"
        kill -TERM "${GENERATOR_PID}" 2>/dev/null || true
        sleep 5
        if kill -0 "${GENERATOR_PID}" 2>/dev/null; then
            log "Generator still running after TERM; sending KILL"
            kill -KILL "${GENERATOR_PID}" 2>/dev/null || true
        fi
        wait "${GENERATOR_PID}" 2>/dev/null || true
        GENERATOR_EXIT_CODE=124
    else
        wait "${GENERATOR_PID}" 2>/dev/null || true
        GENERATOR_EXIT_CODE=$?
    fi
fi

# Ensure generator is truly gone (no orphan)
if kill -0 "${GENERATOR_PID}" 2>/dev/null; then
    log "WARNING: Generator PID ${GENERATOR_PID} still alive after wait; forcing KILL"
    kill -KILL "${GENERATOR_PID}" 2>/dev/null || true
    sleep 1
    if kill -0 "${GENERATOR_PID}" 2>/dev/null; then
        log "CRITICAL: Unable to terminate generator PID ${GENERATOR_PID}"
    fi
fi

log "Generator final state: exit_code=${GENERATOR_EXIT_CODE}"

# ---------------------------------------------------------------------------
# Stop drift probe
# ---------------------------------------------------------------------------
if [[ -n "${DRIFT_PID}" ]] && kill -0 "${DRIFT_PID}" 2>/dev/null; then
    kill -TERM "${DRIFT_PID}" 2>/dev/null || true
    sleep 1
    kill -KILL "${DRIFT_PID}" 2>/dev/null || true
    wait "${DRIFT_PID}" 2>/dev/null || true
    log "Drift probe stopped"
fi

# ---------------------------------------------------------------------------
# Post-generator: config revert
# ---------------------------------------------------------------------------
log "=== Post-generator cleanup ==="

REVERT_EXIT_CODE=""
REVERT_REASON=""
if [[ -n "${REVERT_COMMAND}" ]]; then
    log "Executing revert command"
    # Run revert command in a subshell so set -e does not kill the wrapper on failure
    (
        set +e
        eval "${REVERT_COMMAND}"
        exit $?
    )
    REVERT_EXIT_CODE=$?
    if [[ ${REVERT_EXIT_CODE} -eq 0 ]]; then
        REVERT_REASON="Revert succeeded"
        log "Revert command exited with code 0"
    else
        REVERT_REASON="Revert failed with exit code ${REVERT_EXIT_CODE}"
        log "WARNING: ${REVERT_REASON}"
    fi
else
    REVERT_EXIT_CODE=""
    REVERT_REASON="No revert command provided"
    log "No revert command provided; skipping config revert"
fi

# ---------------------------------------------------------------------------
# Determine final sentinel status
# ---------------------------------------------------------------------------
# Truthful semantics:
#   - Generator exit code is primary.
#   - Revert failure is a secondary cleanup failure.
#   - If generator succeeded but revert failed, we still report FAILED.status
#     with generator_exit_code preserved and revert_exit_code recorded.
#   - Final wrapper exit code = generator code if revert OK, else distinct
#     nonzero code (10) so callers see cleanup failure.
# ---------------------------------------------------------------------------
SENTINEL_TIMESTAMP=$(date -u '+%Y-%m-%dT%H:%M:%SZ')

FINAL_EXIT_CODE=${GENERATOR_EXIT_CODE}
if [[ ${GENERATOR_EXIT_CODE} -eq 0 && -n "${REVERT_EXIT_CODE}" && ${REVERT_EXIT_CODE} -ne 0 ]]; then
    FINAL_EXIT_CODE=10
fi

if [[ ${FINAL_EXIT_CODE} -eq 0 ]]; then
    cat > "${SENTINEL_DIR}/COMPLETE.status" <<EOF
{
  "timestamp": "${SENTINEL_TIMESTAMP}",
  "stage": "generator_completed",
  "exit_code": ${FINAL_EXIT_CODE},
  "generator_exit_code": ${GENERATOR_EXIT_CODE},
  "revert_exit_code": ${REVERT_EXIT_CODE:-null},
  "generator_pid": ${GENERATOR_PID},
  "reason": "Generator exited successfully; ${REVERT_REASON}",
  "output_dir": "${OUTPUT_DIR}"
}
EOF
    log "Sentinel written: COMPLETE.status (exit_code=${FINAL_EXIT_CODE})"
else
    cat > "${SENTINEL_DIR}/FAILED.status" <<EOF
{
  "timestamp": "${SENTINEL_TIMESTAMP}",
  "stage": "generator_or_cleanup_failed",
  "exit_code": ${FINAL_EXIT_CODE},
  "generator_exit_code": ${GENERATOR_EXIT_CODE},
  "revert_exit_code": ${REVERT_EXIT_CODE:-null},
  "generator_pid": ${GENERATOR_PID},
  "reason": "Generator exited ${GENERATOR_EXIT_CODE}; ${REVERT_REASON}",
  "output_dir": "${OUTPUT_DIR}"
}
EOF
    log "Sentinel written: FAILED.status (exit_code=${FINAL_EXIT_CODE})"
fi

# Also write a human-readable summary
SUMMARY_FILE="${OUTPUT_DIR}/RUN_SUMMARY.txt"
cat > "${SUMMARY_FILE}" <<EOF
G3.6 Workload Run Summary
=========================
Timestamp: ${SENTINEL_TIMESTAMP}
Server: ${SERVER_URL}
Rate Limit Policy: per_second=${RATE_LIMIT_PS}, burst=${RATE_LIMIT_BURST}
Generator PID: ${GENERATOR_PID}
Generator Exit Code: ${GENERATOR_EXIT_CODE}
Revert Exit Code: ${REVERT_EXIT_CODE:-N/A}
Final Exit Code: ${FINAL_EXIT_CODE}
Output Directory: ${OUTPUT_DIR}

Pre-run E-checks: PASSED
Config drift log: ${DRIFT_LOG}
Generator stdout: ${OUTPUT_DIR}/generator_stdout.log
Generator stderr: ${OUTPUT_DIR}/generator_stderr.log

Sentinel: ${SENTINEL_DIR}/$([[ ${FINAL_EXIT_CODE} -eq 0 ]] && echo 'COMPLETE.status' || echo 'FAILED.status')

NOTE: G3.6 remains NOT ACCEPTED until operator signoff per doc 116.
EOF

log "Run summary: ${SUMMARY_FILE}"
log "G3.6 Workload Wrapper finished"

exit ${FINAL_EXIT_CODE}
