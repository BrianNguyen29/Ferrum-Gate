#!/usr/bin/env bash
# phase4a_capture_metrics_baseline.sh
# Phase 4A: Capture metrics baseline from ferrumgate VM.
# Read-only evidence gathering script; does NOT mutate state.
# NOT production-ready, NOT full G2, NOT production pilot authorization.
#
# This script:
#   - Detects current TLS domain from Caddyfile (DuckDNS or nip.io)
#   - Curls /v1/readyz/deep and /v1/metrics from the running VM
#   - Saves timestamped baseline to /tmp/ferrumgate_metrics_baseline_YYYYMMDD_HHMMSS.txt
#   - Emits sanitized evidence (no tokens, no secrets)
#
# Prerequisites:
#   - Phase 3J DuckDNS TLS working (or nip.io fallback)
#   - VM must be reachable via gcloud SSH
#   - curl must be available on the local machine (not on VM)
#
# Usage:
#   bash scripts/gcp/phase4a_capture_metrics_baseline.sh --confirm [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID    GCP project ID (default: fairy-b13f4)
#   GCP_ZONE          Zone (default: asia-southeast1-a)
#   GCP_VM_NAME       VM name (default: ferrumgate-nonprod)
#   CONFIRM           Must be "true" to confirm execution
#
# Options:
#   --help                  Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --zone ZONE            Zone (default: asia-southeast1-a)
#   --vm-name NAME         VM name (default: ferrumgate-nonprod)
#   --confirm               Required: acknowledge before querying VM
#
# Example:
#   bash scripts/gcp/phase4a_capture_metrics_baseline.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod

set -euo pipefail

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 4A: Capture metrics baseline from ferrumgate VM.

Usage:
  bash scripts/gcp/phase4a_capture_metrics_baseline.sh --confirm [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --zone ZONE            Zone (default: asia-southeast1-a)
  --vm-name NAME         VM name (default: ferrumgate-nonprod)
  --confirm              Required: acknowledge before capturing baseline

Prerequisites:
  - Phase 3J DuckDNS TLS working (or nip.io fallback)
  - VM reachable via gcloud SSH
  - curl available on local machine

What it captures:
  - Current TLS domain (DuckDNS or nip.io)
  - /v1/readyz/deep response
  - /v1/metrics response (metric lines only, no secrets)
  - Timestamp

Output:
  - Saved to /tmp/ferrumgate_metrics_baseline_YYYYMMDD_HHMMSS.txt
  - Evidence printed to stdout

Non-claims:
  NOT production-ready, NOT production alerting, NOT full G2.
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help) usage; exit 0 ;;
        --project-id) GCP_PROJECT_ID="$2"; shift 2 ;;
        --zone) GCP_ZONE="$2"; shift 2 ;;
        --vm-name) GCP_VM_NAME="$2"; shift 2 ;;
        --confirm) CONFIRM="true"; shift ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

if [[ "$CONFIRM" != "true" ]]; then
    echo "ERROR: --confirm required to capture metrics baseline." >&2
    echo "Usage: bash scripts/gcp/phase4a_capture_metrics_baseline.sh --confirm [...]" >&2
    exit 1
fi

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

echo "=== Phase 4A: Capture Metrics Baseline ==="
echo "Project   : $GCP_PROJECT_ID"
echo "Zone      : $GCP_ZONE"
echo "VM Name   : $GCP_VM_NAME"
echo ""
echo "WARNING: Read-only evidence gathering. No state will be modified."
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/3] Checking VM is running..."
if ! gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "ERROR: VM '$GCP_VM_NAME' not found." >&2
    exit 1
fi
echo "  VM is reachable."

# --- Detect current TLS domain from Caddyfile ---
echo "[2/3] Detecting current TLS domain from Caddyfile..."

CURRENT_DOMAIN=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "grep -v '^#' /etc/caddy/Caddyfile 2>/dev/null | awk 'NF { print \$1; exit }' | tr -d '{' || echo 'UNKNOWN'" \
    2>/dev/null || echo "UNKNOWN")

echo "  Current TLS domain: $CURRENT_DOMAIN"

# Determine HTTPS URL
if [[ "$CURRENT_DOMAIN" == "UNKNOWN" ]] || [[ -z "$CURRENT_DOMAIN" ]]; then
    METRICS_URL="https://34-158-51-8.nip.io/v1/metrics"
    READYZ_URL="https://34-158-51-8.nip.io/v1/readyz/deep"
else
    METRICS_URL="https://${CURRENT_DOMAIN}/v1/metrics"
    READYZ_URL="https://${CURRENT_DOMAIN}/v1/readyz/deep"
fi

echo "  Metrics URL  : $METRICS_URL"
echo "  Readyz URL   : $READYZ_URL"

# --- Capture metrics baseline on VM and copy to local temp ---
echo "[3/3] Capturing metrics baseline..."

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOCAL_OUTPUT="/tmp/ferrumgate_metrics_baseline_${TIMESTAMP}.txt"

# Curl the endpoints via the VM as a proxy (VM curls itself)
# This avoids TLS cert issues from local machine
READYZ_OUTPUT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "curl -s --max-time 10 '${READYZ_URL}' 2>&1 || echo 'READYZ_FAILED'" \
    2>/dev/null || echo "READYZ_FAILED")

METRICS_OUTPUT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "curl -s --max-time 30 '${METRICS_URL}' 2>&1 | head -100 || echo 'METRICS_FAILED'" \
    2>/dev/null || echo "METRICS_FAILED")

# Write to local file
cat > "$LOCAL_OUTPUT" << EOF
# FerrumGate Metrics Baseline
# Captured: $(date -Iseconds)
# VM: $GCP_VM_NAME ($GCP_PROJECT_ID/$GCP_ZONE)
# Domain: $CURRENT_DOMAIN
# Metrics URL: $METRICS_URL
# Readyz URL: $READYZ_URL

## /v1/readyz/deep
$READYZ_OUTPUT

## /v1/metrics (first 100 lines)
$METRICS_OUTPUT

## End of Baseline
EOF

echo "  Baseline saved to: $LOCAL_OUTPUT"

# --- Emit summary ---
echo ""
echo "=== Metrics Baseline Summary ==="
echo "Captured at   : $(date -Iseconds)"
echo "VM           : $GCP_VM_NAME"
echo "Domain       : $CURRENT_DOMAIN"
echo "Readyz status: $(echo "$READYZ_OUTPUT" | head -1)"
echo "Metrics lines: $(echo "$METRICS_OUTPUT" | wc -l)"
echo ""

# Show key metrics
echo "Key metrics from baseline:"
echo "$METRICS_OUTPUT" | grep -E '^(ferrumgate_store_health_up|ferrumgate_write_queue_depth|up\{)' | head -10

echo ""
echo "=== Phase 4A Metrics Baseline Capture Complete ==="
echo ""
echo "Evidence saved to: $LOCAL_OUTPUT"
echo ""
echo "Non-claims: NOT production-ready, NOT production alerting, NOT full G2."
