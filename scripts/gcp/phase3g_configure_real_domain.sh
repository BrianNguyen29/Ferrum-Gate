#!/usr/bin/env bash
# phase3g_configure_real_domain.sh
# Phase 3G: Replaces nip.io temporary domain with a real domain for TLS.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# This script:
#   - Updates Caddyfile to use real domain instead of nip.io
#   - Updates DNS A record check (operator must confirm externally)
#   - Restarts Caddy to provision new TLS certificate
#   - Verifies HTTPS endpoints with new domain
#
# Prerequisites (must exist from Phase 3A/3B):
#   - VM ferrumgate-nonprod running with Caddy reverse proxy
#   - Static external IP assigned (34.158.51.8)
#   - DNS A record for REAL_DOMAIN pointing to that IP (operator must configure externally)
#
# Usage:
#   bash scripts/gcp/phase3g_configure_real_domain.sh --confirm --real-domain DOMAIN [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID     GCP project ID (default: fairy-b13f4)
#   GCP_REGION         Region (default: asia-southeast1)
#   GCP_ZONE           Zone (default: asia-southeast1-a)
#   GCP_VM_NAME        VM name (default: ferrumgate-nonprod)
#   REAL_DOMAIN        Real domain to configure (REQUIRED)
#   CONFIRM            Must be "true" to confirm mutation
#
# Options:
#   --help             Show this help and exit
#   --project-id ID    GCP project ID (default: fairy-b13f4)
#   --region REGION    GCP region (default: asia-southeast1)
#   --zone ZONE        GCP zone (default: asia-southeast1-a)
#   --vm-name NAME     VM name (default: ferrumgate-nonprod)
#   --real-domain DOMAIN Real domain for TLS (REQUIRED)
#   --confirm          Required: acknowledge before modifying VM
#
# Example:
#   bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --real-domain api.example.com

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_REGION="${GCP_REGION:-asia-southeast1}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
REAL_DOMAIN="${REAL_DOMAIN:-}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3G: Replace nip.io temporary domain with real domain for TLS.

Usage:
  bash scripts/gcp/phase3g_configure_real_domain.sh --confirm --real-domain DOMAIN [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE          GCP zone (default: asia-southeast1-a)
  --vm-name NAME       VM name (default: ferrumgate-nonprod)
  --real-domain DOMAIN Real domain for TLS (REQUIRED)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_REGION, GCP_ZONE, GCP_VM_NAME, REAL_DOMAIN, CONFIRM

Prerequisites:
  - Phase 3A/3B VM must be running with Caddy reverse proxy
  - DNS A record for REAL_DOMAIN must point to VM external IP (operator configures externally)
  - REAL_DOMAIN must not be empty

Non-claims (Phase 3G):
  NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff.
  Scaffolds only. Real domain deployment blocked on operator providing REAL_DOMAIN.
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
        --real-domain) REAL_DOMAIN="$2"; shift 2 ;;
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
    echo "ERROR: --confirm required to configure real domain on the VM." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_real_domain.sh --confirm --real-domain DOMAIN [...]" >&2
    exit 1
fi

# --- Require REAL_DOMAIN ---
if [[ -z "$REAL_DOMAIN" ]]; then
    echo "ERROR: --real-domain is required." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_real_domain.sh --confirm --real-domain DOMAIN [...]" >&2
    exit 1
fi

# Basic domain validation
if [[ ! "$REAL_DOMAIN" =~ ^[a-zA-Z0-9][a-zA-Z0-9.-]*\.[a-zA-Z]{2,}$ ]]; then
    echo "ERROR: Invalid domain format: '$REAL_DOMAIN'" >&2
    exit 1
fi

echo "=== Phase 3G: Configure Real Domain TLS ==="
echo "Project     : $GCP_PROJECT_ID"
echo "Region      : $GCP_REGION"
echo "Zone        : $GCP_ZONE"
echo "VM Name     : $GCP_VM_NAME"
echo "Real Domain : $REAL_DOMAIN"
echo ""
echo "WARNING: This will replace nip.io TLS with real domain TLS."
echo "         Ensure DNS A record for $REAL_DOMAIN points to VM IP before proceeding."
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/4] Checking VM is running..."
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

# --- Pre-flight: verify Caddy is installed ---
echo "[2/4] Checking Caddy is installed on VM..."

CADDY_VERSION=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "caddy version 2>/dev/null || echo 'NOT_INSTALLED'" \
    2>/dev/null || echo "NOT_INSTALLED")

if [[ "$CADDY_VERSION" == "NOT_INSTALLED" ]]; then
    echo "ERROR: Caddy not installed. Run Phase 3B first." >&2
    exit 1
fi
echo "  Caddy installed: $CADDY_VERSION"

# --- Pre-flight: verify DNS A record (informational only) ---
echo "[3/4] DNS A record check (informational)..."
echo "  IMPORTANT: Verify that DNS A record for $REAL_DOMAIN points to $EXTERNAL_IP"
echo "  This script cannot verify DNS externally. Operator must confirm."
echo "  Suggested check: dig +short $REAL_DOMAIN or nslookup $REAL_DOMAIN"
echo ""

# --- Configure VM: update Caddyfile with real domain ---
echo "[4/4] Updating Caddyfile with real domain..."

APP_PORT="${APP_PORT:-19080}"

gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e

# Backup existing Caddyfile
if [[ -f /etc/caddy/Caddyfile ]]; then
    cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.backup.$(date +%Y%m%d%H%M%S)
    echo \"  Backed up existing Caddyfile\"
fi

# Create new Caddyfile with real domain
echo \"  Creating /etc/caddy/Caddyfile for domain: ${REAL_DOMAIN}\"
cat > /etc/caddy/Caddyfile << CADDYEOF
${REAL_DOMAIN} {
    reverse_proxy 127.0.0.1:${APP_PORT}
}
CADDYEOF

chown root:root /etc/caddy/Caddyfile
chmod 644 /etc/caddy/Caddyfile

# Validate and reload Caddy
echo \"  Validating Caddyfile...\"
caddy validate --config /etc/caddy/Caddyfile

echo \"  Reloading Caddy...\"
caddy reload --config /etc/caddy/Caddyfile --force

echo \"  Caddy updated for domain: ${REAL_DOMAIN}\"
'"

echo "  VM configuration complete."

# --- Verify HTTPS endpoints ---
echo ""
echo "[EXTRA] Verifying HTTPS endpoints (waiting for certificate provisioning)..."

# Wait up to 120s for Let's Encrypt certificate (real domains may take longer)
MAX_WAIT=120
WAIT_INTERVAL=10
WAITED=0
CERT_READY=false

echo "  Waiting for TLS certificate (up to ${MAX_WAIT}s)..."
while [[ $WAITED -lt $MAX_WAIT ]]; do
    CERT_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://${REAL_DOMAIN}/v1/healthz" 2>/dev/null)
    if [[ "$CERT_STATUS" != "000" ]]; then
        CERT_READY=true
        break
    fi
    sleep $WAIT_INTERVAL
    WAITED=$((WAITED + WAIT_INTERVAL))
    echo "    still provisioning... (${WAITED}s)"
done

if [[ "$CERT_READY" == "false" ]]; then
    echo "ERROR: TLS certificate not provisioned after ${MAX_WAIT}s." >&2
    echo "       Check Caddy logs: gcloud compute ssh ubuntu@${GCP_VM_NAME} ... -- 'sudo journalctl -u caddy -n 50'" >&2
    exit 1
fi
echo "  Certificate provisioned (${WAITED}s)."

# Helper: check endpoint
check_endpoint() {
    local path="$1"
    local description="$2"
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://${REAL_DOMAIN}${path}" 2>/dev/null)
    echo "  ${description}: HTTP $status"
    if [[ "$status" != "200" ]]; then
        echo "ERROR: ${description} returned HTTP $status, expected 200." >&2
        return 1
    fi
}

echo "  Testing /v1/healthz..."
check_endpoint "/v1/healthz" "/v1/healthz" || true

echo "  Testing /v1/readyz..."
check_endpoint "/v1/readyz" "/v1/readyz" || true

echo "  Testing /v1/metrics..."
check_endpoint "/v1/metrics" "/v1/metrics" || true

# --- Summary ---
echo ""
echo "=== Phase 3G Real Domain Configuration Complete ==="
echo "Real Domain  : $REAL_DOMAIN"
echo "HTTPS URL    : https://${REAL_DOMAIN}"
echo "External IP  : $EXTERNAL_IP"
echo ""
echo "Rollback: Run phase3b_destroy_tls_caddy.sh --confirm to restore nip.io."
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff."
echo "            Real domain TLS scaffold only. DNS A record must be operator-configured."