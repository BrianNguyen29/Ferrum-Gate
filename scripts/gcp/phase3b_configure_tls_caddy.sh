#!/usr/bin/env bash
# phase3b_configure_tls_caddy.sh
# Phase 3B: Configures TLS on the existing Phase 3A VM using Caddy as a reverse proxy.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# Prerequisites (must exist from Phase 3A):
#   - VM ferrumgate-nonprod running with ferrumgate service on port 19080
#   - Static external IP assigned (34.158.51.8)
#   - TLS_DOMAIN resolves to that IP (e.g. 34-158-51-8.nip.io -> 34.158.51.8)
#
# This script:
#   - Creates GCP firewall rules for ports 80 and 443 (0.0.0.0/0, targeted to VM network tag)
#   - SSHes to VM and:
#     * Installs Caddy via official Cloudsmith apt repository
#     * Changes ferrumgate bind from 0.0.0.0:19080 to 127.0.0.1:19080 (internal only)
#     * Creates Caddy Caddyfile for TLS_DOMAIN reverse_proxy to 127.0.0.1:19080
#     * Reloads/restarts ferrumgate service
#     * Reloads Caddy
#     * Tests HTTPS endpoints: /v1/healthz, /v1/readyz, /v1/metrics
#   - Prints non-secret outputs
#
# Usage:
#   bash scripts/gcp/phase3b_configure_tls_caddy.sh [--confirm]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID   GCP project ID (default: fairy-b13f4)
#   GCP_REGION      Region (default: asia-southeast1)
#   GCP_ZONE        Zone (default: asia-southeast1-a)
#   GCP_VM_NAME     VM name (default: ferrumgate-nonprod)
#   TLS_DOMAIN      TLS domain (default: 34-158-51-8.nip.io)
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
TLS_DOMAIN="${TLS_DOMAIN:-34-158-51-8.nip.io}"
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
Phase 3B: Configure TLS on existing Phase 3A VM using Caddy reverse proxy.

Usage:
  bash scripts/gcp/phase3b_configure_tls_caddy.sh [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --tls-domain DOMAIN   TLS domain (default: 34-158-51-8.nip.io)
  --app-port PORT       FerrumGate app port (default: 19080)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, TLS_DOMAIN,
  APP_PORT, CONFIRM

Prerequisites:
  - Phase 3A VM must be running with ferrumgate service on port 19080
  - TLS_DOMAIN must resolve to the VM's external IP (DNS A record or nip.io)

Outputs on success:
  TLS_DOMAIN, HTTPS_URL, HTTP_PORT, FERRUMGATE_INTERNAL_URL

Non-claims (Phase 3B):
  NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff.
  nip.io is a temporary domain; do not use in production.
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
        --tls-domain) TLS_DOMAIN="$2"; shift 2 ;;
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
    echo "ERROR: --confirm required to configure TLS on the VM." >&2
    echo "Usage: bash scripts/gcp/phase3b_configure_tls_caddy.sh --confirm [...]" >&2
    exit 1
fi

echo "=== Phase 3B: Configure TLS with Caddy ==="
echo "Project   : $GCP_PROJECT_ID"
echo "Region    : $GCP_REGION"
echo "Zone      : $GCP_ZONE"
echo "VM Name   : $GCP_VM_NAME"
echo "TLS Domain: $TLS_DOMAIN"
echo "App Port  : $APP_PORT"
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/6] Checking VM is running..."
if ! gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "ERROR: VM '$GCP_VM_NAME' not found. Run Phase 3A create first." >&2
    exit 1
fi

EXTERNAL_IP=$(gcloud compute instances describe "$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --format='value(networkInterfaces[0].accessConfigs[0].natIP)')

echo "  VM external IP: $EXTERNAL_IP"
echo "  VM is reachable."

# --- Create HTTP firewall rule (port 80) ---
echo "[2/6] Creating firewall rules for HTTP (80) and HTTPS (443)..."

if gcloud compute firewall-rules describe "$FW_HTTP_NAME" \
    --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Firewall '$FW_HTTP_NAME' already exists."
else
    gcloud compute firewall-rules create "$FW_HTTP_NAME" \
        --network="$VPC_NAME" \
        --allow=tcp:80 \
        --source-ranges=0.0.0.0/0 \
        --target-tags="$NETWORK_TAG" \
        --project="$GCP_PROJECT_ID"
    echo "  Created firewall: $FW_HTTP_NAME (tcp:80 from 0.0.0.0/0)"
fi

if gcloud compute firewall-rules describe "$FW_HTTPS_NAME" \
    --project="$GCP_PROJECT_ID" &>/dev/null; then
    echo "  Firewall '$FW_HTTPS_NAME' already exists."
else
    gcloud compute firewall-rules create "$FW_HTTPS_NAME" \
        --network="$VPC_NAME" \
        --allow=tcp:443 \
        --source-ranges=0.0.0.0/0 \
        --target-tags="$NETWORK_TAG" \
        --project="$GCP_PROJECT_ID"
    echo "  Created firewall: $FW_HTTPS_NAME (tcp:443 from 0.0.0.0/0)"
fi

# --- Configure VM: install Caddy, update ferrumgate bind, set up Caddyfile ---
echo "[3/6] Configuring VM (install Caddy, update ferrumgate bind, configure Caddyfile)..."

gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e

# Install Caddy via official Cloudsmith apt repository
echo \"  Installing Caddy...\"
if ! command -v caddy &>/dev/null; then
    apt-get update -qq
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https &>/dev/null
    curl -1sLf \"https://dl.cloudsmith.io/public/caddy/stable/gpg.key\" \
        | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg 2>/dev/null
    curl -1sLf \"https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt\" \
        | tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null
    apt-get update -qq
    apt-get install -y -qq caddy &>/dev/null
    echo \"  Caddy installed: \$(caddy version | head -1)\"
else
    echo \"  Caddy already installed: \$(caddy version | head -1)\"
fi

# Change ferrumgate bind from 0.0.0.0 to 127.0.0.1 (internal only behind reverse proxy)
echo \"  Updating ferrumgate bind to localhost...\"
sed -i \"s|FERRUMD_BIND_ADDR=0\\.0\\.0\\.0:[0-9]*|FERRUMD_BIND_ADDR=127.0.0.1:${APP_PORT}|g\" /etc/ferrumgate/env
echo \"  ferrumgate bind updated to 127.0.0.1:${APP_PORT}\"

# Restart ferrumgate to pick up new bind address
systemctl daemon-reload
systemctl restart ferrumgate.service
sleep 2
if systemctl is-active --quiet ferrumgate.service; then
    echo \"  ferrumgate.service restarted and active (bind: 127.0.0.1:${APP_PORT})\"
else
    echo \"  WARNING: ferrumgate.service may not be active. Check: journalctl -u ferrumgate.service\"
fi

# Create Caddyfile for reverse proxy
echo \"  Creating /etc/caddy/Caddyfile...\"
cat > /etc/caddy/Caddyfile << CADDYEOF
${TLS_DOMAIN} {
    reverse_proxy 127.0.0.1:${APP_PORT}
}
CADDYEOF
chown root:root /etc/caddy/Caddyfile
chmod 644 /etc/caddy/Caddyfile

# Validate and reload Caddy
caddy validate --config /etc/caddy/Caddyfile
caddy reload --config /etc/caddy/Caddyfile --force
echo \"  Caddy configured and reloaded for ${TLS_DOMAIN}\"
'"

echo "  VM configuration complete."

# --- Verify HTTPS endpoints ---
echo "[4/6] Verifying HTTPS endpoints (waiting for ACME certificate provisioning)..."

# Wait up to 60s for Let's Encrypt certificate to be provisioned
MAX_WAIT=60
WAIT_INTERVAL=5
WAITED=0
CERT_READY=false

echo "  Waiting for TLS certificate (up to ${MAX_WAIT}s)..."
while [[ $WAITED -lt $MAX_WAIT ]]; do
    CERT_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://${TLS_DOMAIN}/v1/healthz" 2>/dev/null)
    if [[ "$CERT_STATUS" != "000" ]]; then
        CERT_READY=true
        break
    fi
    sleep $WAIT_INTERVAL
    WAITED=$((WAITED + WAIT_INTERVAL))
    echo "    still provisioning... (${WAITED}s)"
done

if [[ "$CERT_READY" == "false" ]]; then
    echo "ERROR: TLS certificate not provisioned after ${MAX_WAIT}s. Check Caddy logs." >&2
    exit 1
fi
echo "  Certificate provisioned (${WAITED}s)."

# Retrieve bearer token prefix from VM using sudo (file is mode 600, owned by ferrumgate)
TOKEN_PREFIX=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo grep FERRUMD_BEARER_TOKEN /etc/ferrumgate/env | cut -d= -f2 | head -c 8")

echo "  Token prefix: ${TOKEN_PREFIX}..."

# Helper: check endpoint and fail if not HTTP 200
check_endpoint() {
    local path="$1"
    local description="$2"
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://${TLS_DOMAIN}${path}" 2>/dev/null)
    echo "  ${description}: HTTP $status"
    if [[ "$status" != "200" ]]; then
        echo "ERROR: ${description} returned HTTP $status, expected 200. Aborting." >&2
        exit 1
    fi
}

echo "  Testing /v1/healthz..."
check_endpoint "/v1/healthz" "/v1/healthz"

echo "  Testing /v1/readyz..."
check_endpoint "/v1/readyz" "/v1/readyz"

echo "  Testing /v1/metrics..."
check_endpoint "/v1/metrics" "/v1/metrics"

# --- Summary ---
echo ""
echo "[5/6] Fetching Caddy certificate status..."
CADDY_STATUS=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "caddy list-certificates 2>/dev/null | head -10 || echo 'certificate status unavailable'")
echo "$CADDY_STATUS"

echo ""
echo "[6/6] Summary..."
echo ""
echo "=== Phase 3B TLS Configuration Complete ==="
echo "TLS_DOMAIN            : $TLS_DOMAIN"
echo "HTTPS_URL             : https://${TLS_DOMAIN}"
echo "HTTP_PORT             : 80 (Caddy ACME challenge)"
echo "HTTPS_PORT            : 443 (TLS terminated by Caddy)"
echo "FERRUMGATE_INTERNAL  : http://127.0.0.1:${APP_PORT} (localhost only)"
echo "EXTERNAL_IP          : $EXTERNAL_IP"
echo ""
echo "Test with real token:"
echo "  TOKEN=\$(gcloud compute ssh ubuntu@${GCP_VM_NAME} --zone=${GCP_ZONE} --project=${GCP_PROJECT_ID} --quiet -- 'sudo cat /etc/ferrumgate/ferrumgate_initial_token')"
echo "  curl -H \"Authorization: Bearer \${TOKEN}\" https://${TLS_DOMAIN}/v1/healthz"
echo ""
echo "Rollback: Run phase3b_destroy_tls_caddy.sh --confirm to remove TLS and restore Phase 3A."
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff."
echo "            nip.io is a temporary domain; do not use in production."
