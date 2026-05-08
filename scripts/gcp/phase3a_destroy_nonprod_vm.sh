#!/usr/bin/env bash
# phase3a_destroy_nonprod_vm.sh
# Phase 3A: Destroys GCP non-prod Compute Engine resources created for FerrumGate rehearsal.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# Destroys (in safe order):
#   - VM instance
#   - Firewall rules (SSH and app)
#   - Static external IP
#   - Subnet
#   - VPC network
#
# Usage:
#   bash scripts/gcp/phase3a_destroy_nonprod_vm.sh \
#     --project-id PROJECT_ID \
#     --region REGION \
#     --zone ZONE \
#   [--confirm]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID   GCP project ID (default: fairy-b13f4)
#   GCP_REGION       Region (default: asia-southeast1)
#   GCP_ZONE         Zone (default: asia-southeast1-a)
#   GCP_VM_NAME      VM name (default: ferrumgate-nonprod)
#
# Required flags / env for actual destruction:
#   --confirm  or  CONFIRM=true   (avoids accidental data loss)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
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
Phase 3A: Destroy GCP non-prod VM and related resources.

Usage:
  bash scripts/gcp/phase3a_destroy_nonprod_vm.sh [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --confirm             Required: acknowledge destruction before proceeding

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, CONFIRM

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
    echo "ERROR: --confirm required to destroy GCP resources." >&2
    echo "Usage: bash scripts/gcp/phase3a_destroy_nonprod_vm.sh --confirm [...]" >&2
    exit 1
fi

echo "=== Phase 3A: Destroy GCP Non-Prod VM ==="
echo "Project : $GCP_PROJECT_ID"
echo "Region  : $GCP_REGION"
echo "Zone    : $GCP_ZONE"
echo "VM Name : $GCP_VM_NAME"
echo ""
echo "WARNING: This will permanently delete:"
echo "  - VM instance: $GCP_VM_NAME"
echo "  - Static IP: $STATIC_IP_NAME"
echo "  - Firewall rules: $FW_SSH_NAME, $FW_APP_NAME"
echo "  - Subnet: $SUBNET_NAME"
echo "  - VPC: $VPC_NAME"
echo ""

# --- Helper to check existence and delete ---
delete_fw() {
    local name="$1"
    if gcloud compute firewall-rules describe "$name" \
        --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Deleting firewall rule: $name"
        gcloud compute firewall-rules delete "$name" \
            --project="$GCP_PROJECT_ID" --quiet
    else
        echo "  Firewall rule '$name' does not exist (skipping)."
    fi
}

delete_address() {
    local name="$1"
    if gcloud compute addresses describe "$name" \
        --region="$GCP_REGION" --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Deleting static address: $name"
        gcloud compute addresses delete "$name" \
            --region="$GCP_REGION" --project="$GCP_PROJECT_ID" --quiet
    else
        echo "  Static address '$name' does not exist (skipping)."
    fi
}

delete_subnet() {
    local name="$1"
    if gcloud compute networks subnets describe "$name" \
        --region="$GCP_REGION" --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Deleting subnet: $name"
        gcloud compute networks subnets delete "$name" \
            --region="$GCP_REGION" --project="$GCP_PROJECT_ID" --quiet
    else
        echo "  Subnet '$name' does not exist (skipping)."
    fi
}

delete_vpc() {
    local name="$1"
    if gcloud compute networks describe "$name" \
        --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Deleting VPC: $name"
        gcloud compute networks delete "$name" \
            --project="$GCP_PROJECT_ID" --quiet
    else
        echo "  VPC '$name' does not exist (skipping)."
    fi
}

# --- Delete VM ---
echo "[1/5] Deleting VM instance..."
if gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Deleting VM: $GCP_VM_NAME"
    gcloud compute instances delete "$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" --quiet
else
    echo "  VM '$GCP_VM_NAME' does not exist (skipping)."
fi

# --- Delete firewall rules ---
echo "[2/5] Deleting firewall rules..."
delete_fw "$FW_SSH_NAME"
delete_fw "$FW_APP_NAME"

# --- Delete static IP ---
echo "[3/5] Deleting static IP..."
delete_address "$STATIC_IP_NAME"

# --- Delete subnet ---
echo "[4/5] Deleting subnet..."
delete_subnet "$SUBNET_NAME"

# --- Delete VPC ---
echo "[5/5] Deleting VPC..."
delete_vpc "$VPC_NAME"

echo ""
echo "=== Destruction Complete ==="
echo "All Phase 3A GCP resources for '$GCP_VM_NAME' have been deleted."
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized."
