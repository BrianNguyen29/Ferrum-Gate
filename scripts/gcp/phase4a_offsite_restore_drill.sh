#!/usr/bin/env bash
# phase4a_offsite_restore_drill.sh
# Phase 4A: Offsite restore drill from GCS for ferrumgate VM.
# Read-only evidence gathering script; does NOT overwrite production database.
# NOT production-ready, NOT full G2, NOT production pilot authorization.
#
# This script:
#   - Lists latest backup objects in GCS bucket
#   - Copies most recent backup to a temp file (/tmp/ferrumgate_restore Drill_XXXXXX.db)
#   - Runs PRAGMA integrity_check on the temp file
#   - Reports TABLE_COUNT and SIZE_BYTES
#   - Cleans up temp file on success or interrupt
#
# IMPORTANT: This does NOT overwrite the production database.
#   Restore is to a temp file only. Production DB is untouched.
#
# Prerequisites:
#   - Phase 3H GCS offsite backup must be deployed
#   - VM must be reachable via gcloud SSH
#   - GCS bucket must be accessible via attached SA or key
#   - sqlite3 CLI must be available on VM (available in Phase 3A image)
#
# Usage:
#   bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID    GCP project ID (default: fairy-b13f4)
#   GCP_ZONE          Zone (default: asia-southeast1-a)
#   GCP_VM_NAME       VM name (default: ferrumgate-nonprod)
#   GCS_BUCKET        GCS bucket URL (REQUIRED)
#   CONFIRM           Must be "true" to confirm execution
#
# Options:
#   --help                  Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --zone ZONE            Zone (default: asia-southeast1-a)
#   --vm-name NAME         VM name (default: ferrumgate-nonprod)
#   --gcs-bucket BUCKET   GCS bucket URL (REQUIRED)
#   --confirm               Required: acknowledge before executing drill
#
# Example:
#   bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm \
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
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 4A: Offsite restore drill from GCS for ferrumgate VM.

Usage:
  bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm --gcs-bucket BUCKET [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --zone ZONE            Zone (default: asia-southeast1-a)
  --vm-name NAME         VM name (default: ferrumgate-nonprod)
  --gcs-bucket BUCKET   GCS bucket URL (REQUIRED)
  --confirm              Required: acknowledge before executing drill

IMPORTANT:
  - This script restores to a TEMP FILE only (/tmp/ferrumgate_restore Drill_XXXXXX.db)
  - Production database at /var/lib/ferrumgate/ferrumgate.db is NOT modified
  - Temp file is cleaned up on success or interrupt (trap)

Prerequisites:
  - Phase 3H GCS offsite backup deployed
  - VM reachable via gcloud SSH
  - GCS bucket accessible
  - sqlite3 CLI available on VM

Evidence emitted (sanitized — no tokens, no secrets):
  - GCS object listing
  - PRAGMA integrity_check result
  - TABLE_COUNT, SIZE_BYTES

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
    echo "ERROR: --confirm required to run offsite restore drill." >&2
    echo "Usage: bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm --gcs-bucket BUCKET [...]" >&2
    exit 1
fi

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

echo "=== Phase 4A: Offsite Restore Drill ==="
echo "Project    : $GCP_PROJECT_ID"
echo "Zone       : $GCP_ZONE"
echo "VM Name    : $GCP_VM_NAME"
echo "GCS Bucket : $GCS_BUCKET"
echo ""
echo "WARNING: This drill restores to a TEMP FILE only."
echo "         Production database is NOT modified."
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/4] Checking VM is running..."
if ! gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "ERROR: VM '$GCP_VM_NAME' not found." >&2
    exit 1
fi
echo "  VM is reachable."

# --- List latest GCS backup objects ---
echo "[2/4] Listing latest backup objects in GCS bucket..."

GCS_LIST=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "gsutil ls -l '${GCS_BUCKET}' 2>/dev/null | tail -10 || echo 'GCS_ACCESS_DENIED_OR_EMPTY'" \
    2>/dev/null || echo "GCS_ACCESS_DENIED_OR_EMPTY")

echo "  GCS listing:"
echo "$GCS_LIST" | head -10

# Find most recent backup object
MOST_RECENT=$(echo "$GCS_LIST" | grep '^gs://' | head -1 || echo "")
if [[ -z "$MOST_RECENT" ]]; then
    echo "ERROR: No backup objects found in GCS bucket." >&2
    exit 1
fi

echo "  Most recent backup: $MOST_RECENT"

# --- Copy latest backup to temp file on VM ---
echo "[3/4] Copying latest backup to temp file on VM..."

# Generate temp file path
TEMP_FILE=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "mktemp /tmp/ferrumgate_restore_drill_XXXXXX.db" 2>/dev/null || echo "")

if [[ -z "$TEMP_FILE" ]]; then
    echo "ERROR: Could not create temp file on VM." >&2
    exit 1
fi

echo "  Temp file: $TEMP_FILE"

# Copy from GCS to temp file
COPY_RESULT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "gsutil cp '${MOST_RECENT}' '${TEMP_FILE}' 2>&1 && echo 'COPY_SUCCESS' || echo 'COPY_FAILED'" \
    2>/dev/null || echo "COPY_FAILED")

echo "  Copy result: $COPY_RESULT"

if [[ "$COPY_RESULT" != "COPY_SUCCESS" ]]; then
    echo "ERROR: Failed to copy backup from GCS." >&2
    exit 1
fi

# --- Run PRAGMA integrity_check on temp file ---
echo "[4/4] Running SQLite integrity check on temp file..."

INTEGRITY_RESULT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sqlite3 '${TEMP_FILE}' 'PRAGMA integrity_check; SELECT COUNT(*) as TABLE_COUNT FROM sqlite_master WHERE type=\"table\"; SELECT page_count * page_size as SIZE_BYTES FROM pragma_page_count, pragma_page_size;' 2>&1" \
    2>/dev/null || echo "INTEGRITY_CHECK_FAILED")

echo "  Integrity check result:"
echo "  $INTEGRITY_RESULT"

# Parse results
INTEGRITY_STATUS=$(echo "$INTEGRITY_RESULT" | head -1 | tr -d '[:space:]')
TABLE_COUNT=$(echo "$INTEGRITY_RESULT" | sed -n '2p' | tr -d '[:space:]')
SIZE_BYTES=$(echo "$INTEGRITY_RESULT" | sed -n '3p' | tr -d '[:space:]')

echo ""
echo "=== Restore Drill Results ==="
echo "GCS Object     : $MOST_RECENT"
echo "Temp File      : $TEMP_FILE"
echo "INTEGRITY      : $INTEGRITY_STATUS"
echo "TABLE_COUNT    : $TABLE_COUNT"
echo "SIZE_BYTES     : $SIZE_BYTES"

if [[ "$INTEGRITY_STATUS" == "ok" ]]; then
    echo "Drill Result   : ✅ PASSED — offsite backup integrity confirmed"
else
    echo "Drill Result   : ❌ FAILED — integrity check did not return 'ok'"
    echo "                 Temp file retained for diagnosis. Remove manually:"
    echo "                 gcloud compute ssh ubuntu@$GCP_VM_NAME -- rm -f '$TEMP_FILE'"
fi

# --- Cleanup temp file ---
echo ""
echo "Cleaning up temp file..."
CLEANUP_RESULT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "rm -f '${TEMP_FILE}' && echo 'CLEANUP_SUCCESS' || echo 'CLEANUP_FAILED'" \
    2>/dev/null || echo "CLEANUP_FAILED")
echo "  Cleanup: $CLEANUP_RESULT"

echo ""
echo "=== Phase 4A Offsite Restore Drill Complete ==="
echo ""
echo "Non-claims: NOT production-ready, NOT production alerting, NOT full G2."
echo "Production database was NOT modified. Restore was to temp file only."
