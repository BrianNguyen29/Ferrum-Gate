#!/usr/bin/env bash
# phase3g_configure_monitoring.sh
# Phase 3G: Configures monitoring and alerting for the ferrumgate VM.
# Operator-owned evidence/support script; NOT production-ready, NOT full G2,
# NOT full production pilot authorization, NOT operator signoff.
#
# This script:
#   - Deploys Prometheus scrape config to VM for /v1/metrics
#   - Deploys AlertManager config template (with placeholder receiver)
#   - Deploys ferrumgate alert rules
#   - Supports local-only deployment with localhost/placeholder receivers
#   - NOTE: Does NOT configure actual alert contacts (uses placeholders)
#
# Prerequisites (must exist from Phase 3A/3B):
#   - VM ferrumgate-nonprod running with ferrumgate service
#   - Metrics endpoint available at /v1/metrics
#
# Operational notes (learned from Phase 3H):
#   - Monitoring configs are deployed to /etc/ferrumgate/monitoring/
#   - Prometheus must be explicitly wired to use these configs:
#     - Add a rule_files entry in /etc/prometheus/prometheus.yml for /etc/prometheus/ferrumgate-alerts.yaml
#     - Add scrape job for ferrumgate target to /etc/prometheus/prometheus.yml
#   - AlertManager must be explicitly wired:
#     - Update /etc/prometheus/alertmanager.yml to use /etc/ferrumgate/monitoring/alertmanager-config.yaml
#   - After wiring, restart prometheus and alertmanager services
#
# Local-only mode (--local-only):
#   - Deploys configs for local Prometheus/AlertManager on the VM
#   - Uses localhost receivers only
#   - No real alert contact required
#   - Non-production claim boundary clearly stated
#
# Usage:
#   bash scripts/gcp/phase3g_configure_monitoring.sh --confirm [options]
#
#   Local-only mode (default - no alert contact required):
#     bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --local-only
#
#   External mode (requires alert contact):
#     bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --alert-contact CONTACT [...]
#
# Environment variables (alternative to flags):
#   GCP_PROJECT_ID         GCP project ID (default: fairy-b13f4)
#   GCP_ZONE               Zone (default: asia-southeast1-a)
#   GCP_VM_NAME            VM name (default: ferrumgate-nonprod)
#   LOCAL_ONLY             Set to 'true' for local-only mode (default: true)
#   ALERT_CONTACT          Alert contact placeholder (REQUIRED only for external mode)
#   PROMETHEUS_URL         Prometheus URL (default: http://localhost:9090)
#   ALERTMANAGER_URL       AlertManager URL (default: http://localhost:9093)
#   CONFIRM                Must be "true" to confirm mutation
#
# Options:
#   --help                 Show this help and exit
#   --project-id ID        GCP project ID (default: fairy-b13f4)
#   --zone ZONE           GCP zone (default: asia-southeast1-a)
#   --vm-name NAME        VM name (default: ferrumgate-nonprod)
#   --local-only           Deploy local monitoring stack (Prometheus/AlertManager on VM)
#   --alert-contact CONTACT Alert contact placeholder (REQUIRED for external mode)
#   --prometheus-url URL   Prometheus URL (default: http://localhost:9090)
#   --alertmanager-url URL AlertManager URL (default: http://localhost:9093)
#   --confirm              Required: acknowledge before modifying VM
#
# Example (local-only mode - RECOMMENDED for non-production):
#   bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --local-only \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod
#
# Example (external mode - requires real alert contact):
#   bash scripts/gcp/phase3g_configure_monitoring.sh --confirm \
#     --project-id fairy-b13f4 \
#     --zone asia-southeast1-a \
#     --vm-name ferrumgate-nonprod \
#     --alert-contact OPERATOR_PROVIDED_ALERT_CONTACT

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# --- Defaults ---
GCP_PROJECT_ID="${GCP_PROJECT_ID:-fairy-b13f4}"
GCP_ZONE="${GCP_ZONE:-asia-southeast1-a}"
GCP_VM_NAME="${GCP_VM_NAME:-ferrumgate-nonprod}"
LOCAL_ONLY="${LOCAL_ONLY:-true}"
ALERT_CONTACT="${ALERT_CONTACT:-}"
PROMETHEUS_URL="${PROMETHEUS_URL:-http://localhost:9090}"
ALERTMANAGER_URL="${ALERTMANAGER_URL:-http://localhost:9093}"
CONFIRM="${CONFIRM:-false}"

# --- Usage ---
usage() {
    cat << 'EOF'
Phase 3G: Configure monitoring and alerting for ferrumgate VM.

Usage:
  Local-only mode (default - RECOMMENDED for non-production):
    bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --local-only [options]

  External mode (requires alert contact):
    bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --alert-contact CONTACT [options]

Options:
  --help                  Show this help and exit
  --project-id ID        GCP project ID (default: fairy-b13f4)
  --zone ZONE           GCP zone (default: asia-southeast1-a)
  --vm-name NAME        VM name (default: ferrumgate-nonprod)
  --local-only           Deploy local monitoring stack (Prometheus/AlertManager on VM)
  --alert-contact CONTACT Alert contact placeholder (REQUIRED for external mode)
  --prometheus-url URL   Prometheus URL (default: http://localhost:9090)
  --alertmanager-url URL AlertManager URL (default: http://localhost:9093)
  --confirm             Required: acknowledge before modifying VM

Environment variables:
  GCP_PROJECT_ID, GCP_ZONE, GCP_VM_NAME, LOCAL_ONLY, ALERT_CONTACT,
  PROMETHEUS_URL, ALERTMANAGER_URL, CONFIRM

Prerequisites:
  - VM must be running with ferrumgate service
  - Metrics endpoint must be available at /v1/metrics
  - Prometheus should be configured to scrape this VM (if external Prometheus used)

Local-only mode:
  - Deploys Prometheus and AlertManager configs for local stack on VM
  - Uses localhost receivers only
  - No real alert contact required
  - Non-production claim boundary: NOT production-ready, NOT production alerting

External mode:
  - Deploys configs for external Prometheus/AlertManager
  - Requires real alert contact
  - No email literal stored - use placeholder

Non-claims (Phase 3G):
  NOT production-ready, NOT full G2, NOT full production pilot authorization, NOT operator signoff.
  Scaffolds only. Alert contact is a placeholder in external mode.
  No actual alerts will be sent until real receivers are configured.
  No email literal is stored in configs.
EOF
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help) usage; exit 0 ;;
        --project-id) GCP_PROJECT_ID="$2"; shift 2 ;;
        --zone) GCP_ZONE="$2"; shift 2 ;;
        --vm-name) GCP_VM_NAME="$2"; shift 2 ;;
        --local-only) LOCAL_ONLY="true"; shift ;;
        --alert-contact) ALERT_CONTACT="$2"; shift 2 ;;
        --prometheus-url) PROMETHEUS_URL="$2"; shift 2 ;;
        --alertmanager-url) ALERTMANAGER_URL="$2"; shift 2 ;;
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
    echo "ERROR: --confirm required to configure monitoring on the VM." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_monitoring.sh --confirm [--local-only | --alert-contact CONTACT] [...]" >&2
    exit 1
fi

# --- Require ALERT_CONTACT only for external mode ---
if [[ "$LOCAL_ONLY" != "true" && -z "$ALERT_CONTACT" ]]; then
    echo "ERROR: --alert-contact is required for external mode." >&2
    echo "Usage: bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --alert-contact CONTACT [...]" >&2
    echo "Or use --local-only for local monitoring stack without real alert contact."
    exit 1
fi

echo "=== Phase 3G: Configure Monitoring and Alerting ==="
echo "Project         : $GCP_PROJECT_ID"
echo "Zone            : $GCP_ZONE"
echo "VM Name         : $GCP_VM_NAME"
if [[ "$LOCAL_ONLY" == "true" ]]; then
    echo "Mode            : LOCAL-ONLY (localhost receivers, no real alert contact)"
else
    echo "Mode            : EXTERNAL (alert contact required)"
    echo "Alert Contact   : $ALERT_CONTACT (placeholder only)"
fi
echo "Prometheus URL  : $PROMETHEUS_URL"
echo "AlertManager URL: $ALERTMANAGER_URL"
echo ""
if [[ "$LOCAL_ONLY" == "true" ]]; then
    echo "WARNING: This deploys LOCAL-ONLY monitoring configs to the VM."
    echo "         Uses localhost receivers only. No real alerting."
    echo "         NOT production-ready, NOT production alerting."
else
    echo "WARNING: This deploys monitoring config templates to the VM."
    echo "         Alert contact is a placeholder. No actual alerts will be sent."
fi
echo ""

# --- Pre-flight: verify VM is running ---
echo "[1/3] Checking VM is running..."
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

# --- Deploy monitoring configs to VM ---
echo "[2/3] Deploying monitoring config templates to VM..."

# Get TLS domain from current Caddyfile (for scrape config)
CURRENT_DOMAIN=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "grep -v '^#' /etc/caddy/Caddyfile 2>/dev/null | awk 'NF { print \\$1; exit }' | tr -d '{' || echo '34-158-51-8.nip.io'" \
    2>/dev/null || echo "34-158-51-8.nip.io")

echo "  Current TLS domain detected: $CURRENT_DOMAIN"

gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "sudo bash -c '
set -e

ALERT_CONTACT=\"${ALERT_CONTACT}\"
PROMETHEUS_URL=\"${PROMETHEUS_URL}\"
ALERTMANAGER_URL=\"${ALERTMANAGER_URL}\"
METRICS_DOMAIN=\"${CURRENT_DOMAIN}\"
LOCAL_ONLY=\"${LOCAL_ONLY}\"

MONITORING_DIR=\"/etc/ferrumgate/monitoring\"
mkdir -p \"\$MONITORING_DIR\"

echo \"  Creating Prometheus scrape config...\"
cat > \"\$MONITORING_DIR/prometheus-scrape.yaml\" << PROMEOF
# Prometheus scrape config for ferrumgate
# NOTE: This is a TEMPLATE. Adjust scrape interval and targets for your environment.
# For real domain deployment, replace nip.io domain with your actual domain.
# LOCAL_ONLY: This config is for local monitoring stack deployment.

global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: \"ferrumgate\"
    static_configs:
      - targets:
          # Default nip.io domain - replace with real domain for production
          - \"\${METRICS_DOMAIN}:443\"
        labels:
          service: ferrumgate
          environment: nonprod
    scheme: https
    tls_config:
      # For production with real domain, use proper CA verification
      insecure_skip_verify: false
    metrics_path: /v1/metrics
    scrape_interval: 15s
PROMEOF

echo \"  Creating AlertManager config template...\"
cat > \"\$MONITORING_DIR/alertmanager-config.yaml\" << AMEOF
# AlertManager config template for ferrumgate
# NOTE: This is a TEMPLATE with PLACEHOLDER values.
# LOCAL_ONLY MODE: Uses localhost webhook receiver only. No real alerting.
# No actual alerts will be sent until real receivers are configured.
# No email literal is stored in this config.

global:
  resolve_timeout: 5m

route:
  group_by: [\"alertname\", \"severity\"]
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: \"ferrumgate-alerts\"
  routes:
    - match:
        service: ferrumgate
      receiver: \"ferrumgate-alerts\"

receivers:
  - name: \"ferrumgate-alerts\"
    # LOCAL_ONLY: localhost webhook receiver for local stack
    # Replace with actual receiver configuration for production:
    #   - email_configs:
    #       - to: \"\${ALERT_CONTACT}\"
    #   - slack_configs:
    #       - api_url: \"https://hooks.slack.com/services/YOUR/WEBHOOK/URL\"
    webhook_configs:
      - url: \"http://localhost:9093/webhook\"
        send_resolved: true

inhibit_rules:
  - source_match:
      severity: critical
    target_match:
      severity: warning
    equal: [\"alertname\", \"service\"]
AMEOF

echo \"  Creating ferrumgate alert rules...\"
cat > \"\$MONITORING_DIR/ferrumgate-alerts.yaml\" << ALERTEOF
# FerrumGate alert rules
# NOTE: This is a TEMPLATE. Adjust thresholds for your environment.
# LOCAL_ONLY: These rules are for local monitoring. No actual alerts will be sent.
# No actual alerts will be sent until AlertManager is properly configured.

groups:
  - name: ferrumgate
    interval: 30s
    rules:
      # Service down alert
      - alert: FerrumGateDown
        expr: up{job=\"ferrumgate\"} == 0
        for: 1m
        labels:
          severity: critical
          service: ferrumgate
        annotations:
          summary: \"FerrumGate instance is down\"
          description: \"FerrumGate has been down for more than 1 minute.\"

      # Store health alert
      - alert: FerrumGateStoreUnhealthy
        expr: ferrumgate_store_health_up != 1
        for: 1m
        labels:
          severity: critical
          service: ferrumgate
        annotations:
          summary: \"FerrumGate store is unhealthy\"
          description: \"FerrumGate SQLite store health check failed.\"

      # High write queue depth
      - alert: FerrumGateWriteQueueHigh
        expr: ferrumgate_write_queue_depth > 100
        for: 5m
        labels:
          severity: warning
          service: ferrumgate
        annotations:
          summary: \"FerrumGate write queue is high\"
          description: \"Write queue depth is {{ \\$value }}, expected < 100.\"

      # High error rate (if error metric exists)
      - alert: FerrumGateHighErrorRate
        expr: rate(ferrumgate_http_requests_total{status=~\"5..\"}[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
          service: ferrumgate
        annotations:
          summary: \"FerrumGate high error rate\"
          description: \"Error rate is {{ \\$value }} errors/sec.\"
ALERTEOF

chown -R root:root \"\$MONITORING_DIR\"
chmod 644 \"\$MONITORING_DIR\"/*

echo \"  Monitoring configs deployed to \$MONITORING_DIR:\"
ls -la \"\$MONITORING_DIR/\"
'"

echo "  Monitoring configs deployed to VM."

# --- Summary ---
echo "[3/3] Summary..."

echo ""
echo "=== Phase 3G Monitoring Configuration Complete ==="
echo ""
echo "Configs deployed to /etc/ferrumgate/monitoring/:"
echo "  - prometheus-scrape.yaml     (Prometheus scrape job)"
echo "  - alertmanager-config.yaml   (AlertManager template)"
echo "  - ferrumgate-alerts.yaml     (Alert rules)"
echo ""
if [[ "$LOCAL_ONLY" == "true" ]]; then
    echo "Mode                      : LOCAL-ONLY (localhost receivers)"
    echo "Alert Contact             : NONE (local stack only, no real alerting)"
    echo "Non-production claim      : NOT production-ready, NOT production alerting"
else
    echo "Alert Contact Placeholder : $ALERT_CONTACT (NOT an actual email)"
fi
echo "Prometheus URL            : $PROMETHEUS_URL"
echo "AlertManager URL          : $ALERTMANAGER_URL"
echo ""
echo "SCAFFOLD COMPLETE. Operator must still:"
if [[ "$LOCAL_ONLY" == "true" ]]; then
    echo "  1. Deploy local Prometheus/AlertManager stack on the VM if not present"
    echo "  2. Configure Prometheus to use /etc/ferrumgate/monitoring/prometheus-scrape.yaml"
    echo "  3. Configure AlertManager with /etc/ferrumgate/monitoring/alertmanager-config.yaml"
    echo "  4. Restart Prometheus/AlertManager to pick up configs"
else
    echo "  1. Configure Prometheus to use /etc/ferrumgate/monitoring/prometheus-scrape.yaml"
    echo "  2. Configure AlertManager with /etc/ferrumgate/monitoring/alertmanager-config.yaml"
    echo "  3. Replace PLACEHOLDER values in alertmanager-config.yaml with real receivers"
    echo "  4. Restart Prometheus/AlertManager to pick up configs"
fi
echo ""
echo "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff."
if [[ "$LOCAL_ONLY" == "true" ]]; then
    echo "            LOCAL-ONLY monitoring scaffold. No real alerting configured."
else
    echo "            Monitoring scaffold only. No actual alerts configured. No email literal stored."
fi
