#!/usr/bin/env bash
# phase3e_sqlite_pilot_evidence.sh
# Phase 3E: SQLite pilot evidence gathering script for GCP non-prod FerrumGate VM.
# Read-only evidence gathering; no GCP config mutations.
#
# This script performs non-destructive read-only health/readiness/auth checks
# and gathers evidence for a conditional single-node SQLite pilot.
#
# READ-ONLY checks (no --confirm required):
#   - HTTPS health/readiness/deep/metrics probes
#   - Service status checks (caddy, ferrumgate.service, ferrumgate-backup.timer)
#   - Firewall rule summary
#   - Backup timer status
#   - Auth probe (401 without token, 200 with VM-local token; token never printed)
#   - Metrics snapshot (store health, write queue)
#   - Backup file listing (read-only)
#
# No destructive actions. No --confirm required for any operation.
#
# Usage:
#   bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh [options]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID     GCP project ID (default: fairy-b13f4)
#   GCP_REGION         Region (default: asia-southeast1)
#   GCP_ZONE           Zone (default: asia-southeast1-a)
#   GCP_VM_NAME        VM name (default: ferrumgate-nonprod)
#   TLS_DOMAIN         TLS domain (default: 34-158-51-8.nip.io)
#   APP_PORT           FerrumGate app port (default: 19080)
#
# Options:
#   --help             Show this help and exit
#   --project-id ID    GCP project ID (default: fairy-b13f4)
#   --region REGION    GCP region (default: asia-southeast1)
#   --zone ZONE        GCP zone (default: asia-southeast1-a)
#   --vm-name NAME     VM name (default: ferrumgate-nonprod)
#   --tls-domain DOMAIN TLS domain (default: 34-158-51-8.nip.io)
#   --app-port PORT    FerrumGate app port (default: 19080)
#
# Examples:
#   # Read-only evidence gathering
#   bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh
#
#   # With explicit GCP parameters
#   bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh \
#     --project-id fairy-b13f4 \
#     --region asia-southeast1 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --tls-domain 34-158-51-8.nip.io

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

# Derived resource names
VPC_NAME="${GCP_VM_NAME}-vpc"
NETWORK_TAG="${GCP_VM_NAME}-app"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3E: SQLite pilot evidence gathering for GCP non-prod FerrumGate VM.

Usage:
  bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh [options]

Options:
  --help                Show this help and exit
  --project-id ID       GCP project ID (default: fairy-b13f4)
  --region REGION       GCP region (default: asia-southeast1)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --tls-domain DOMAIN   TLS domain (default: 34-158-51-8.nip.io)
  --app-port PORT       FerrumGate app port (default: 19080)

Read-only checks performed (no --confirm required):
  - HTTPS /v1/healthz, /v1/readyz, /v1/readyz/deep, /v1/metrics
  - Auth probe: 401 without token, 200 with token
  - Service status: caddy, ferrumgate.service, ferrumgate-backup.timer
  - Firewall rule summary
  - Backup timer status
  - Metrics snapshot (store health, write queue depth)
  - Backup file listing (read-only)

Non-claims:
  NOT production-ready, NOT full G2 beyond conditional signed scope,
  NOT full production pilot authorization, NOT Phase 3E operator signoff.
  Evidence gathering for the signed conditional single-node SQLite pilot only.
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
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# --- Validate gcloud availability ---
if ! command -v gcloud &>/dev/null; then
    echo "ERROR: gcloud CLI not found. Install Google Cloud SDK." >&2
    exit 1
fi

echo "=== Phase 3E: SQLite Pilot Evidence Gathering ==="
echo "Project   : $GCP_PROJECT_ID"
echo "Region    : $GCP_REGION"
echo "Zone      : $GCP_ZONE"
echo "VM Name   : $GCP_VM_NAME"
echo "TLS Domain: $TLS_DOMAIN"
echo "App Port  : $APP_PORT"
echo ""
echo "Scope: Signed conditional single-node SQLite pilot evidence only."
echo "       NOT production-ready, NOT full G2/full production pilot authorization."
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/7] Checking VM is running..."
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

# --- Retrieve bearer token prefix (8 chars only, no full token) ---
echo "[2/7] Checking VM-local bearer token availability without printing token..."

TOKEN_PREFIX=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo test -s /etc/ferrumgate/ferrumgate_initial_token && printf PRESENT" \
    2>/dev/null || echo "FAILED")

if [[ "$TOKEN_PREFIX" == "FAILED" || -z "$TOKEN_PREFIX" ]]; then
    echo "  WARNING: Could not confirm VM-local token availability. Auth probes will be skipped."
    TOKEN_PREFIX=""
else
    echo "  VM-local token: PRESENT (value not printed)"
fi

# Track failed checks for fail-closed behavior
FAILED_CHECKS=0

# --- HTTPS health/readiness/deep/metrics probes ---
echo "[3/7] Probing HTTPS endpoints..."

check_https() {
    local path="$1"
    local description="$2"
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        "https://${TLS_DOMAIN}${path}" 2>/dev/null || echo "000")
    echo "  ${description}: HTTP $status"
    if [[ "$status" == "200" ]]; then
        return 0
    else
        echo "  ERROR: ${description} returned HTTP $status, expected 200."
        return 1
    fi
}

check_https "/v1/healthz" "/v1/healthz" || (( FAILED_CHECKS++ ))
check_https "/v1/readyz" "/v1/readyz" || (( FAILED_CHECKS++ ))
check_https "/v1/readyz/deep" "/v1/readyz/deep" || (( FAILED_CHECKS++ ))
check_https "/v1/metrics" "/v1/metrics" || (( FAILED_CHECKS++ ))

# --- Metrics snapshot (store health, write queue) ---
echo "[4/7] Gathering metrics snapshot..."

METRICS_OUTPUT=$(curl -s "https://${TLS_DOMAIN}/v1/metrics" 2>/dev/null || echo "")

if [[ -n "$METRICS_OUTPUT" ]]; then
    STORE_HEALTH=$(echo "$METRICS_OUTPUT" | grep "^ferrumgate_store_health_up " | awk '{print $2}' || echo "N/A")
    WRITE_QUEUE=$(echo "$METRICS_OUTPUT" | grep "^ferrumgate_write_queue_depth " | awk '{print $2}' || echo "N/A")
    echo "  ferrumgate_store_health_up: ${STORE_HEALTH}"
    echo "  ferrumgate_write_queue_depth: ${WRITE_QUEUE}"

    if [[ "$STORE_HEALTH" != "1" ]]; then
        echo "  ERROR: Store health is ${STORE_HEALTH}, expected 1."
        (( FAILED_CHECKS++ ))
    fi
else
    echo "  ERROR: Could not retrieve metrics."
    (( FAILED_CHECKS++ ))
fi

# --- Auth probes (401 without token, 200 with token) ---
echo "[5/7] Probing auth endpoints..."

AUTH_401_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    "https://${TLS_DOMAIN}/v1/approvals" 2>/dev/null || echo "000")
echo "  GET /v1/approvals without token: HTTP $AUTH_401_STATUS (expected: 401)"

if [[ "$AUTH_401_STATUS" != "401" ]]; then
    echo "  ERROR: Auth probe without token returned HTTP $AUTH_401_STATUS, expected 401."
    (( FAILED_CHECKS++ ))
fi

if [[ -n "$TOKEN_PREFIX" ]]; then
    # Fetch full token for auth probe (only used in-memory for curl, never printed)
    FULL_TOKEN=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo cat /etc/ferrumgate/ferrumgate_initial_token 2>/dev/null" \
        2>/dev/null || echo "")

    if [[ -n "$FULL_TOKEN" ]]; then
        AUTH_200_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
            -H "Authorization: Bearer ${FULL_TOKEN}" \
            "https://${TLS_DOMAIN}/v1/approvals" 2>/dev/null || echo "000")
        echo "  GET /v1/approvals with VM-local token: HTTP $AUTH_200_STATUS (expected: 200)"
        if [[ "$AUTH_200_STATUS" != "200" ]]; then
            echo "  ERROR: Auth probe with token returned HTTP $AUTH_200_STATUS, expected 200."
            (( FAILED_CHECKS++ ))
        fi
    else
        echo "  ERROR: Could not retrieve full token for auth probe."
        (( FAILED_CHECKS++ ))
    fi
else
    echo "  ERROR: Skipping authenticated probe (token prefix unavailable)."
    (( FAILED_CHECKS++ ))
fi

# --- Service status checks ---
echo "[6/7] Checking service statuses on VM..."

# Read each status on its own line (newline-separated output)
mapfile -t SERVICE_STATUSES < <(
    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "sudo systemctl is-active caddy 2>/dev/null || echo 'inactive';
         sudo systemctl is-active ferrumgate.service 2>/dev/null || echo 'inactive';
         sudo systemctl is-enabled ferrumgate-backup.timer 2>/dev/null || echo 'disabled'" \
        2>/dev/null || echo "inactive\ninactive\ndisabled"
)

CADDY_STATUS="${SERVICE_STATUSES[0]:-inactive}"
FERRUM_STATUS="${SERVICE_STATUSES[1]:-inactive}"
BACKUP_TIMER_STATUS="${SERVICE_STATUSES[2]:-disabled}"

echo "  caddy.service:                ${CADDY_STATUS} (expected: active)"
echo "  ferrumgate.service:           ${FERRUM_STATUS} (expected: active)"
echo "  ferrumgate-backup.timer:      ${BACKUP_TIMER_STATUS} (expected: enabled)"

# Fail-closed on service statuses
if [[ "$CADDY_STATUS" != "active" ]]; then
    echo "  ERROR: caddy.service is '${CADDY_STATUS}', expected 'active'."
    (( FAILED_CHECKS++ ))
fi
if [[ "$FERRUM_STATUS" != "active" ]]; then
    echo "  ERROR: ferrumgate.service is '${FERRUM_STATUS}', expected 'active'."
    (( FAILED_CHECKS++ ))
fi
if [[ "$BACKUP_TIMER_STATUS" != "enabled" ]]; then
    echo "  ERROR: ferrumgate-backup.timer is '${BACKUP_TIMER_STATUS}', expected 'enabled'."
    (( FAILED_CHECKS++ ))
fi

# --- Backup file listing (read-only) ---
echo "[7/7] Listing backup files (read-only)..."

BACKUP_FILES=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "ls -t /var/lib/ferrumgate/backups/ 2>/dev/null | head -5" \
    2>/dev/null || echo "(backup directory unavailable)")

if [[ "$BACKUP_FILES" == "(backup directory unavailable)" ]]; then
    echo "  ERROR: Could not list backup files."
    (( FAILED_CHECKS++ ))
else
    echo "  Latest backup files:"
    echo "$BACKUP_FILES" | while read -r line; do
        echo "    $line"
    done
    if [[ -z "$BACKUP_FILES" ]]; then
        echo "  WARNING: No backup files found."
    fi
fi

# --- Firewall rule summary ---
echo ""
echo "[EXTRA] Fetching firewall rule summary..."

echo "  Firewall rules for ${GCP_VM_NAME}:"
gcloud compute firewall-rules list \
    --project="$GCP_PROJECT_ID" \
    --filter="network:${VPC_NAME}" \
    --format="table(name,allowed[].map().firewall_rule().list(),sourceRanges.list().list())" \
    2>/dev/null | while read -r line; do
    echo "    $line"
done || echo "  (Could not list firewall rules)"

# --- Backup timer next run ---
echo ""
echo "[EXTRA] Checking backup timer next run..."

BACKUP_TIMER_NEXT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo systemctl list-timers --no-pager 2>/dev/null | grep ferrumgate-backup.timer" \
    2>/dev/null || echo "(timer status unavailable)")

echo "  ${BACKUP_TIMER_NEXT}"

# --- Summary ---
echo ""
echo "=== Phase 3E SQLite Pilot Evidence Gathering Complete ==="
echo ""
echo "TLS Domain:     https://${TLS_DOMAIN}"
echo "External IP:    ${EXTERNAL_IP}"
echo "Database:       SQLite single-node (no PostgreSQL)"
echo ""
if [[ "$FAILED_CHECKS" -gt 0 ]]; then
    echo "FAILED: ${FAILED_CHECKS} check(s) failed."
    echo ""
    echo "Non-claims: NOT production-ready, NOT full G2/full production pilot authorization, NOT Phase 3E operator signoff."
    echo "            Signed conditional single-node SQLite pilot evidence only."
    exit 1
fi
echo "PASSED: All evidence checks succeeded."
echo ""
echo "Non-claims: NOT production-ready, NOT full G2/full production pilot authorization, NOT Phase 3E operator signoff."
echo "            Signed conditional single-node SQLite pilot evidence only."
echo "            No GCP config mutations performed."
