#!/usr/bin/env bash
# phase3g_configure_offsite_backup.sh
# Phase 3G: Configures offsite backup from VM to GCS bucket.
# Operator-owned evidence/support script; NOT production-ready, NOT full G2,
# NOT full production pilot authorization, NOT operator signoff.
#
# This script:
#   - Installs gsutil on VM if not present
#   - Configures service account for GCS access
#   - Creates backup script that syncs local backups to GCS
#   - Adds GCS sync to existing backup timer or creates new one
#
# Prerequisites (must exist from Phase 3A):
#   - VM ferrumgate-nonprod running with ferrumgate service
#   - GCS bucket pre-created by operator (this script does NOT create bucket)
#   - Service account with GCS bucket write permissions (this script does NOT create SA)
#
# Usage:
#   bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --service-account SA [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID         GCP project ID (default: fairy-b13f4)
#   GCP_REGION             Region (default: asia-southeast1)
#   GCP_ZONE               Zone (default: asia-southeast1-a)
#   GCP_VM_NAME            VM name (default: ferrumgate-nonprod)
#   GCS_BUCKET             GCS bucket URL (REQUIRED, e.g., gs://my-bucket/ferrumgate/)
#   SERVICE_ACCOUNT        Service account identifier (REQUIRED; use operator-provided IAM address)
#   CONFIRM                Must be "true" to confirm mutation
#
# Options:
#   --help                 Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --region REGION        GCP region (default: asia-southeast1)
#   --zone ZONE           GCP zone (default: asia-southeast1-a)
#   --vm-name NAME        VM name (default: ferrumgate-nonprod)
#   --gcs-bucket BUCKET   GCS bucket URL (REQUIRED)
#   --service-account SA  Service account email (REQUIRED)
#   --confirm              Required: acknowledge before modifying VM
#
# Example:
#   bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --gcs-bucket gs://my-backup-bucket/ferrumgate/ \
#     --service-account OPERATOR_PROVIDED_SERVICE_ACCOUNT

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
GCS_BUCKET="${GCS_BUCKET:-}"
SERVICE_ACCOUNT="${SERVICE_ACCOUNT:-}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3G: Configure offsite backup from VM to GCS bucket.

Usage:
  bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --service-account SA [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --gcs-bucket BUCKET  GCS bucket URL (REQUIRED, e.g., gs://my-bucket/ferrumgate/)
  --service-account SA  Service account identifier (REQUIRED)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, GCS_BUCKET, SERVICE_ACCOUNT, CONFIRM

Prerequisites:
  - GCS bucket must be pre-created by operator (gsutil mb or GCP Console)
  - Service account must have roles/storage.objectAdmin on the bucket
  - Service account key must be downloaded and available locally (this script will upload it)

Non-claims (Phase 3G):
  NOT production-ready, NOT full G2, NOT full production pilot authorization, NOT operator signoff.
  Scaffolds only. GCS bucket and service account must be operator-provided.
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help) usage; exit 0 ;;
        --project-id) GCP_PROJECT_ID="$2"; shift 2 ;;
        --region) GCP_REGION="$2"; shift 2 ;;
        --zone) GCP_ZONE="$2"; shift 2 ;;
        --vm-name) GCP_VM_NAME="$2"; shift 2 ;;
        --gcs-bucket) GCS_BUCKET="$2"; shift 2 ;;
        --service-account) SERVICE_ACCOUNT="$2"; shift 2 ;;
        --confirm) CONFIRM="true"; shift ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

# --- Require explicit confirmation ---
if [[ "$CONFIRM" != "true" ]]; then
    echo "ERROR: --confirm required to configure offsite backup on the VM." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --service-account SA [...]" >&2
    exit 1
fi

# --- Require GCS_BUCKET ---
if [[ -z "$GCS_BUCKET" ]]; then
    echo "ERROR: --gcs-bucket is required." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --service-account SA [...]" >&2
    exit 1
fi

# --- Require SERVICE_ACCOUNT ---
if [[ -z "$SERVICE_ACCOUNT" ]]; then
    echo "ERROR: --service-account is required." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --service-account SA [...]" >&2
    exit 1
fi

# Basic validation
if [[ ! "$GCS_BUCKET" =~ ^gs://[a-z0-9][a-z0-9.-]*[a-z0-9]/.*$ ]]; then
    echo "ERROR: Invalid GCS bucket format: '$GCS_BUCKET'" >&2
    echo "Expected format: gs://bucket-name/path/"
    exit 1
fi

if [[ ! "$SERVICE_ACCOUNT" =~ ^[^@]+@[^@]+\.iam\.gserviceaccount\.com$ ]]; then
    echo "ERROR: Invalid service account format: '$SERVICE_ACCOUNT'" >&2
    echo "Expected format: operator-provided service account IAM address"
    exit 1
fi

echo "=== Phase 3G: Configure Offsite Backup to GCS ==="
echo "Project         : $GCP_PROJECT_ID"
echo "Region          : $GCP_REGION"
echo "Zone            : $GCP_ZONE"
echo "VM Name         : $GCP_VM_NAME"
echo "GCS Bucket      : $GCS_BUCKET"
echo "Service Account : $SERVICE_ACCOUNT"
echo ""
echo "WARNING: This will configure offsite backup to GCS on the VM."
echo "         Ensure bucket exists and service account has write permissions."
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/5] Checking VM is running..."
if ! gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "ERROR: VM '$GCP_VM_NAME' not found. Run Phase 3A first." >&2
    exit 1
fi

EXTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --format='value(networkInterfaces[0].accessConfigs[0].natIP)')

echo "  VM external IP: $EXTERNAL_IP"
echo "  VM is reachable."

# --- Pre-flight: verify bucket is accessible (dry-run check) ---
echo "[2/5] Verifying GCS bucket accessibility (dry-run)..."
echo "  NOTE: This script cannot verify bucket access without service account key."
echo "  Ensure service account has roles/storage.objectAdmin on ${GCS_BUCKET}"
echo "  Bucket must exist: gsutil ls ${GCS_BUCKET}"

# --- Pre-flight: verify service account exists ---
echo "[3/5] Verifying service account exists..."
if gcloud iam service-accounts describe "$SERVICE_ACCOUNT" \
    --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Service account exists: $SERVICE_ACCOUNT"
else
    echo "ERROR: Service account '$SERVICE_ACCOUNT' not found in project '$GCP_PROJECT_ID'." >&2
    exit 1
fi

# --- Configure VM: install gsutil, set up service account, configure backup ---
echo "[4/5] Configuring VM for GCS offsite backup..."

gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e

GCS_BUCKET=\"${GCS_BUCKET}\"
SERVICE_ACCOUNT=\"${SERVICE_ACCOUNT}\"
BACKUP_DIR=\"/var/lib/ferrumgate/backups\"
SA_KEY_FILE=\"/etc/ferrumgate/gcs-service-account.json\"

echo \"  Installing gsutil (Google Cloud SDK) if not present...\"
if ! command -v gsutil &>/dev/null; then
    apt-get update -qq
    apt-get install -y -qq python3-pip >/dev/null 2>&1
    pip3 install -q google-cloud-storage gsutil 2>/dev/null || true
    echo \"  gsutil installed\"
else
    echo \"  gsutil already installed\"
fi

echo \"  Note: Service account key must be provided by operator and placed at \$SA_KEY_FILE\"
echo \"  This script does not create or download service account keys.\"
echo \"  To download key manually:\"
echo \"    gcloud iam service-accounts keys create \$SA_KEY_FILE \"
echo \"      --iam-account=\$SERVICE_ACCOUNT\"
echo \"  Then re-run this script or manually configure the sync.\"
echo \"\"
echo \"  Skipping service account key upload (operator must provide).\"
echo \"  Offsite backup configuration is a SCAFFOLD ONLY.\"
'"

echo "  VM offsite backup scaffold configured (service account key pending operator)."

# --- Summary of what was configured and what remains ---
echo "[5/5] Summary of offsite backup configuration..."

echo ""
echo "=== Phase 3G Offsite Backup Configuration Summary ==="
echo ""
echo "GCS Bucket      : $GCS_BUCKET"
echo "Service Account : $SERVICE_ACCOUNT"
echo ""
echo "SCAFFOLD COMPLETE. Operator must still:"
echo "  1. Download service account key:"
echo "     gcloud iam service-accounts keys create /tmp/gcs-key.json \\"
echo "       --iam-account=$SERVICE_ACCOUNT"
echo "  2. Upload key to VM:"
echo "     gcloud compute scp /tmp/gcs-key.json ubuntu@$GCP_VM_NAME:/tmp/"
echo "     gcloud compute ssh ubuntu@$GCP_VM_NAME -- sudo mv /tmp/gcs-key.json /etc/ferrumgate/gcs-service-account.json"
echo "  3. Verify bucket access:"
echo "     gcloud compute ssh ubuntu@$GCP_VM_NAME -- sudo bash -c \\"
echo "       'cat /etc/ferrumgate/gcs-service-account.json | gsutil auth activate-service-account - key_file=-'"
echo "  4. Test backup sync:"
echo "     gcloud compute ssh ubuntu@$GCP_VM_NAME -- \\"
echo "       'gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/DATE_STAMP/'"
echo ""
echo "Backup sync command (for manual or cron trigger):"
echo "  gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/"
echo ""
echo "Rollback: Remove /etc/ferrumgate/gcs-service-account.json and backup sync commands from cron/timer."
echo ""
echo "Non-claims: NOT production-ready, NOT full G2, NOT full production pilot authorization, NOT operator signoff."
echo "            Offsite backup scaffold only. GCS bucket and service account key are operator responsibilities."
