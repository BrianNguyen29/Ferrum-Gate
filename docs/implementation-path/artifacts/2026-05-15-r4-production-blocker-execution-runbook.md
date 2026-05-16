# R4 — Production Blocker Execution Runbook

> **Status**: Operator-owned runbook. NOT production-ready. No live execution claimed.
> **Purpose**: Exact command sequences and rollback procedures for Blocks A (real domain), B (off-VM alerting), and C (backup auth).
> **Scope**: Single-node SQLite v1 conditional pilot. Docs-only; placeholders for all operator inputs.
> **Blocked until**: Operator provides all gated inputs and explicitly runs commands.

---

## Prerequisites

| Prerequisite | Verification Command |
|--------------|---------------------|
| gcloud CLI installed | `gcloud version` |
| Authenticated to GCP project `fairy-b13f4` | `gcloud config get-value project` |
| VM `ferrumgate-nonprod` exists and is running | `gcloud compute instances describe ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4` |
| Static external IP `34.158.51.8` assigned | `gcloud compute instances describe ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 --format='value(networkInterfaces[0].accessConfigs[0].natIP)'` |
| Local clone of repo with scripts | `ls scripts/gcp/phase3g_*.sh` |

---

## Block A — Real Owned Domain

### A.1 Goal
Replace the temporary DuckDNS domain with a real operator-owned domain for TLS.

### A.2 Operator Inputs (All Required)

| Input | Placeholder | Description |
|-------|-------------|-------------|
| Real domain | `REAL_DOMAIN` | Operator-owned domain, e.g., `api.example.com` |
| DNS A record | N/A | Must point `REAL_DOMAIN` → `34.158.51.8` (operator configures externally) |

### A.3 Exact Command Sequence

```bash
# Step 0: Pre-flight — verify DNS A record externally (operator-run from local machine)
dig +short REAL_DOMAIN
# Expected output: 34.158.51.8

# Step 1: Run Phase 3G real-domain configuration script
bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --real-domain REAL_DOMAIN

# Step 2: Verify HTTPS endpoints (script does this automatically; manual check below)
curl -s -o /dev/null -w "%{http_code}" https://REAL_DOMAIN/v1/healthz
# Expected: 200

curl -s -o /dev/null -w "%{http_code}" https://REAL_DOMAIN/v1/readyz
# Expected: 200

curl -s -o /dev/null -w "%{http_code}" https://REAL_DOMAIN/v1/metrics
# Expected: 200

# Step 3: Verify bearer auth through proxy
curl -H "Authorization: Bearer <OPERATOR_BEARER_TOKEN>" \
  -s -o /dev/null -w "%{http_code}" https://REAL_DOMAIN/v1/approvals
# Expected: 200
```

### A.4 Rollback

```bash
# 1. Identify latest Caddyfile backup on VM
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'ls -la /etc/caddy/Caddyfile.backup.* | tail -1'

# 2. Restore backup and reload Caddy
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo cp /etc/caddy/Caddyfile.backup.<TIMESTAMP> /etc/caddy/Caddyfile && \
   sudo caddy reload --config /etc/caddy/Caddyfile --force'

# 3. Verify old endpoint still works (DuckDNS or nip.io)
curl -s -o /dev/null -w "%{http_code}" https://ferrumgate.duckdns.org/v1/healthz
# Expected: 200 (if DuckDNS record still points to 34.158.51.8)
```

### A.5 Evidence Gate

| Gate | Evidence |
|------|----------|
| G-A1 | `curl` output showing HTTPS 200 on `https://REAL_DOMAIN/v1/healthz` |
| G-A2 | `curl` output showing HTTPS 200 on `https://REAL_DOMAIN/v1/approvals` with bearer token |
| G-A3 | DNS A record screenshot or `dig` output showing `REAL_DOMAIN` → `34.158.51.8` |

---

## Block B — Off-VM Alerting

### B.1 Goal
Configure AlertManager to deliver alerts to an off-VM channel (email/SMS/pager/webhook) with confirmed delivery.

### B.2 Operator Inputs (All Required)

| Input | Placeholder | Description |
|-------|-------------|-------------|
| Alert provider | `ALERT_PROVIDER` | SendGrid, SES, PagerDuty, Slack, SMTP relay, etc. |
| Provider API key / token | `PROVIDER_API_KEY` | Stored at `/etc/ferrumgate/secrets/alert-provider-api-key` |
| Primary contact | `PRIMARY_CONTACT` | Email or webhook URL |
| Secondary contact | `SECONDARY_CONTACT` | Escalation email or webhook URL |
| Sender identity | `ALERT_SENDER` | Verified sender (for email providers) |

### B.3 Exact Command Sequence

```bash
# Step 0: Pre-flight — verify AlertManager is running on VM
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'curl -s http://localhost:9093/-/healthy'
# Expected: "OK" or HTTP 200

# Step 1: Copy bridge template to deployed location on VM
gcloud compute scp \
  configs/monitoring/alertmanager-sendgrid-bridge.example.yaml \
  ubuntu@ferrumgate-nonprod:/tmp/alertmanager-config.yaml \
  --zone=asia-southeast1-a --project=fairy-b13f4

# Step 2: Move to monitoring directory and set permissions
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo mkdir -p /etc/ferrumgate/monitoring && \
   sudo mv /tmp/alertmanager-config.yaml /etc/ferrumgate/monitoring/alertmanager-config.yaml && \
   sudo chmod 644 /etc/ferrumgate/monitoring/alertmanager-config.yaml && \
   sudo chown root:root /etc/ferrumgate/monitoring/alertmanager-config.yaml'

# Step 3: Create API key secret on VM
# WARNING: Do NOT commit PROVIDER_API_KEY to git. Run this interactively or via secure channel.
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo mkdir -p /etc/ferrumgate/secrets && \
   sudo chmod 700 /etc/ferrumgate/secrets && \
   sudo sh -c "cat > /etc/ferrumgate/secrets/alert-provider-api-key" && \
   sudo chmod 600 /etc/ferrumgate/secrets/alert-provider-api-key && \
   sudo chown root:root /etc/ferrumgate/secrets/alert-provider-api-key'
# When prompted, paste PROVIDER_API_KEY and press Ctrl+D

# Step 4: Edit config on VM to replace placeholders (operator uses nano/vim on VM)
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo nano /etc/ferrumgate/monitoring/alertmanager-config.yaml'
# Replace:
#   ${SENDGRID_API_KEY_FILE}  → /etc/ferrumgate/secrets/alert-provider-api-key
#   ${ALERT_RECIPIENT}        → PRIMARY_CONTACT
#   ${ALERT_SENDER}           → ALERT_SENDER
# If using a non-SendGrid provider, replace the entire webhook_configs section with the appropriate receiver config.

# Step 5: Validate AlertManager config
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo amtool check-config /etc/ferrumgate/monitoring/alertmanager-config.yaml'
# Expected: "Success"

# Step 6: Reload AlertManager
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'curl -X POST http://localhost:9093/-/reload'

# Step 7: Send test alert
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'amtool alert add alertname=TestAlert severity=critical \
   --alertmanager.url=http://localhost:9093'

# Step 8: Verify delivery
# Operator checks PRIMARY_CONTACT inbox/channel for the test alert.
# If using email, check spam folders.
```

### B.4 Rollback

```bash
# 1. Backup current config (already at /etc/ferrumgate/monitoring/alertmanager-config.yaml)
# 2. Restore local-only config (no external receivers)
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo cp /etc/ferrumgate/monitoring/alertmanager-config.yaml \
           /etc/ferrumgate/monitoring/alertmanager-config.yaml.backup.$(date +%Y%m%d%H%M%S) && \
   sudo sh -c "cat > /etc/ferrumgate/monitoring/alertmanager-config.yaml << 'EOF'
global:
  resolve_timeout: 5m
route:
  group_by: ['alertname']
  receiver: 'default'
receivers:
  - name: 'default'
    # No external notification — log only
EOF" && \
   sudo curl -X POST http://localhost:9093/-/reload'

# 3. Remove API key secret (optional)
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo rm -f /etc/ferrumgate/secrets/alert-provider-api-key'
```

### B.5 Evidence Gate

| Gate | Evidence |
|------|----------|
| G-B1 | Screenshot or log showing test alert delivered to `PRIMARY_CONTACT` |
| G-B2 | Screenshot or log showing test alert delivered to `SECONDARY_CONTACT` |
| G-B3 | `amtool check-config` output showing "Success" |
| G-B4 | `curl -X POST http://localhost:9093/-/reload` returned HTTP 200 |

---

## Block C — Backup Authentication (Keyless vs. Key-Based)

### C.1 Goal
Enable verified offsite backup to GCS with either keyless VM identity (preferred) or accepted key-based auth.

### C.2 Operator Inputs (All Required)

| Input | Placeholder | Description |
|-------|-------------|-------------|
| Path selection | `C1` or `C2` | Operator chooses keyless (C1) or key-based (C2) |
| GCS bucket | `GCS_BUCKET` | e.g., `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/` |
| Service account ID (C2 only) | `OPERATOR_BACKUP_SA_ID` | Short ID for `gcloud iam service-accounts create`, e.g., `ferrumgate-backup` |
| Service account email (C2 only) | `OPERATOR_BACKUP_SA_EMAIL` | Full email, e.g., `OPERATOR_BACKUP_SA_ID@fairy-b13f4.iam.gserviceaccount.com` |

### C.3 Path C1 — Stop-Start VM with GCS Write Scopes (Keyless, Preferred)

**Primary approach**: Stop the VM, update the attached service account scopes using `gcloud compute instances set-service-account`, then start the VM. This avoids delete/recreate and preserves disk, metadata, and network configuration.

**Fallback**: If `set-service-account` is unavailable or fails, recreate the VM from a snapshot (see Fallback C1b below).

```bash
# Step 0: Pre-flight — verify current scopes
gcloud compute instances describe ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --format='value(serviceAccounts.scopes)'
# Expected: devstorage.read_only (and possibly others), but NOT devstorage.read_write

# Step 1: Schedule maintenance window and notify stakeholders

# Step 2: Create a snapshot of the boot disk before any change
gcloud compute disks snapshot $(gcloud compute instances describe ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --format='value(disks[0].source)') \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --snapshot-names=ferrumgate-pre-c1-$(date +%Y%m%d%H%M%S)

# Step 3: Stop VM (DOWNTIME BEGINS)
gcloud compute instances stop ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4

# Step 4: Update service account and scopes (preserves disk, metadata, network)
gcloud compute instances set-service-account ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --scopes=storage-rw,logging-write,monitoring-write

# Step 5: Start VM (DOWNTIME ENDS)
gcloud compute instances start ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4

# Step 6: Wait for ferrumgate service to be ready
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'while ! curl -s -o /dev/null -w "%{http_code}" http://localhost:19080/v1/healthz | grep -q "200"; do echo "waiting..."; sleep 5; done; echo "ready"'

# Step 7: Verify keyless GCS access
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil ls GCS_BUCKET'
# Expected: lists objects or empty bucket (no permission error)

# Step 8: Test backup sync
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil rsync -r /var/lib/ferrumgate/backups/ GCS_BUCKET'
# Expected: completes without error
```

#### Fallback C1b — Recreate VM from Snapshot (if set-service-account fails)

Use this **only** if Step 4 (`set-service-account`) fails or is unavailable for the VM configuration.

```bash
# Delete VM (disk must NOT be auto-deleted; verify first)
gcloud compute instances delete ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet \
  --keep-disks=all

# Recreate VM from boot disk snapshot with proper scopes
# NOTE: Operator must supply original machine-type, network tags, and metadata.
gcloud compute instances create ferrumgate-nonprod \
  --zone=asia-southeast1-a \
  --project=fairy-b13f4 \
  --machine-type=e2-medium \
  --scopes=storage-rw,logging-write,monitoring-write \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --tags=ferrumgate \
  --address=34.158.51.8 \
  --source-snapshot=ferrumgate-pre-c1-<TIMESTAMP> \
  # --metadata=... \
  # Add other flags to match original VM configuration.

# Re-deploy ferrumgate service and Caddy (use existing deployment scripts)
# Verify GCS access and backup sync as in Step 7–8 above.
```

### C.4 Path C2 — Key-Based Backup (Zero Downtime)

```bash
# Step 0: Pre-flight — verify bucket exists
gsutil ls GCS_BUCKET
# Expected: lists objects or empty bucket

# Step 1: Create dedicated backup SA (if not already exists)
# Use OPERATOR_BACKUP_SA_ID (short ID) for create; do NOT use the full email.
gcloud iam service-accounts describe OPERATOR_BACKUP_SA_EMAIL \
  --project=fairy-b13f4 2>/dev/null || \
gcloud iam service-accounts create OPERATOR_BACKUP_SA_ID \
  --display-name="FerrumGate Backup" --project=fairy-b13f4

# Step 2: Grant GCS write permissions
# Use the full email for IAM binding and key commands.
gcloud projects add-iam-policy-binding fairy-b13f4 \
  --member='serviceAccount:OPERATOR_BACKUP_SA_EMAIL' \
  --role='roles/storage.objectAdmin'

# Step 3: Generate and download key (on secure admin host)
gcloud iam service-accounts keys create /tmp/ferrumgate-backup-key.json \
  --iam-account=OPERATOR_BACKUP_SA_EMAIL

# Step 4: Upload key to VM
gcloud compute scp /tmp/ferrumgate-backup-key.json \
  ubuntu@ferrumgate-nonprod:/tmp/ --zone=asia-southeast1-a --project=fairy-b13f4

# Step 5: Secure key on VM
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo mkdir -p /etc/ferrumgate/secrets && \
   sudo mv /tmp/ferrumgate-backup-key.json /etc/ferrumgate/secrets/gcs-service-account.json && \
   sudo chmod 600 /etc/ferrumgate/secrets/gcs-service-account.json && \
   sudo chown root:root /etc/ferrumgate/secrets/gcs-service-account.json'

# Step 6: Activate key for gsutil
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo bash -c "cat /etc/ferrumgate/secrets/gcs-service-account.json | gsutil auth activate-service-account - key_file=-"'

# Step 7: Verify GCS access
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil ls GCS_BUCKET'

# Step 8: Test backup sync
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil rsync -r /var/lib/ferrumgate/backups/ GCS_BUCKET'

# Step 9: Securely delete key from admin host
shred -u /tmp/ferrumgate-backup-key.json
```

### C.5 Rollback

```bash
# Common rollback for both paths:
# 1. Disable backup sync in systemd timer or cron

# Path C1 rollback (stop-start):
# If scopes change caused issues, stop VM and revert to original scopes
# (operator must know the original scope list, e.g., default or compute-rw,devstorage.read_only)
gcloud compute instances stop ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4
gcloud compute instances set-service-account ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --scopes=compute-rw,devstorage.read_only
gcloud compute instances start ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4

# Fallback C1b rollback (if VM was recreated):
# Restore VM from snapshot taken before recreation
gcloud compute instances delete ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet \
  --keep-disks=all
gcloud compute instances create ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --source-snapshot=ferrumgate-pre-c1-<TIMESTAMP> \
  # Add other original flags

# Path C2 rollback:
# 1. Revoke key
gcloud iam service-accounts keys list --iam-account=OPERATOR_BACKUP_SA_EMAIL \
  --project=fairy-b13f4
gcloud iam service-accounts keys delete KEY_ID \
  --iam-account=OPERATOR_BACKUP_SA_EMAIL --project=fairy-b13f4

# 2. Remove key file from VM
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo rm -f /etc/ferrumgate/secrets/gcs-service-account.json'

# 3. Remove gsutil auth
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil auth revoke OPERATOR_BACKUP_SA_EMAIL'
```

### C.6 Evidence Gate

| Gate | Evidence |
|------|----------|
| G-C1 | `gsutil ls GCS_BUCKET` from VM returns success (no 403/scope error) |
| G-C2 | `gsutil rsync` from VM completes without error; object appears in bucket |
| G-C3 | If C2: signed risk acceptance statement (see `R2` artifact) |
| G-C4 | If C2: key file on VM has permissions `600` and is owned by root |

---

## Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This runbook | `67-production-readiness-roadmap.md` | Blocker status and evidence gates |
| This runbook | `R1-alerting-rotation-policy.md` | Alerting rotation and escalation details |
| This runbook | `R2-key-based-backup-risk-acceptance.md` | C1/C2 decision matrix and risk acceptance |
| This runbook | `scripts/gcp/phase3g_configure_real_domain.sh` | Block A script |
| This runbook | `scripts/gcp/phase3g_configure_offsite_backup.sh` | Block C scaffold |
| This runbook | `configs/monitoring/alertmanager-sendgrid-bridge.example.yaml` | Block B template |

---

## Non-Claims

- NOT production-ready
- NOT live domain configured (operator must provide `REAL_DOMAIN`)
- NOT live alerting configured (operator must provide provider + contacts)
- NOT keyless backup working (OAuth scope blocker; C1 or C2 required)
- NOT VM recreated or modified by this document
- All commands use placeholders; operator must replace before execution

---

*Artifact created: 2026-05-15. Production blocker execution runbook — docs-only, no secrets, no live mutation.*
