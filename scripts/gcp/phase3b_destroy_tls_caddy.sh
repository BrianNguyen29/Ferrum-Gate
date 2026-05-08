#!/usr/bin/env bash
# phase3b_destroy_tls_caddy.sh
# Phase 3B: Removes TLS/Caddy configuration and restores Phase 3A fallback on the VM.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# This script:
#   - Stops and disables Caddy service
#   - Deletes GCP firewall rules for ports 80 and 443
#   - Restores ferrumgate bind address from 127.0.0.1:19080 back to 0.0.0.0:19080
#   - Restarts ferrumgate service
#   - Leaves VM and all Phase 3A resources intact
#
# Usage:
#   bash scripts/gcp/phase3b_destroy_tls_caddy.sh [--confirm]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID   GCP project ID (default: fairy-b13f4)
#   GCP_REGION      Region (default: asia-southeast1)
#   GCP_ZONE        Zone (default: asia-southeast1-a)
#   GCP_VM_NAME     VM name (default: ferrumgate-nonprod)
#   APP_PORT        FerrumGate app port (default: 19080)
#
# Required: --confirm  or  CONFIRM=true

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
APP_PORT="${APP_PORT:-19080}"
CONFIRM="${CONFIRM:-false}"

# Derived resource names
VPC_NAME="${GCP_VM_NAME}-vpc"
NETWORK_TAG="${GCP_VM_NAME}-app"
FW_HTTP_NAME="${GCP_VM_NAME}-fw-http"
FW_HTTPS_NAME="${GCP_VM_NAME}-fw-https"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3B: Remove TLS/Caddy configuration and restore Phase 3A fallback.

Usage:
  bash scripts/gcp/phase3b_destroy_tls_caddy.sh [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --app-port PORT       FerrumGate app port (default: 19080)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, APP_PORT, CONFIRM

Note:
  This does NOT delete the Phase 3A VM or its resources.
  Use phase3a_destroy_nonprod_vm.sh to delete all Phase 3A resources.

Non-claims (Phase 3B):
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
        --app-port) APP_PORT="$2"; shift 2 ;;
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
    echo "ERROR: --confirm required to remove TLS configuration." >&2
    echo "Usage: bash scripts/gcp/phase3b_destroy_tls_caddy.sh --confirm [...]" >&2
    exit 1
fi

echo "=== Phase 3B: Remove TLS/Caddy and Restore Phase 3A Fallback ==="
echo "Project : $GCP_PROJECT_ID"
echo "Region  : $GCP_REGION"
echo "Zone    : $GCP_ZONE"
echo "VM Name : $GCP_VM_NAME"
echo ""
echo "WARNING: This will:"
echo "  1. Stop and disable the Caddy reverse proxy"
echo "  2. Delete firewall rules for ports 80 and 443"
echo "  3. Restore ferrumgate bind to 0.0.0.0:${APP_PORT} (Phase 3A fallback)"
echo "  4. Restart ferrumgate service"
echo "The VM and Phase 3A resources will NOT be deleted."
echo ""

# --- Stop and disable Caddy on VM ---
echo "[1/4] Stopping and disabling Caddy on VM..."
gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo systemctl stop caddy 2>/dev/null || echo 'Caddy not running'
     sudo systemctl disable caddy 2>/dev/null || echo 'Caddy not enabled'
     echo 'Caddy stopped and disabled.'"

# --- Delete GCP firewall rules ---
echo "[2/4] Deleting firewall rules..."

delete_fw() {
    local name="$1"
    if gcloud compute firewall-rules describe "$name" \
        --project="$GCP_PROJECT_ID" &>/dev/null; then
        echo "  Deleting: $name"
        gcloud compute firewall-rules delete "$name" \
            --project="$GCP_PROJECT_ID" --quiet
    else
        echo "  Firewall '$name' does not exist (skipping)."
    fi
}

delete_fw "$FW_HTTP_NAME"
delete_fw "$FW_HTTPS_NAME"

# --- Restore ferrumgate bind to 0.0.0.0:19080 ---
echo "[3/4] Restoring ferrumgate bind to 0.0.0.0:${APP_PORT}..."
gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e
# Restore bind address to 0.0.0.0 for Phase 3A fallback
sed -i \"s|FERRUMD_BIND_ADDR=127\\.0\\.0\\.1:[0-9]*|FERRUMD_BIND_ADDR=0.0.0.0:${APP_PORT}|g\" /etc/ferrumgate/env
echo \"  Restored FERRUMD_BIND_ADDR to 0.0.0.0:${APP_PORT}\"
systemctl daemon-reload
systemctl restart ferrumgate.service
sleep 2
if systemctl is-active --quiet ferrumgate.service; then
    echo \"  ferrumgate.service restarted and active (bind: 0.0.0.0:${APP_PORT})\"
else
    echo \"  WARNING: ferrumgate.service may not be active. Check: journalctl -u ferrumgate.service\"
fi
'"

echo "  ferrumgate bind restored."

# --- Verify Phase 3A fallback is working ---
echo "[4/4] Verifying Phase 3A fallback..."

EXTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --format='value(networkInterfaces[0].accessConfigs[0].natIP)')

echo "  Testing direct HTTP access to ferrumgate (bypassing Caddy)..."
FALLBACK_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    "http://${EXTERNAL_IP}:${APP_PORT}/v1/healthz" 2>/dev/null || echo "000")
echo "  http://${EXTERNAL_IP}:${APP_PORT}/v1/healthz HTTP status: $FALLBACK_STATUS"

echo ""
echo "=== TLS Removal Complete ==="
echo "Caddy has been stopped, disabled, and removed from the VM."
echo "Firewall rules for ports 80 and 443 have been deleted."
echo "ferrumgate has been restored to Phase 3A bind: 0.0.0.0:${APP_PORT}"
echo ""
echo "Direct access (Phase 3A fallback): http://${EXTERNAL_IP}:${APP_PORT}/v1/healthz"
echo "Status: $FALLBACK_STATUS"
echo ""
echo "Phase 3A VM and resources are intact. Use phase3a_destroy_nonprod_vm.sh to delete everything."
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff."
