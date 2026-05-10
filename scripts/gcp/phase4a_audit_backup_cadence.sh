#!/usr/bin/env bash
# phase4a_audit_backup_cadence.sh
# Phase 4A: Audit backup cadence for ferrumgate VM.
# Read-only evidence gathering script; does NOT mutate state.
# NOT production-ready, NOT full G2, NOT production pilot authorization.
#
# This script:
#   - Queries backup timer schedule via systemd on the VM
#   - Lists recent backup objects in GCS bucket
#   - Computes age of most recent backup
#   - Compares against RPO threshold (default: 1 hour)
#   - Emits sanitized evidence (no tokens, no secrets)
#
# Prerequisites:
#   - Phase 3H GCS offsite backup must be deployed
#   - VM must be reachable via gcloud SSH
#   - GCS bucket must be accessible via attached SA or key
#
# Usage:
#   bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID    GCP project ID (default: fairy-b13f4)
#   GCP_ZONE          Zone (default: asia-southeast1-a)
#   GCP_VM_NAME       VM name (default: ferrumgate-nonprod)
#   GCS_BUCKET        GCS bucket URL (REQUIRED)
#   RPO_THRESHOLD_SEC RPO threshold in seconds (default: 3600)
#   CONFIRM           Must be "true" to confirm execution
#
# Options:
#   --help                  Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --zone ZONE            Zone (default: asia-southeast1-a)
#   --vm-name NAME         VM name (default: ferrumgate-nonprod)
#   --gcs-bucket BUCKET    GCS bucket URL (REQUIRED)
#   --rpo-threshold SEC    RPO threshold in seconds (default: 3600)
#   --confirm               Required: acknowledge before querying VM
#
# Example:
#   bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/

set -euo pipefail

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
GCS_BUCKET="${GCS_BUCKET:-}"
RPO_THRESHOLD_SEC="${RPO_THRESHOLD_SEC:-3600}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 4A: Audit backup cadence for ferrumgate VM.

Usage:
  bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm --gcs-bucket BUCKET [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --zone ZONE            Zone (default: asia-southeast1-a)
  --vm-name NAME         VM name (default: ferrumgate-nonprod)
  --gcs-bucket BUCKET   GCS bucket URL (REQUIRED)
  --rpo-threshold SEC   RPO threshold in seconds (default: 3600)
  --confirm              Required: acknowledge before querying VM

Prerequisites:
  - Phase 3H GCS offsite backup deployed
  - VM reachable via gcloud SSH
  - GCS bucket accessible

Evidence emitted (sanitized — no tokens, no secrets):
  - Backup timer schedule and last trigger time
  - Last 5 GCS backup objects with age
  - RPO compliance status

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
        --gcs-bucket) GCS_BUCKET="$2"; shift 2 ;;
        --rpo-threshold) RPO_THRESHOLD_SEC="$2"; shift 2 ;;
        --confirm) CONFIRM="true"; shift ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# --- Validate required inputs ---
if [[ -z "$GCS_BUCKET" ]]; then
    echo "ERROR: --gcs-bucket is required." >&2
    usage; exit 1
fi

if [[ "$CONFIRM" != "true" ]]; then
    echo "ERROR: --confirm required to run backup cadence audit." >&2
    echo "Usage: bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm --gcs-bucket BUCKET [...]" >&2
    exit 1
fi

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

echo "=== Phase 4A: Backup Cadence Audit ==="
echo "Project       : $GCP_PROJECT_ID"
echo "Zone          : $GCP_ZONE"
echo "VM Name       : $GCP_VM_NAME"
echo "GCS Bucket    : $GCS_BUCKET"
echo "RPO Threshold : ${RPO_THRESHOLD_SEC}s"
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

# --- Query backup timer on VM ---
echo "[2/3] Querying backup timer schedule on VM..."

TIMER_OUTPUT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "systemctl list-timers --all --no-pager 2>/dev/null | grep ferrumgate-offsite-backup.timer || echo 'TIMER_NOT_FOUND'" \
    2>/dev/null || echo "TIMER_NOT_FOUND")

LAST_OUTPUT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "systemctl show ferrumgate-offsite-backup.timer --property=LastTriggerZone,LastTriggerTime --value 2>/dev/null || echo 'TIMER_NOT_FOUND'" \
    2>/dev/null || echo "TIMER_NOT_FOUND")

echo "  Timer status:"
echo "  $TIMER_OUTPUT"
echo "  Last trigger: $LAST_OUTPUT"

# --- List recent GCS backup objects ---
echo "[3/3] Listing recent GCS backup objects..."

# Use gsutil to list recent objects (last 5)
GCS_OUTPUT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "gsutil ls -l '${GCS_BUCKET}' 2>/dev/null | tail -6 || echo 'GCS_ACCESS_DENIED_OR_EMPTY'" \
    2>/dev/null || echo "GCS_ACCESS_DENIED_OR_EMPTY")

echo "  Recent backup objects (up to 5):"
echo "$GCS_OUTPUT"

# --- Compute age of most recent backup ---
# gsutil ls -l output: SIZE  DATE  gs://bucket/path (URL not at line start)
# Parse the line containing gs:// to extract GCS mtime (field 2) and URL (field 3)
OBJECT_LINE=$(echo "$GCS_OUTPUT" | grep -m1 -E 'gs://[^[:space:]]+' || echo "")
if [[ -n "$OBJECT_LINE" ]]; then
    # Extract ISO timestamp (field 2) and URL (field 3) from the listing line
    GCS_TIMESTAMP=$(echo "$OBJECT_LINE" | awk '{print $2}' | sed 's/Z$//')
    MOST_RECENT=$(echo "$OBJECT_LINE" | awk '{print $3}')
    if [[ -n "$GCS_TIMESTAMP" && -n "$MOST_RECENT" ]]; then
        BACKUP_EPOCH=$(date -d "$GCS_TIMESTAMP" +%s 2>/dev/null || echo "0")
        NOW_EPOCH=$(date +%s)
        AGE_SEC=$((NOW_EPOCH - BACKUP_EPOCH))
        AGE_HUMAN=""
        if command -v numfmt &>/dev/null; then
            AGE_HUMAN=$(numfmt --to=iec "$AGE_SEC" 2>/dev/null || echo "${AGE_SEC}s")
        else
            AGE_HUMAN="${AGE_SEC}s"
        fi

        echo ""
        echo "  Most recent backup: $MOST_RECENT"
        echo "  GCS mtime         : ${GCS_TIMESTAMP}Z"
        echo "  Age              : $AGE_HUMAN"
        echo "  RPO threshold    : ${RPO_THRESHOLD_SEC}s ($(echo "scale=1; $RPO_THRESHOLD_SEC/3600" | bc 2>/dev/null || echo "${RPO_THRESHOLD_SEC}s")h)"

        if [[ "$AGE_SEC" -le "$RPO_THRESHOLD_SEC" ]]; then
            echo "  RPO compliance   : ✅ WITHIN RPO (${AGE_SEC}s <= ${RPO_THRESHOLD_SEC}s)"
        else
            echo "  RPO compliance   : ⚠️  EXCEEDS RPO (${AGE_SEC}s > ${RPO_THRESHOLD_SEC}s)"
            echo "                    Backup is older than RPO threshold. Verify backup schedule is running."
        fi
    else
        echo "  Could not parse timestamp from GCS listing."
    fi
else
    echo "  No GCS backup objects found or access denied."
fi

echo ""
echo "=== Phase 4A Backup Cadence Audit Complete ==="
echo ""
echo "Evidence summary:"
echo "  - Timer found: $(echo "$TIMER_OUTPUT" | grep -q 'ferrumgate-offsite-backup' && echo 'YES' || echo 'NO')"
echo "  - GCS objects: $(echo "$GCS_OUTPUT" | grep -cE 'gs://[^[:space:]]+' 2>/dev/null || echo '0')"
echo "  - Script: read-only (no state modified)"
echo ""
echo "Non-claims: NOT production-ready, NOT production alerting, NOT full G2."
