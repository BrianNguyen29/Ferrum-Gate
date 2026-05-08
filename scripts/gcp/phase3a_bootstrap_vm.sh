#!/usr/bin/env bash
# phase3a_bootstrap_vm.sh
# Phase 3A: Bootstraps a GCP non-prod VM for FerrumGate target rehearsal.
# Run this script ON the VM via SSH after VM is created and binaries are deployed.
# Operator-owned evidence/support script; NOT production-ready, NOT G2 complete,
# NOT pilot authorized, NOT operator signoff.
#
# This script (runs on VM):
#   - Creates 'ferrumgate' system user and directories
#   - Creates config/env template with bearer auth and store path
#   - Creates systemd service for ferrumd (binds 0.0.0.0:19080)
#   - Creates backup service and timer (hourly backup to /var/lib/ferrumgate/backups)
#   - Creates evidence and logs directories
#   - If binaries are missing, creates placeholder scripts (for rehearsal only)
#   - If bearer token is missing, generates one on VM and prints only prefix (never full token)
#   - SQLite store path: /var/lib/ferrumgate/data/ferrumgate.db
#
# Usage (run on VM via SSH):
#   ssh -o StrictHostKeyChecking=no ubuntu@<VM_IP> 'bash -s' < scripts/gcp/phase3a_bootstrap_vm.sh
#   or
#   gcloud compute ssh ubuntu@<VM_NAME> --zone=<ZONE> --project=<PROJECT_ID> -- < scripts/gcp/phase3a_bootstrap_vm.sh
#
# Environment variables (set locally before SSH call, or inside VM):
#   FERRUM_BEARER_TOKEN     Bearer token for ferrumd auth (generated if missing)
#   FERRUM_STORE_DSN       Store DSN (default: sqlite:///var/lib/ferrumgate/data/ferrumgate.db)
#   FERRUM_APP_PORT        App port (default: 19080)
#   FERRUM_BIND_ADDR       Bind address (default: 0.0.0.0)
#   FERRUM_VERSION         Version tag (default: placeholder, set by deploy script)

set -euo pipefail

# --- Config (can be overridden before running on VM) ---
FERRUM_APP_PORT="${FERRUM_APP_PORT:-19080}"
FERRUM_BIND_ADDR="${FERRUM_BIND_ADDR:-0.0.0.0}"
FERRUM_STORE_DSN="${FERRUM_STORE_DSN:-sqlite:///var/lib/ferrumgate/data/ferrumgate.db?mode=rwc}"
FERRUM_VERSION="${FERRUM_VERSION:-placeholder}"

# Paths
INSTALL_DIR="/opt/ferrumgate"
DATA_DIR="/var/lib/ferrumgate"
BACKUP_DIR="$DATA_DIR/backups"
LOG_DIR="/var/log/ferrumgate"
CONFIG_DIR="/etc/ferrumgate"
SERVICE_USER="ferrumgate"

# Binary names
FERRUMD_BIN="$INSTALL_DIR/ferrumd"
FERRUMCTL_BIN="$INSTALL_DIR/ferrumctl"
BOOTSTRAP_LOG="/tmp/ferrumgate_bootstrap.log"

# --- Log helper ---
log() {
    echo "[$(date '+%Y-%m-%dT%H:%M:%S%z')] $*"
}

log "=== Phase 3A: Bootstrap VM for FerrumGate ==="
log "App Port   : $FERRUM_APP_PORT"
log "Bind Addr  : $FERRUM_BIND_ADDR"
log "Store DSN  : $FERRUM_STORE_DSN"
log "Version    : $FERRUM_VERSION"
log ""

# --- Check if running as root ---
if [[ $EUID -ne 0 ]]; then
    log "ERROR: Bootstrap must be run as root (or with sudo)."
    exit 1
fi

# --- Detect if binaries exist ---
FERRUMD_EXISTS=false
FERRUMCTL_EXISTS=false
if [[ -x "$FERRUMD_BIN" ]]; then
    FERRUMD_EXISTS=true
    log "ferrumd binary found at $FERRUMD_BIN"
else
    log "WARNING: ferrumd binary not found at $FERRUMD_BIN (placeholder will be created)"
fi
if [[ -x "$FERRUMCTL_BIN" ]]; then
    FERRUMCTL_EXISTS=true
    log "ferrumctl binary found at $FERRUMCTL_BIN"
else
    log "WARNING: ferrumctl binary not found at $FERRUMCTL_BIN (placeholder will be created)"
fi

# --- Create system user ---
log "[1/8] Creating system user '$SERVICE_USER'..."
if id "$SERVICE_USER" &>/dev/null; then
    log "  User '$SERVICE_USER' already exists."
else
    useradd --system --no-create-home --shell=/usr/sbin/nologin "$SERVICE_USER"
    log "  User '$SERVICE_USER' created."
fi

# --- Create directories ---
log "[2/9] Creating directories..."
for dir in "$INSTALL_DIR" "$DATA_DIR" "$DATA_DIR/data" "$BACKUP_DIR" "$LOG_DIR" "$CONFIG_DIR"; do
    if [[ ! -d "$dir" ]]; then
        mkdir -p "$dir"
        log "  Created: $dir"
    else
        log "  Exists: $dir"
    fi
done

# --- Set ownership ---
chown -R "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR" "$LOG_DIR"

# --- Generate bearer token if missing (after CONFIG_DIR exists) ---
log "[3/9] Generating bearer token..."
if [[ -z "${FERRUM_BEARER_TOKEN:-}" ]]; then
    log "  FERRUM_BEARER_TOKEN not set. Generating one on VM..."
    FERRUM_BEARER_TOKEN=$(openssl rand -hex 32)
    TOKEN_PREFIX="${FERRUM_BEARER_TOKEN:0:8}"
    log "  Generated token with prefix: ${TOKEN_PREFIX}..."
    log "  IMPORTANT: Save the full token securely. It will not be shown again."
    # Write to a file readable only by root (operator must retrieve via sudo)
    echo "$FERRUM_BEARER_TOKEN" > "$CONFIG_DIR/ferrumgate_initial_token"
    chmod 600 "$CONFIG_DIR/ferrumgate_initial_token"
    log "  Token written to $CONFIG_DIR/ferrumgate_initial_token (root-only)"
else
    TOKEN_PREFIX="${FERRUM_BEARER_TOKEN:0:8}"
    log "  Using provided token (prefix: ${TOKEN_PREFIX}...)"
fi

# --- Create env config file ---
log "[4/9] Creating environment config at $CONFIG_DIR/env..."
FERRUM_AUTH_MODE="bearer"
# FERRUMD_BIND_ADDR must be a full SocketAddr: port
FERRUM_BIND_FULL="0.0.0.0:${FERRUM_APP_PORT}"
cat > "$CONFIG_DIR/env" << EOF
# FerrumGate Phase 3A non-prod target config (generated by bootstrap)
# DO NOT commit this file to version control
FERRUMD_BIND_ADDR=${FERRUM_BIND_FULL}
FERRUMD_STORE_DSN=${FERRUM_STORE_DSN}
FERRUMD_AUTH_MODE=${FERRUM_AUTH_MODE}
FERRUMD_BEARER_TOKEN=${FERRUM_BEARER_TOKEN}
FERRUMD_STORE_SYNCHRONOUS=true
FERRUMD_STORE_WAL_AUTOCHECKPOINT=1000
FERRUMD_LOG_FILTER=info
EOF
chmod 600 "$CONFIG_DIR/env"
chown "$SERVICE_USER:$SERVICE_USER" "$CONFIG_DIR/env"
log "  Config written (mode 600, owned by $SERVICE_USER)"

# --- Create placeholder binaries if missing ---
if [[ "$FERRUMD_EXISTS" == "false" ]]; then
    log "[5/9] Creating ferrumd placeholder..."
    cat > "$FERRUMD_BIN" << 'EOF'
#!/usr/bin/env bash
# Placeholder ferrumd binary for Phase 3A rehearsal
# Replace with real binary via phase3a_deploy_binaries.sh
echo "ferrumd placeholder: real binary not yet deployed"
exit 1
EOF
    chmod +x "$FERRUMD_BIN"
    chown "$SERVICE_USER:$SERVICE_USER" "$FERRUMD_BIN"
fi

if [[ "$FERRUMCTL_EXISTS" == "false" ]]; then
    log "[6/9] Creating ferrumctl placeholder..."
    cat > "$FERRUMCTL_BIN" << 'EOF'
#!/usr/bin/env bash
# Placeholder ferrumctl binary for Phase 3A rehearsal
# Replace with real binary via phase3a_deploy_binaries.sh
echo "ferrumctl placeholder: real binary not yet deployed"
exit 1
EOF
    chmod +x "$FERRUMCTL_BIN"
    chown "$SERVICE_USER:$SERVICE_USER" "$FERRUMCTL_BIN"
fi

# --- Create systemd service ---
log "[7/9] Creating systemd service..."
cat > /etc/systemd/system/ferrumgate.service << EOF
[Unit]
Description=FerrumGate ferrumd Phase 3A non-prod
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
Group=${SERVICE_USER}
EnvironmentFile=${CONFIG_DIR}/env
ExecStart=${FERRUMD_BIN} --config ${CONFIG_DIR}/ferrumgate.toml
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR} ${LOG_DIR}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

# Create a minimal toml config that defers to env file
cat > "$CONFIG_DIR/ferrumgate.toml" << 'EOF'
# FerrumGate Phase 3A minimal config (env vars take precedence via EnvironmentFile)
[server]
bind_addr = "0.0.0.0:19080"

[store]
synchronous = true
wal_autocheckpoint = 1000

[auth]
mode = "bearer"
EOF

chmod 644 /etc/systemd/system/ferrumgate.service
systemctl daemon-reload
log "  Service installed: ferrumgate.service"

# --- Create backup service and timer ---
log "[8/9] Creating backup service and timer..."

# Ensure sqlite3 is available for safe SQLite backup (WAL-safe .backup API)
if ! command -v sqlite3 &>/dev/null; then
    log "  sqlite3 not found; installing..."
    apt-get update -qq && apt-get install -y -qq sqlite3 >/dev/null 2>&1
    log "  sqlite3 installed."
fi

cat > /etc/systemd/system/ferrumgate-backup.service << EOF
[Unit]
Description=FerrumGate SQLite backup (Phase 3A non-prod)
Requires=ferrumgate.service

[Service]
Type=oneshot
User=${SERVICE_USER}
Group=${SERVICE_USER}
EnvironmentFile=${CONFIG_DIR}/env
ExecStart=/bin/bash -c '\
  BACKUP_FILE="${BACKUP_DIR}/ferrumgate_$(date +%Y%m%d_%H%M%S).db"; \
  if [[ -f "${DATA_DIR}/data/ferrumgate.db" ]]; then \
    if command -v sqlite3 &>/dev/null; then \
      sqlite3 "${DATA_DIR}/data/ferrumgate.db" ".backup ""\$BACKUP_FILE"""; \
    else \
      echo "WARNING: sqlite3 unavailable, using plain cp (unsafe with WAL)"; \
      cp "${DATA_DIR}/data/ferrumgate.db" "\$BACKUP_FILE"; \
    fi; \
    chmod 600 "\$BACKUP_FILE"; \
    echo "Backup: \$BACKUP_FILE"; \
  else \
    echo "No DB file to backup"; \
  fi'
StandardOutput=journal
StandardError=journal
EOF

cat > /etc/systemd/system/ferrumgate-backup.timer << EOF
[Unit]
Description=FerrumGate hourly backup timer (Phase 3A non-prod)
Requires=ferrumgate-backup.service

[Timer]
OnBootSec=5min
OnUnitActiveSec=1h
Persistent=true

[Install]
WantedBy=timers.target
EOF

chmod 644 /etc/systemd/system/ferrumgate-backup.service
chmod 644 /etc/systemd/system/ferrumgate-backup.timer
systemctl daemon-reload
log "  Backup service and timer installed"

# --- Enable and start services ---
log "[9/9] Enabling and starting services..."
systemctl enable ferrumgate.service
systemctl enable ferrumgate-backup.timer

# Start ferrumgate service
if systemctl is-active --quiet ferrumgate.service; then
    log "  ferrumgate.service is already running (restarting to pick up changes)..."
    systemctl restart ferrumgate.service
else
    log "  Starting ferrumgate.service..."
    systemctl start ferrumgate.service
fi

systemctl enable ferrumgate-backup.timer
systemctl start ferrumgate-backup.timer

sleep 2

if systemctl is-active --quiet ferrumgate.service; then
    log "  ferrumgate.service is active."
else
    log "  WARNING: ferrumgate.service is not active. Check journalctl -u ferrumgate.service"
fi

# --- Summary ---
log ""
log "=== Bootstrap Complete ==="
log "ferrumgate service  : $(systemctl is-active ferrumgate.service 2>/dev/null || echo 'inactive')"
log "ferrumgate enabled   : $(systemctl is-enabled ferrumgate.service 2>/dev/null || echo 'disabled')"
log "backup timer enabled : $(systemctl is-enabled ferrumgate-backup.timer 2>/dev/null || echo 'disabled')"
log ""
log "Data dir      : $DATA_DIR"
log "Backup dir    : $BACKUP_DIR"
log "Log dir       : $LOG_DIR"
log "Config dir    : $CONFIG_DIR"
log "Service       : $CONFIG_DIR/ferrumgate.service (or systemd)"
log "Token prefix  : ${TOKEN_PREFIX}..."
log "Token file    : $CONFIG_DIR/ferrumgate_initial_token (root-only)"
log ""
log "Test endpoint once ferrumd is deployed:"
log "  curl -H 'Authorization: Bearer <token>' http://localhost:${FERRUM_APP_PORT}/v1/healthz"
log ""
log "Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized."
