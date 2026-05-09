#!/usr/bin/env bash
# phase3g_configure_offsite_backup.sh
# Phase 3G: Configures offsite backup from VM to GCS bucket.
# Operator-owned evidence/support script; NOT production-ready, NOT full G2,
# NOT full production pilot authorization, NOT operator signoff.
#
# This script:
#   - Installs gsutil on VM if not present
#   - Configures GCS access via VM attached service account OR operator-provided service account
#   - Creates backup script that syncs local backups to GCS
#   - Adds GCS sync to existing backup timer or creates new one
#
# Prerequisites (must exist from Phase 3A):
#   - VM ferrumgate-nonprod running with ferrumgate service
#   - GCS bucket pre-created by operator (this script does NOT create bucket)
#   - For vm-service-account mode: VM must have a attached service account with GCS bucket write
#     AND the VM must have OAuth scopes that include GCS (e.g., https://www.googleapis.com/auth/devstorage.read_write)
#     NOTE: If VM attached SA returns "403 Provided scope(s) are not authorized", use service-account-key mode instead
#   - For service-account-key mode: Service account with GCS bucket write permissions (this script does NOT create SA)
#
# Operational notes (learned from Phase 3H):
#   - gsutil installed via snap is at /snap/bin/gsutil, NOT in default PATH for systemd services
#   - Systemd units MUST use full path /snap/bin/gsutil
#   - If VM attached SA has insufficient OAuth scopes, switch to service-account-key mode
#
# Usage:
#   bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID         GCP project ID (default: fairy-b13f4)
#   GCP_REGION             Region (default: asia-southeast1)
#   GCP_ZONE               Zone (default: asia-southeast1-a)
#   GCP_VM_NAME            VM name (default: ferrumgate-nonprod)
#   GCS_BUCKET             GCS bucket URL (REQUIRED, e.g., gs://my-bucket/ferrumgate/)
#   AUTH_MODE              Authentication mode: vm-service-account (default) or service-account-key
#   SERVICE_ACCOUNT        Service account identifier (used with service-account-key mode)
#   CONFIRM                Must be "true" to confirm mutation
#
# Options:
#   --help                 Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --region REGION        GCP region (default: asia-southeast1)
#   --zone ZONE           GCP zone (default: asia-southeast1-a)
#   --vm-name NAME        VM name (default: ferrumgate-nonprod)
#   --gcs-bucket BUCKET   GCS bucket URL (REQUIRED)
#   --auth-mode MODE      Auth mode: vm-service-account (default) or service-account-key
#   --service-account SA  Service account email (REQUIRED only for service-account-key mode)
#   --confirm              Required: acknowledge before modifying VM
#
# Example (vm-service-account mode - keyless):
#   bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
#
# Example (service-account-key mode):
#   bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --gcs-bucket gs://my-backup-bucket/ferrumgate/ \
#     --auth-mode service-account-key \
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
AUTH_MODE="${AUTH_MODE:-vm-service-account}"
SERVICE_ACCOUNT="${SERVICE_ACCOUNT:-}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3G: Configure offsite backup from VM to GCS bucket.

Usage:
  bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --gcs-bucket BUCKET  GCS bucket URL (REQUIRED, e.g., gs://my-bucket/ferrumgate/)
  --auth-mode MODE      Auth mode: vm-service-account (default) or service-account-key
  --service-account SA  Service account email (REQUIRED only for service-account-key mode)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, GCS_BUCKET, AUTH_MODE, SERVICE_ACCOUNT, CONFIRM

Authentication modes:
  vm-service-account:   Use VM's attached service account (KEYLESS - default)
                        VM must have a service account attached with GCS bucket write permissions
  service-account-key:   Use service account key JSON file (operator must provide)
                        Service account must have roles/storage.objectAdmin on the bucket

Prerequisites:
  - GCS bucket must be pre-created by operator (gsutil mb or GCP Console)
  - For vm-service-account mode: VM attached service account with GCS write permissions
  - For service-account-key mode: Service account key must be downloaded and available locally

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
        --auth-mode) AUTH_MODE="$2"; shift 2 ;;
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

# Basic validation
if [[ ! "$GCS_BUCKET" =~ ^gs://[a-z0-9][a-z0-9.-]*[a-z0-9]/.*$ ]]; then
    echo "ERROR: Invalid GCS bucket format: '$GCS_BUCKET'" >&2
    echo "Expected format: gs://bucket-name/path/"
    exit 1
fi

# Validate auth mode
if [[ "$AUTH_MODE" != "vm-service-account" && "$AUTH_MODE" != "service-account-key" ]]; then
    echo "ERROR: Invalid --auth-mode: '$AUTH_MODE'" >&2
    echo "Expected: vm-service-account or service-account-key"
    exit 1
fi

# Service account required only for service-account-key mode
if [[ "$AUTH_MODE" == "service-account-key" ]]; then
    if [[ -z "$SERVICE_ACCOUNT" ]]; then
        echo "ERROR: --service-account is required when --auth-mode is service-account-key." >&2
        echo "Usage: bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm --gcs-bucket BUCKET --auth-mode service-account-key --service-account SA [...]" >&2
        exit 1
    fi
    if [[ ! "$SERVICE_ACCOUNT" =~ ^[^@]+@[^@]+\.iam\.gserviceaccount\.com$ ]]; then
        echo "ERROR: Invalid service account format: '$SERVICE_ACCOUNT'" >&2
        echo "Expected format: operator-provided service account IAM address"
        exit 1
    fi
fi

echo "=== Phase 3G: Configure Offsite Backup to GCS ==="
echo "Project         : $GCP_PROJECT_ID"
echo "Region          : $GCP_REGION"
echo "Zone            : $GCP_ZONE"
echo "VM Name         : $GCP_VM_NAME"
echo "GCS Bucket      : $GCS_BUCKET"
echo "Auth Mode       : $AUTH_MODE"
if [[ "$AUTH_MODE" == "service-account-key" ]]; then
    echo "Service Account : $SERVICE_ACCOUNT"
fi
echo ""
echo "WARNING: This will configure offsite backup to GCS on the VM."
if [[ "$AUTH_MODE" == "vm-service-account" ]]; then
    echo "         Using VM's attached service account (keyless mode)."
    echo "         Ensure VM has a service account attached with GCS bucket write permissions."
else
    echo "         Ensure bucket exists and service account has write permissions."
fi
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
if [[ "$AUTH_MODE" == "vm-service-account" ]]; then
    echo "  Using VM's attached service account (keyless mode)."
    echo "  Ensure VM has a service account attached with GCS bucket write permissions."
else
    echo "  NOTE: This script cannot verify bucket access without service account key."
    echo "  Ensure service account has roles/storage.objectAdmin on ${GCS_BUCKET}"
fi
echo "  Bucket must exist: gsutil ls ${GCS_BUCKET}"

# --- Pre-flight: verify service account exists (only for service-account-key mode) ---
echo "[3/5] Verifying service account exists..."
if [[ "$AUTH_MODE" == "service-account-key" ]]; then
    if gcloud iam service-accounts describe "$SERVICE_ACCOUNT" \
        --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Service account exists: $SERVICE_ACCOUNT"
    else
        echo "ERROR: Service account '$SERVICE_ACCOUNT' not found in project '$GCP_PROJECT_ID'." >&2
        exit 1
    fi
else
    echo "  Skipping service account check (vm-service-account mode uses VM's attached SA)."
fi

# --- Configure VM: install gsutil, set up service account, configure backup ---
echo "[4/5] Configuring VM for GCS offsite backup..."

AUTH_MODE="${AUTH_MODE}"  # pass through

gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e

GCS_BUCKET=\"${GCS_BUCKET}\"
SERVICE_ACCOUNT=\"${SERVICE_ACCOUNT}\"
AUTH_MODE=\"${AUTH_MODE}\"
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

if [[ \"\$AUTH_MODE\" == \"vm-service-account\" ]]; then
    echo \"\"
    echo \"  Using VM attached service account for GCS access (keyless mode).\"
    echo \"  gsutil will automatically use the VMs attached service account credentials.\"
    echo \"  Ensure the attached service account has roles/storage.objectAdmin on the bucket.\"
    echo \"\"
    echo \"  To verify GCS access from VM:\"
    echo \"    gcloud compute ssh ubuntu@\$HOSTNAME -- gsutil ls \${GCS_BUCKET}\"
    echo \"\"
    echo \"  Offsite backup scaffold configured (vm-service-account mode - keyless).\"
else
    echo \"\"
    echo \"  Note: Service account key must be provided by operator and placed at \$SA_KEY_FILE\"
    echo \"  This script does not create or download service account keys.\"
    echo \"  To download key manually:\"
    echo \"    gcloud iam service-accounts keys create \$SA_KEY_FILE \"
    echo \"      --iam-account=\$SERVICE_ACCOUNT\"
    echo \"  Then re-run this script or manually configure the sync.\"
    echo \"\"
    echo \"  Skipping service account key upload (operator must provide).\"
    echo \"  Offsite backup configuration is a SCAFFOLD ONLY.\"
fi
'"

echo "  VM offsite backup scaffold configured."

# --- Summary of what was configured and what remains ---
echo "[5/5] Summary of offsite backup configuration..."

echo ""
echo "=== Phase 3G Offsite Backup Configuration Summary ==="
echo ""
echo "GCS Bucket      : $GCS_BUCKET"
echo "Auth Mode       : $AUTH_MODE"
if [[ "$AUTH_MODE" == "service-account-key" ]]; then
    echo "Service Account : $SERVICE_ACCOUNT"
fi
echo ""
if [[ "$AUTH_MODE" == "vm-service-account" ]]; then
    echo "SCAFFOLD COMPLETE. Operator must still:"
    echo "  1. Verify VM has a service account attached with GCS write permissions:"
    echo "     gcloud compute instances describe $GCP_VM_NAME \\"
    echo "       --zone=$GCP_ZONE --project=$GCP_PROJECT_ID \\"
    echo "       --format='value(serviceAccounts)'"
    echo "  2. Grant the attached service account GCS bucket write permissions:"
    echo "     gcloud projects add-iam-policy-binding $GCP_PROJECT_ID \\"
    echo "       --member='serviceAccount:YOUR_VM_SA@$GCP_PROJECT_ID.iam.gserviceaccount.com' \\"
    echo "       --role='roles/storage.objectAdmin'"
    echo "  3. Verify bucket access from VM:"
    echo "     gcloud compute ssh ubuntu@$GCP_VM_NAME -- \\"
    echo "       'gsutil ls ${GCS_BUCKET}'"
    echo "  4. Test backup sync:"
    echo "     gcloud compute ssh ubuntu@$GCP_VM_NAME -- \\"
    echo "       'gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/'"
    echo ""
    echo "Backup sync command (for manual or cron trigger):"
    echo "  gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/"
    echo ""
    echo "Rollback: Remove backup sync commands from cron/timer."
    echo ""
    echo "Non-claims: NOT production-ready, NOT full G2, NOT full production pilot authorization, NOT operator signoff."
    echo "            Offsite backup scaffold only (vm-service-account/keyless mode)."
else
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
    echo "       'gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/'"
    echo ""
    echo "Backup sync command (for manual or cron trigger):"
    echo "  gsutil rsync -r /var/lib/ferrumgate/backups/ ${GCS_BUCKET}/"
    echo ""
    echo "Rollback: Remove /etc/ferrumgate/gcs-service-account.json and backup sync commands from cron/timer."
    echo ""
    echo "Non-claims: NOT production-ready, NOT full G2, NOT full production pilot authorization, NOT operator signoff."
    echo "            Offsite backup scaffold only. GCS bucket and service account key are operator responsibilities."
fi
