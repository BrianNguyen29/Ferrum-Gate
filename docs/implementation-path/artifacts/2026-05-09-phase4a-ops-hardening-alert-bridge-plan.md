# Phase 4A Ops Hardening and Alert Bridge Artifact

**Date**: 2026-05-09

**Scope**: Phase 4A helper scripts and SendGrid bridge template — backup cadence audit, offsite restore drill, metrics baseline capture, and SendGrid AlertManager bridge template

**Status**: **NON-PROD scaffold evidence only**. Helper scripts are read-only audit/drill tools. SendGrid bridge template uses placeholders only. DuckDNS TLS is current primary (Phase 3J success). NOT production-ready, NOT production alerting, NOT PostgreSQL, NOT HA.

---

## Non-Claims

This Phase 4A artifact does **not** claim:

- production-ready status
- full production posture
- production alerting capability (SendGrid bridge is a template only, not a deployed config)
- real SendGrid API key stored (placeholder only)
- real recipient email committed to repo
- PostgreSQL runtime (SQLite single-node)
- HA/multi-node deployment
- real owned domain (DuckDNS is free DNS)
- Phase 4A operator signoff
- actual SendGrid configuration deployed

Phase 4A documents helper script creation and SendGrid bridge template existence. No live GCP mutations, no secrets, no real email literals.

---

## Overview

Phase 4A adds:

1. **Backup Cadence Audit Helper** (`phase4a_audit_backup_cadence.sh`): Read-only audit of backup timer schedule and GCS artifact age
2. **Offsite Restore Drill Helper** (`phase4a_offsite_restore_drill.sh`): Read-only restore-from-GCS drill with SQLite integrity verification
3. **Metrics Baseline Capture Helper** (`phase4a_capture_metrics_baseline.sh`): Read-only metrics snapshot from running VM
4. **SendGrid AlertManager Bridge Template** (`alertmanager-sendgrid-bridge.example.yaml`): AlertManager webhook→SendGrid API email template with placeholder API key

---

## Helper Scripts

### phase4a_audit_backup_cadence.sh

Read-only audit of backup cadence.

**Location**: `scripts/gcp/phase4a_audit_backup_cadence.sh`

**What it does**:
- Queries `ferrumgate-offsite-backup.timer` schedule via systemd
- Lists last 5 backup objects in configured GCS bucket via `gsutil ls`
- Computes age of most recent backup
- Compares against RPO threshold (default 1 hour)
- Emits sanitized evidence (no secrets, no tokens)

**Syntax check**:
```
bash -n scripts/gcp/phase4a_audit_backup_cadence.sh
→ PASSED (no syntax errors)
```

### phase4a_offsite_restore_drill.sh

Read-only restore drill from GCS offsite backup.

**Location**: `scripts/gcp/phase4a_offsite_restore_drill.sh`

**What it does**:
- Lists latest backup objects in GCS bucket
- Copies most recent backup to temp file (`/tmp/ferrumgate_restore Drill_XXXXXX.db`)
- Runs `PRAGMA integrity_check` on temp file
- Reports TABLE_COUNT and SIZE_BYTES
- Cleans up temp file on success or interrupt

**Syntax check**:
```
bash -n scripts/gcp/phase4a_offsite_restore_drill.sh
→ PASSED (no syntax errors)
```

**Note**: This does NOT overwrite the production database. Restore is to a temp file only.

### phase4a_capture_metrics_baseline.sh

Read-only metrics baseline capture.

**Location**: `scripts/gcp/phase4a_capture_metrics_baseline.sh`

**What it does**:
- Detects current TLS domain from Caddyfile (DuckDNS or nip.io)
- curls `/v1/readyz/deep` and `/v1/metrics` from the running VM
- Saves timestamped baseline to `/tmp/ferrumgate_metrics_baseline_YYYYMMDD_HHMMSS.txt`
- Emits sanitized evidence (no tokens, no secrets)

**Syntax check**:
```
bash -n scripts/gcp/phase4a_capture_metrics_baseline.sh
→ PASSED (no syntax errors)
```

---

## SendGrid AlertManager Bridge Template

### Overview

**Location**: `configs/monitoring/alertmanager-sendgrid-bridge.example.yaml`

The SendGrid bridge template configures AlertManager to send email alerts via the SendGrid Web API v3 instead of SMTP. This is useful when the VM has no direct SMTP access but can reach the SendGrid API over HTTPS (port 443).

### Design

- **Receiver type**: AlertManager `webhook_configs`
- **API endpoint**: `https://api.sendgrid.com/v3/mail/send`
- **Authentication**: Bearer token (API key) stored in `/etc/ferrumgate/secrets/sendgrid-api-key` on the VM
- **Secret handling**: API key is NOT stored in version control; operator creates the secret file on the VM
- **Email routing**: SendGrid personalizations array for per-recipient routing
- **Alert payload**: Uses AlertManager webhook payload with Go template formatting

### Placeholders

| Placeholder | Description | How to fill |
|-------------|-------------|-------------|
| `${SENDGRID_API_KEY_FILE}` | Path to file containing SendGrid API key | Create `/etc/ferrumgate/secrets/sendgrid-api-key` on VM (operator-owned) |
| `${ALERT_RECIPIENT}` | Recipient email address | Operator provides; NOT committed to repo |
| `${ALERT_SENDER}` | Verified sender email in SendGrid | Operator provides; must be a verified sender in SendGrid |

### VM-Local Secret Handling

```
# On the VM, create the secrets directory (operator action):
sudo mkdir -p /etc/ferrumgate/secrets
sudo chmod 700 /etc/ferrumgate/secrets

# Store the API key (operator action — key not in version control):
sudo vi /etc/ferrumgate/secrets/sendgrid-api-key
# Enter only the API key (e.g., SG.xxxxxx.yyyyyy), no JSON, no extra whitespace

# Secure the file:
sudo chmod 600 /etc/ferrumgate/secrets/sendgrid-api-key
sudo chown root:root /etc/ferrumgate/secrets/sendgrid-api-key

# Copy and configure the bridge template (operator action):
sudo cp /path/to/alertmanager-sendgrid-bridge.example.yaml \
       /etc/ferrumgate/monitoring/alertmanager-config.yaml
# Edit the file to replace placeholders

# Reload AlertManager:
curl -X POST http://localhost:9093/-/reload
```

### What Phase 4A does NOT do with SendGrid

- Does NOT create a SendGrid account
- Does NOT store any API key in the repo
- Does NOT configure real recipient email
- Does NOT deploy the bridge to the VM (operator-owned action)
- Does NOT claim production alerting YES

---

## Target Environment (Phase 4A)

| Field | Value | Notes |
|-------|-------|-------|
| Project | `fairy-b13f4` | GCP project |
| Region | `asia-southeast1` | GCP region |
| Zone | `asia-southeast1-a` | GCP zone |
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Static IP | `34.158.51.8` | External IP |
| TLS Domain | `ferrumgate.duckdns.org` | DuckDNS — TLS SUCCESS (Phase 3J) |
| HTTPS URL | `https://ferrumgate.duckdns.org` | Primary endpoint |
| Database | SQLite single-node | Not PostgreSQL |
| Monitoring | Local-only (Prometheus + AlertManager on VM) | Phase 3H deployed |
| Alert Contact | **None** (local-only mode) | SendGrid bridge is a template only |
| GCS Bucket | `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/` | Phase 3H configured |
| SendGrid Bridge | **Not deployed** | Template only; operator-owned deployment |

---

## What Phase 4A is NOT

Phase 4A does NOT:

- Claim production-ready or full production posture
- Include real SendGrid API key or secrets (placeholder only)
- Include real recipient email literals
- Deploy SendGrid bridge (operator-owned action)
- Claim production alerting YES
- Include PostgreSQL or HA
- Modify Rust code
- Make live GCP mutations (scripts are read-only audit/drill helpers)

---

## Remaining Blockers

| Item | Status | Blocker |
|------|--------|---------|
| Production alerting | **BLOCKED** | No alert contact; local-only mode; SendGrid bridge is template only |
| Real owned domain TLS | **BLOCKED** | DuckDNS is free DNS; real owned domain required for production |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |
| PostgreSQL runtime | **BLOCKED** | Path 3 — not in Phase 1 scope |

---

## References

- Phase 3G plan: [101-phase3g-ops-hardening-plan.md](../101-phase3g-ops-hardening-plan.md)
- Phase 3H offsite monitoring: [2026-05-09-gcp-phase3h-offsite-monitoring.md](./2026-05-09-gcp-phase3h-offsite-monitoring.md)
- Phase 3J DuckDNS TLS: [2026-05-09-gcp-phase3j-duckdns-tls-attempt.md](./2026-05-09-gcp-phase3j-duckdns-tls-attempt.md)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Production readiness roadmap: [67-production-readiness-roadmap.md](../67-production-readiness-roadmap.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT real SendGrid API key, NOT real recipient email, NOT PostgreSQL, NOT HA. Phase 4A helper scripts are read-only audit/drill tools. SendGrid bridge is a template only. DuckDuckDNS TLS SUCCESS. Operator-owned deployment required for SendGrid bridge.
