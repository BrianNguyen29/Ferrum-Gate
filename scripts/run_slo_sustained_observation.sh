#!/usr/bin/env bash
#
# SLO Sustained Observation — Domain-Free Rehearsal & Long-Window Tooling
#
# Purpose:
#   - Run bounded or long-window HTTP observations against any configured endpoint.
#   - Dry-run mode exercises script logic without network calls.
#   - Real mode polls the target and records latency + HTTP status per sample.
#
# Safety:
#   - Defaults are local, short, and low-frequency.
#   - Dry-run is the default when invoked via `make slo-sustained-dry-run`.
#   - Bearer token is read from env (FERRUM_BEARER_TOKEN) and never echoed.
#   - No production-ready or SLO-window-closure claim is produced by this script.
#
# Usage:
#   ./scripts/run_slo_sustained_observation.sh --dry-run
#   ./scripts/run_slo_sustained_observation.sh \
#       --base-url https://<host> \
#       --duration-min 60 \
#       --interval-min 5 \
#       --output-dir /tmp/slo-obs-$(date +%Y%m%d_%H%M%S)
#
# Constraints:
#   - bash required (math, arrays, date formatting).
#   - No secrets in output.
#   - Does NOT close an SLO window; evidence is rehearsal/observation only.

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults (safe / local / short)
# ---------------------------------------------------------------------------
BASE_URL="http://localhost:8080"
DURATION_MIN=5
INTERVAL_MIN=1
OUTPUT_DIR=""
DRY_RUN=0
BEARER_TOKEN="${FERRUM_BEARER_TOKEN:-}"
ENDPOINT="/v1/healthz"
CURL_MAX_TIME=30

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

Options:
  --base-url URL         Target base URL (default: ${BASE_URL})
  --duration-min N       Total observation duration in minutes (default: ${DURATION_MIN})
  --interval-min N       Minutes between samples (default: ${INTERVAL_MIN})
  --output-dir DIR       Evidence output directory (required in real mode; auto-created in dry-run)
  --endpoint PATH        HTTP path to poll (default: ${ENDPOINT})
  --dry-run              Simulate observations without network calls
  --help                 Show this help

Environment:
  FERRUM_BEARER_TOKEN    Bearer token for authenticated endpoints (optional; never logged)

Examples:
  # Safe dry-run (no network):
  ${BASH_SOURCE[0]} --dry-run

  # Short local rehearsal (real HTTP calls):
  ${BASH_SOURCE[0]} --base-url http://localhost:8080 --duration-min 2 --interval-min 1 --output-dir /tmp/slo-rehearsal

  # Longer window against a real host (operator decision required):
  FERRUM_BEARER_TOKEN="<token>" ${BASH_SOURCE[0]} \
      --base-url https://<host> \
      --duration-min 10080 \
      --interval-min 30 \
      --output-dir /tmp/slo-7day-$(date +%Y%m%d_%H%M%S)
EOF
}

# ---------------------------------------------------------------------------
# Parse CLI args
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --base-url) BASE_URL="$2"; shift 2 ;;
        --duration-min) DURATION_MIN="$2"; shift 2 ;;
        --interval-min) INTERVAL_MIN="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --endpoint) ENDPOINT="$2"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --help) usage; exit 0 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done

# Validate numeric args
if ! [[ "${DURATION_MIN}" =~ ^[0-9]+$ ]]; then
    fail "--duration-min must be a positive integer (got: ${DURATION_MIN})"
fi
if ! [[ "${INTERVAL_MIN}" =~ ^[0-9]+$ ]]; then
    fail "--interval-min must be a positive integer (got: ${INTERVAL_MIN})"
fi
if [[ ${INTERVAL_MIN} -lt 1 ]]; then
    fail "--interval-min must be >= 1 minute"
fi
if [[ ${DURATION_MIN} -lt ${INTERVAL_MIN} ]]; then
    fail "--duration-min (${DURATION_MIN}) must be >= --interval-min (${INTERVAL_MIN})"
fi

# ---------------------------------------------------------------------------
# Output directory
# ---------------------------------------------------------------------------
if [[ -z "${OUTPUT_DIR}" ]]; then
    if [[ ${DRY_RUN} -eq 1 ]]; then
        OUTPUT_DIR="/tmp/slo-obs-dryrun-$(date +%Y%m%d_%H%M%S)"
    else
        fail "--output-dir is required in real mode"
    fi
fi

mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd "${OUTPUT_DIR}" && pwd)"

OBSERVATION_LOG="${OUTPUT_DIR}/observations.jsonl"
SUMMARY_MD="${OUTPUT_DIR}/observation_summary.md"
RUN_META="${OUTPUT_DIR}/run_meta.json"

# ---------------------------------------------------------------------------
# Redact token for any incidental logging
# ---------------------------------------------------------------------------
_redact_token() {
    local tok="$1"
    if [[ ${#tok} -lt 8 ]]; then
        echo "<REDACTED>"
    else
        echo "${tok:0:4}...${tok: -4}"
    fi
}

# ---------------------------------------------------------------------------
# JSON string escape (Python stdlib)
# ---------------------------------------------------------------------------
_json_escape() {
    printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()), end="")'
}

# ---------------------------------------------------------------------------
# Run meta
# ---------------------------------------------------------------------------
RUN_START_ISO="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
RUN_START_EPOCH=$(date +%s)
EXPECTED_SAMPLES=$(( (DURATION_MIN + INTERVAL_MIN - 1) / INTERVAL_MIN ))

log "SLO Sustained Observation starting"
log "  mode:          $([[ ${DRY_RUN} -eq 1 ]] && echo 'DRY-RUN (simulated)' || echo 'REAL')"
log "  base_url:      ${BASE_URL}"
log "  endpoint:      ${ENDPOINT}"
log "  duration_min:  ${DURATION_MIN}"
log "  interval_min:  ${INTERVAL_MIN}"
log "  output_dir:    ${OUTPUT_DIR}"
log "  token:         $(_redact_token "${BEARER_TOKEN}")"
log "  expected_samples: ${EXPECTED_SAMPLES}"

# Write run meta (token excluded)
cat > "${RUN_META}" <<EOF
{
  "start_time_iso": "${RUN_START_ISO}",
  "start_time_epoch": ${RUN_START_EPOCH},
  "mode": "$([[ ${DRY_RUN} -eq 1 ]] && echo 'dry-run' || echo 'real')",
  "base_url": "${BASE_URL}",
  "endpoint": "${ENDPOINT}",
  "duration_min": ${DURATION_MIN},
  "interval_min": ${INTERVAL_MIN},
  "expected_samples": ${EXPECTED_SAMPLES}
}
EOF

# ---------------------------------------------------------------------------
# Observation loop
# ---------------------------------------------------------------------------
SAMPLES_TAKEN=0
SAMPLES_OK=0
SAMPLES_FAIL=0
TOTAL_DURATION_MS=0

_poll_once() {
    local url="$1"
    local token="$2"
    local sample_idx="$3"

    local sample_time_iso
    sample_time_iso="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    local sample_time_epoch
    sample_time_epoch=$(date +%s)

    local status="simulated"
    local latency_ms="0"
    local body_snip=""
    local curl_rc="0"

    if [[ ${DRY_RUN} -eq 1 ]]; then
        # Simulate alternating success/failure to exercise summary logic
        if [[ $((sample_idx % 7)) -eq 0 ]]; then
            status="simulated_fail"
            latency_ms="999"
        else
            status="simulated_ok"
            latency_ms="42"
        fi
        body_snip="dry-run body"
        curl_rc="0"
    else
        local tmp_body
        tmp_body="${OUTPUT_DIR}/.tmp_body.$$.${sample_idx}"
        local start_ms end_ms
        start_ms=$(date +%s%3N)

        if [[ -n "${token}" ]]; then
            curl_rc=$(curl -s -o "${tmp_body}" -w "%{http_code}" \
                -H "Authorization: Bearer ${token}" \
                --max-time "${CURL_MAX_TIME}" \
                "${url}" 2>/dev/null || echo "000")
        else
            curl_rc=$(curl -s -o "${tmp_body}" -w "%{http_code}" \
                --max-time "${CURL_MAX_TIME}" \
                "${url}" 2>/dev/null || echo "000")
        fi

        end_ms=$(date +%s%3N)
        latency_ms=$((end_ms - start_ms))
        status="${curl_rc}"

        # Capture first line of body for non-2xx responses (sanitized)
        if [[ -f "${tmp_body}" ]]; then
            body_snip=$(head -n1 "${tmp_body}" | tr -d '\r\n' | cut -c1-200 || true)
            rm -f "${tmp_body}"
        fi
    fi

    printf '%s\n' "{\"sample_index\":${sample_idx},\"timestamp_iso\":\"${sample_time_iso}\",\"timestamp_epoch\":${sample_time_epoch},\"status\":\"${status}\",\"latency_ms\":${latency_ms},\"body_snip\":$(_json_escape "${body_snip}"),\"curl_rc\":\"${curl_rc}\"}" >> "${OBSERVATION_LOG}"

    echo "${status}:${latency_ms}"
}

log "=== Starting observation loop ==="

ELAPSED_MIN=0
while [[ ${ELAPSED_MIN} -lt ${DURATION_MIN} ]]; do
    SAMPLES_TAKEN=$((SAMPLES_TAKEN + 1))

    result=$(_poll_once "${BASE_URL}${ENDPOINT}" "${BEARER_TOKEN}" "${SAMPLES_TAKEN}")
    status_val="${result%%:*}"
    latency_val="${result##*:}"

    TOTAL_DURATION_MS=$((TOTAL_DURATION_MS + latency_val))

    if [[ ${DRY_RUN} -eq 1 ]]; then
        if [[ "${status_val}" == "simulated_ok" ]]; then
            SAMPLES_OK=$((SAMPLES_OK + 1))
        else
            SAMPLES_FAIL=$((SAMPLES_FAIL + 1))
        fi
    else
        if [[ "${status_val}" =~ ^2 ]]; then
            SAMPLES_OK=$((SAMPLES_OK + 1))
        else
            SAMPLES_FAIL=$((SAMPLES_FAIL + 1))
        fi
    fi

    ELAPSED_MIN=$((ELAPSED_MIN + INTERVAL_MIN))
    if [[ ${ELAPSED_MIN} -lt ${DURATION_MIN} ]]; then
        log "Sample ${SAMPLES_TAKEN} complete. Sleeping ${INTERVAL_MIN} min until next sample..."
        sleep $((INTERVAL_MIN * 60))
    fi
done

RUN_END_ISO="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
RUN_END_EPOCH=$(date +%s)
RUN_DURATION_SEC=$((RUN_END_EPOCH - RUN_START_EPOCH))

# ---------------------------------------------------------------------------
# Compute summary stats
# ---------------------------------------------------------------------------
AVAILABILITY_PCT="0.00"
if [[ ${SAMPLES_TAKEN} -gt 0 ]]; then
    AVAILABILITY_PCT=$(awk "BEGIN {printf \"%.2f\", (${SAMPLES_OK} / ${SAMPLES_TAKEN}) * 100}")
fi

AVG_LATENCY_MS="0"
if [[ ${SAMPLES_TAKEN} -gt 0 ]]; then
    AVG_LATENCY_MS=$(awk "BEGIN {printf \"%.0f\", ${TOTAL_DURATION_MS} / ${SAMPLES_TAKEN}}")
fi

# ---------------------------------------------------------------------------
# Write markdown summary
# ---------------------------------------------------------------------------
cat > "${SUMMARY_MD}" <<EOF
# SLO Sustained Observation Summary

> **Status**: $([[ ${DRY_RUN} -eq 1 ]] && echo 'DRY-RUN / REHEARSAL — NOT VALID SLO EVIDENCE' || echo 'OBSERVATION — NOT AN SLO WINDOW CLOSURE CLAIM')
> **Mode**: $([[ ${DRY_RUN} -eq 1 ]] && echo 'dry-run (simulated)' || echo 'real HTTP polling')
> **Start**: ${RUN_START_ISO}
> **End**: ${RUN_END_ISO}
> **Duration (scheduled)**: ${DURATION_MIN} min
> **Actual duration (wall)**: ${RUN_DURATION_SEC} s
> **Endpoint**: ${BASE_URL}${ENDPOINT}

## Non-Claims

- This artifact does **not** claim production readiness.
- This artifact does **not** close an SLO window.
- Real sustained-window SLO evidence requires the approved duration on an approved target
  with operator signoff. See \`docs/PRODUCTION_NOTES.md\`.
- Dry-run outputs are simulation only and must not be used as SLO evidence.

## Observation Results

| Metric | Value |
|--------|-------|
| Samples taken | ${SAMPLES_TAKEN} |
| Samples OK | ${SAMPLES_OK} |
| Samples fail | ${SAMPLES_FAIL} |
| Availability (%) | ${AVAILABILITY_PCT} |
| Average latency (ms) | ${AVG_LATENCY_MS} |

## Files

| File | Description |
|------|-------------|
| \`run_meta.json\` | Run parameters and timing |
| \`observations.jsonl\` | Per-sample JSON lines (status, latency, timestamp) |
| \`observation_summary.md\` | This summary |

## Next Steps

- For a **short local rehearsal**: review the output above; if it looks reasonable,
  you may proceed to a real observation against a target host.
- For a **real sustained window**: re-run with \`--dry-run\` removed and
  \`--duration-min\` set to the approved window length (e.g., 10080 for 7 days).
  Store the resulting evidence in version control and seek operator signoff.

## Operator Signoff (blank until signed)

| Field | Value |
|-------|-------|
| Operator initials | ___________ |
| Date | ___________ |
| Approved for SLO window closure | YES / NO |
| Notes | |

---

*Generated by run_slo_sustained_observation.sh*
EOF

log "Observation loop finished"
log "  samples:     ${SAMPLES_TAKEN}"
log "  ok:          ${SAMPLES_OK}"
log "  fail:        ${SAMPLES_FAIL}"
log "  availability: ${AVAILABILITY_PCT}%"
log "  avg_latency: ${AVG_LATENCY_MS} ms"
log "  output_dir:  ${OUTPUT_DIR}"
log "  summary:     ${SUMMARY_MD}"

# Update run_meta with end time
cat > "${RUN_META}" <<EOF
{
  "start_time_iso": "${RUN_START_ISO}",
  "start_time_epoch": ${RUN_START_EPOCH},
  "end_time_iso": "${RUN_END_ISO}",
  "end_time_epoch": ${RUN_END_EPOCH},
  "wall_duration_sec": ${RUN_DURATION_SEC},
  "mode": "$([[ ${DRY_RUN} -eq 1 ]] && echo 'dry-run' || echo 'real')",
  "base_url": "${BASE_URL}",
  "endpoint": "${ENDPOINT}",
  "duration_min": ${DURATION_MIN},
  "interval_min": ${INTERVAL_MIN},
  "expected_samples": ${EXPECTED_SAMPLES},
  "actual_samples": ${SAMPLES_TAKEN},
  "samples_ok": ${SAMPLES_OK},
  "samples_fail": ${SAMPLES_FAIL},
  "availability_pct": "${AVAILABILITY_PCT}",
  "avg_latency_ms": "${AVG_LATENCY_MS}"
}
EOF

log "SLO Sustained Observation complete"

# ---------------------------------------------------------------------------
# Optional JSONL validation (dry-run only)
# ---------------------------------------------------------------------------
if [[ ${DRY_RUN} -eq 1 && -f "${OBSERVATION_LOG}" ]]; then
    python3 -c 'import json,sys; [json.loads(l) for l in open(sys.argv[1])]; print("JSONL validation passed")' "${OBSERVATION_LOG}"
    log "JSONL validation passed"
fi

exit 0
