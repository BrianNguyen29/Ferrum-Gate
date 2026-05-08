#!/usr/bin/env bash
# phase3a_create_nonprod_vm.sh
# Phase 3A: Creates a GCP non-prod Compute Engine VM for FerrumGate target rehearsal.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# Creates:
#   - Custom VPC network and subnet (asia-southeast1)
#   - Regional static external IP
#   - Firewall rules: SSH 22 and app port 19080 from allowlist IP only
#   - Ubuntu 24.04 LTS amd64 e2-medium VM with 30GB pd-balanced disk and network tag
#
# Usage:
#   bash scripts/gcp/phase3a_create_nonprod_vm.sh \
#     --project-id PROJECT_ID \
#     --region REGION \
#   [--confirm]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID       GCP project ID (default: fairy-b13f4)
#   GCP_REGION           Region (default: asia-southeast1)
#   GCP_ZONE             Zone (default: asia-southeast1-a)
#   GCP_VM_NAME          VM name (default: ferrumgate-nonprod)
#   GCP_ALLOWLIST_CIDR   Source CIDR for firewall rules (default: 118.69.4.63/32)
#   GCP_APP_PORT         App port (default: 19080)
#   GCP_MACHINE_TYPE     Machine type (default: e2-medium)
#   GCP_DISK_SIZE_GB     Disk size in GB (default: 30)
#
# Required flags / env for actual creation:
#   --confirm  or  CONFIRM=true   (avoids accidental cost)
#
# This script is idempotent-ish: checks for existing resources before creation.
# Prints outputs (VM name, IP, etc.) on success.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
GCP_ALLOWLIST_CIDR="${GCP_ALLOWLIST_CIDR:-118.69.4.63/32}"
GCP_APP_PORT="${GCP_APP_PORT:-19080}"
GCP_MACHINE_TYPE="${GCP_MACHINE_TYPE:-e2-medium}"
GCP_DISK_SIZE_GB="${GCP_DISK_SIZE_GB:-30}"
CONFIRM="${CONFIRM:-false}"

# Derived names
VPC_NAME="${GCP_VM_NAME}-vpc"
SUBNET_NAME="${GCP_VM_NAME}-subnet"
STATIC_IP_NAME="${GCP_VM_NAME}-ip"
FW_SSH_NAME="${GCP_VM_NAME}-fw-ssh"
FW_APP_NAME="${GCP_VM_NAME}-fw-app"
NETWORK_TAGS="${GCP_VM_NAME}-app"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3A: Create GCP non-prod VM for FerrumGate target rehearsal.

Usage:
  bash scripts/gcp/phase3a_create_nonprod_vm.sh [options]

Options:
  --help                  Show this help and exit
  --project-id ID         GCP project ID (default: fairy-b13f4)
  --region REGION         GCP region (default: asia-southeast1)
  --zone ZONE             GCP zone (default: asia-southeast1-a)
  --vm-name NAME          VM name (default: ferrumgate-nonprod)
  --allowlist-cidr CIDR   Source CIDR for firewall (default: 118.69.4.63/32)
  --app-port PORT         App port (default: 19080)
  --machine-type TYPE     Machine type (default: e2-medium)
  --disk-size-gb GB       Disk size in GB (default: 30)
  --confirm               Required: acknowledge cost before creating resources

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, GCP_ALLOWLIST_CIDR,
  GCP_APP_PORT, GCP_MACHINE_TYPE, GCP_DISK_SIZE_GB, CONFIRM

Outputs on success:
  VM_NAME, INTERNAL_IP, EXTERNAL_IP, ZONE, REGION, PROJECT_ID

Non-claims (Phase 3A):
  NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff.
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
        --allowlist-cidr) GCP_ALLOWLIST_CIDR="$2"; shift 2 ;;
        --app-port) GCP_APP_PORT="$2"; shift 2 ;;
        --machine-type) GCP_MACHINE_TYPE="$2"; shift 2 ;;
        --disk-size-gb) GCP_DISK_SIZE_GB="$2"; shift 2 ;;
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
    echo "ERROR: --confirm required to create GCP resources (incurs cost)." >&2
    echo "Usage: bash scripts/gcp/phase3a_create_nonprod_vm.sh --confirm [...]" >&2
    exit 1
fi

echo "=== Phase 3A: Create GCP Non-Prod VM ==="
echo "Project : $GCP_PROJECT_ID"
echo "Region  : $GCP_REGION"
echo "Zone    : $GCP_ZONE"
echo "VM Name : $GCP_VM_NAME"
echo "App Port: $GCP_APP_PORT"
echo "Allowlist: $GCP_ALLOWLIST_CIDR"
echo ""

# --- Pre-flight check: project and API ---
echo "[1/6] Validating project and Compute API..."
if ! gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  VM does not exist (will create)."
else
    echo "  VM '$GCP_VM_NAME' already exists in $GCP_ZONE."
    echo "  Skipping create. Use phase3a_destroy_nonprod_vm.sh to remove first."
    exit 0
fi

# --- Create VPC ---
echo "[2/6] Creating VPC '$VPC_NAME'..."
if gcloud compute networks describe "$VPC_NAME" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  VPC '$VPC_NAME' already exists."
else
    gcloud compute networks create "$VPC_NAME" \
        --subnet-mode=custom \
        --project="$GCP_PROJECT_ID"
fi

# --- Create subnet ---
echo "[3/6] Creating subnet '$SUBNET_NAME' in $GCP_REGION..."
if gcloud compute networks subnets describe "$SUBNET_NAME" \
    --region="$GCP_REGION" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Subnet '$SUBNET_NAME' already exists."
else
    gcloud compute networks subnets create "$SUBNET_NAME" \
        --network="$VPC_NAME" \
        --region="$GCP_REGION" \
        --range=10.0.0.0/24 \
        --project="$GCP_PROJECT_ID"
fi

# --- Create static IP ---
echo "[4/6] Creating static external IP '$STATIC_IP_NAME'..."
if gcloud compute addresses describe "$STATIC_IP_NAME" \
    --region="$GCP_REGION" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Static IP '$STATIC_IP_NAME' already exists."
    EXTERNAL_IP=$(gcloud compute addresses describe "$STATIC_IP_NAME" \
        --region="$GCP_REGION" --project="$GCP_PROJECT_ID" \
        --format='value(address)')
else
    gcloud compute addresses create "$STATIC_IP_NAME" \
        --region="$GCP_REGION" \
        --project="$GCP_PROJECT_ID"
    EXTERNAL_IP=$(gcloud compute addresses describe "$STATIC_IP_NAME" \
        --region="$GCP_REGION" --project="$GCP_PROJECT_ID" \
        --format='value(address)')
fi
echo "  External IP: $EXTERNAL_IP"

# --- Create firewall rules ---
echo "[5/6] Creating firewall rules..."

# SSH firewall (allow from allowlist only)
if gcloud compute firewall-rules describe "$FW_SSH_NAME" \
    --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Firewall '$FW_SSH_NAME' already exists."
else
    gcloud compute firewall-rules create "$FW_SSH_NAME" \
        --network="$VPC_NAME" \
        --allow=tcp:22 \
        --source-ranges="$GCP_ALLOWLIST_CIDR" \
        --target-tags="$NETWORK_TAGS" \
        --project="$GCP_PROJECT_ID"
fi

# App port firewall (allow from allowlist only)
if gcloud compute firewall-rules describe "$FW_APP_NAME" \
    --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Firewall '$FW_APP_NAME' already exists."
else
    gcloud compute firewall-rules create "$FW_APP_NAME" \
        --network="$VPC_NAME" \
        --allow=tcp:"$GCP_APP_PORT" \
        --source-ranges="$GCP_ALLOWLIST_CIDR" \
        --target-tags="$NETWORK_TAGS" \
        --project="$GCP_PROJECT_ID"
fi

# --- Create VM ---
echo "[6/6] Creating VM '$GCP_VM_NAME'..."
if gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  VM already exists."
else
    gcloud compute instances create "$GCP_VM_NAME" \
        --zone="$GCP_ZONE" \
        --machine-type="$GCP_MACHINE_TYPE" \
        --image-family=ubuntu-2404-lts-amd64 \
        --image-project=ubuntu-os-cloud \
        --boot-disk-size="$GCP_DISK_SIZE_GB" \
        --boot-disk-type=pd-balanced \
        --network-interface="network=$VPC_NAME,subnet=$SUBNET_NAME,address=$EXTERNAL_IP" \
        --tags="$NETWORK_TAGS" \
        --project="$GCP_PROJECT_ID"
fi

# --- Gather outputs ---
INTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --format='value(networkInterfaces[0].networkIP)')

echo ""
echo "=== Creation Complete ==="
echo "VM_NAME      : $GCP_VM_NAME"
echo "INTERNAL_IP  : $INTERNAL_IP"
echo "EXTERNAL_IP  : $EXTERNAL_IP"
echo "ZONE         : $GCP_ZONE"
echo "REGION       : $GCP_REGION"
echo "PROJECT_ID   : $GCP_PROJECT_ID"
echo "SSH_TARGET   : $EXTERNAL_IP"
echo ""
echo "Next step: Use phase3a_deploy_binaries.sh to deploy ferrumd and bootstrap."
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized."
