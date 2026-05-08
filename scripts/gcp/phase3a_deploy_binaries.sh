#!/usr/bin/env bash
# phase3a_deploy_binaries.sh
# Phase 3A: Builds and deploys FerrumGate binaries to the GCP non-prod VM.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# This script:
#   - Builds release binaries (ferrumd, ferrumctl) locally if needed
#   - Copies binaries and bootstrap script to VM via gcloud compute scp/ssh
#   - Runs bootstrap script on VM
#   - Restarts the ferrumgate service
#   - Prints non-secret outputs (token prefix, endpoints)
#
# Usage:
#   bash scripts/gcp/phase3a_deploy_binaries.sh \
#     --project-id PROJECT_ID \
#     --region REGION \
#     --zone ZONE \
#     [--build-only] \
#   [--confirm]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID    GCP project ID (default: fairy-b13f4)
#   GCP_REGION        Region (default: asia-southeast1)
#   GCP_ZONE          Zone (default: asia-southeast1-a)
#   GCP_VM_NAME       VM name (default: ferrumgate-nonprod)
#   FERRUM_VERSION    Version string to embed in bootstrap (default: dev-build)
#
# Required flags / env for actual deployment:
#   --confirm  or  CONFIRM=true

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
FERRUM_VERSION="${FERRUM_VERSION:-dev-build}"
BUILD_ONLY="${BUILD_ONLY:-false}"
CONFIRM="${CONFIRM:-false}"

# Derived names
VPC_NAME="${GCP_VM_NAME}-vpc"
SUBNET_NAME="${GCP_VM_NAME}-subnet"

# Build output
BUILD_DIR="$REPO_ROOT/target/release"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3A: Deploy FerrumGate binaries to GCP non-prod VM.

Usage:
  bash scripts/gcp/phase3a_deploy_binaries.sh [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --version VERSION     Version string for bootstrap (default: dev-build)
  --build-only          Only build binaries, do not deploy
  --confirm             Required: acknowledge before deploying to VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, FERRUM_VERSION,
  BUILD_ONLY, CONFIRM

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
        --version) FERRUM_VERSION="$2"; shift 2 ;;
        --build-only) BUILD_ONLY="true"; shift ;;
        --confirm) CONFIRM="true"; shift ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

# --- Build check ---
build_binaries() {
    echo "[1/5] Building release binaries locally..."
    if [[ -x "$BUILD_DIR/ferrumd" && -x "$BUILD_DIR/ferrumctl" ]]; then
        echo "  Binaries already exist at $BUILD_DIR"
    else
        echo "  Building binaries (this may take several minutes)..."
        cd "$REPO_ROOT"
        cargo build --release --package ferrumd --package ferrumctl
    fi

    if [[ ! -x "$BUILD_DIR/ferrumd" ]]; then
        echo "ERROR: ferrumd binary not found at $BUILD_DIR/ferrumd" >&2
        exit 1
    fi
    if [[ ! -x "$BUILD_DIR/ferrumctl" ]]; then
        echo "ERROR: ferrumctl binary not found at $BUILD_DIR/ferrumctl" >&2
        exit 1
    fi

    FERRUMD_SIZE=$(stat -c%s "$BUILD_DIR/ferrumd" 2>/dev/null || stat -f%z "$BUILD_DIR/ferrumd" 2>/dev/null)
    FERRUMCTL_SIZE=$(stat -c%s "$BUILD_DIR/ferrumctl" 2>/dev/null || stat -f%z "$BUILD_DIR/ferrumctl" 2>/dev/null)
    echo "  ferrumd   : $FERRUMD_SIZE bytes"
    echo "  ferrumctl : $FERRUMCTL_SIZE bytes"
}

# --- VM pre-flight ---
check_vm() {
    echo "[2/5] Checking VM is reachable..."
    if ! gcloud compute instances describe "$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "ERROR: VM '$GCP_VM_NAME' not found in $GCP_ZONE. Create it first." >&2
        exit 1
    fi

    EXTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --format='value(networkInterfaces[0].accessConfigs[0].natIP)')

    INTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --format='value(networkInterfaces[0].networkIP)')

    echo "  VM external IP : $EXTERNAL_IP"
    echo "  VM internal IP : $INTERNAL_IP"

    # Test SSH connectivity (lightweight check)
    echo "  Testing SSH connectivity to $EXTERNAL_IP..."
    if gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- 'echo "SSH OK"' &>/dev/null; then
        echo "  SSH connectivity confirmed."
    else
        echo "WARNING: Could not confirm SSH connectivity. Will attempt deployment anyway."
    fi
}

# --- Deploy ---
deploy_binaries() {
    echo "[3/5] Deploying binaries to VM..."

    # Ensure install directory and service user exist before moving binaries
    echo "  Preparing install directory and user on VM..."
    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo mkdir -p /opt/ferrumgate && \
         id ferrumgate &>/dev/null || sudo useradd --system --no-create-home --shell=/usr/sbin/nologin ferrumgate && \
         echo 'Install dir and user ready'"

    # Copy binaries
    echo "  Copying ferrumd and ferrumctl to VM..."
    gcloud compute scp \
        "$BUILD_DIR/ferrumd" \
        "$BUILD_DIR/ferrumctl" \
        ubuntu@"$GCP_VM_NAME":/tmp/ \
        --zone="$GCP_ZONE" \
        --project="$GCP_PROJECT_ID" \
        --quiet

    # Move to install directory on VM
    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo mv /tmp/ferrumd /opt/ferrumgate/ferrumd && \
         sudo mv /tmp/ferrumctl /opt/ferrumgate/ferrumctl && \
         sudo chmod +x /opt/ferrumgate/ferrumd /opt/ferrumgate/ferrumctl && \
         sudo chown ferrumgate:ferrumgate /opt/ferrumgate/ferrumd /opt/ferrumgate/ferrumctl && \
         echo 'Binaries installed to /opt/ferrumgate/'"

    echo "  Binaries deployed to /opt/ferrumgate/"
}

# --- Run bootstrap ---
run_bootstrap() {
    echo "[4/5] Running bootstrap script on VM..."

    # Copy bootstrap script to VM and execute
    gcloud compute scp \
        "$SCRIPT_DIR/phase3a_bootstrap_vm.sh" \
        ubuntu@"$GCP_VM_NAME":/tmp/phase3a_bootstrap_vm.sh \
        --zone="$GCP_ZONE" \
        --project="$GCP_PROJECT_ID" \
        --quiet

    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "FERRUM_VERSION=$FERRUM_VERSION sudo bash /tmp/phase3a_bootstrap_vm.sh"

    echo "  Bootstrap completed on VM."
}

# --- Restart service ---
restart_service() {
    echo "[5/5] Restarting ferrumgate service on VM..."

    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo systemctl daemon-reload && \
         sudo systemctl restart ferrumgate.service && \
         sudo systemctl status ferrumgate.service --no-pager"

    echo "  Service restarted."
}

# --- Get outputs (token prefix only) ---
get_outputs() {
    echo ""
    echo "=== Deployment Complete ==="
    echo "VM_NAME     : $GCP_VM_NAME"
    echo "EXTERNAL_IP : $EXTERNAL_IP"
    echo "INTERNAL_IP : $INTERNAL_IP"
    echo "APP_PORT    : 19080"
    echo "VERSION     : $FERRUM_VERSION"

    # Retrieve token prefix from VM (never print full token)
    TOKEN_PREFIX=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo grep FERRUMD_BEARER_TOKEN /etc/ferrumgate/env | cut -d= -f2 | head -c 8")

    echo ""
    echo "Token prefix: ${TOKEN_PREFIX}..."
    echo "Token stored : /etc/ferrumgate/ferrumgate_initial_token (root-only on VM)"
    echo ""
    echo "Test commands:"
    echo "  curl -H 'Authorization: Bearer <full-token>' http://$EXTERNAL_IP:19080/v1/healthz"
    echo "  gcloud compute ssh ubuntu@$GCP_VM_NAME --zone=$GCP_ZONE --project=$GCP_PROJECT_ID"
    echo ""
    echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized."
}

# --- Main ---
echo "=== Phase 3A: Deploy FerrumGate Binaries to GCP VM ==="
echo "Project  : $GCP_PROJECT_ID"
echo "Region   : $GCP_REGION"
echo "Zone     : $GCP_ZONE"
echo "VM Name  : $GCP_VM_NAME"
echo "Version  : $FERRUM_VERSION"
echo ""

build_binaries

if [[ "$BUILD_ONLY" == "true" ]]; then
    echo "Build-only mode: binaries ready at $BUILD_DIR"
    exit 0
fi

# --- Require explicit confirmation for deployment ---
if [[ "$CONFIRM" != "true" ]]; then
    echo "ERROR: --confirm required to deploy binaries to VM." >&2
    echo "Usage: bash scripts/gcp/phase3a_deploy_binaries.sh --confirm [...]" >&2
    exit 1
fi

check_vm
deploy_binaries
run_bootstrap
restart_service
get_outputs
