# 102 — Phase 4A Ops Hardening and Alert Bridge Plan

## Overview

Phase 4A creates ops hardening helpers and a SendGrid AlertManager bridge template for the nonprod VM. This phase adds operational runbook scripts for backup cadence audit, offsite restore drill, metrics baseline capture, and an AlertManager→SendGrid email bridge template. All are **scaffolds only** — no live SendGrid API key is stored or configured.

**This document does NOT claim production-ready, full production posture, production alerting, or PostgreSQL runtime.**

---

## Non-Claims (Phase 4A)

> **IMPORTANT**: Phase 4A carries the following explicit non-claims:
> - NOT production-ready
> - NOT full production posture
> - NOT production alerting YES (local-only or placeholder bridge only)
> - NOT real SendGrid API key stored (placeholder only)
> - NOT real recipient email in committed docs/configs
> - NOT PostgreSQL runtime (SQLite single-node only)
> - NOT HA/multi-node
> - DuckDNS is current endpoint (nip.io superseded by Phase 3J retry success)
> - Real owned domain remains NO
> - Scripts are local-only evidence helpers; do not mutate GCP state without explicit --confirm

---

## Scope

### What Phase 4A IS

- Repo-side helper scripts for ops runbook automation (backup cadence audit, restore drill, metrics baseline)
- SendGrid AlertManager webhook bridge template with placeholder API key
- VM-local secret handling documentation (API key stored in `/etc/ferrumgate/secrets/`, not in version control)
- README index update for Phase 4A entries

### What Phase 4A is NOT

- No live GCP mutations (scripts are read-only audit/drill helpers)
- No real SendGrid API key or secrets in version control
- No production alerting claim
- No PostgreSQL or HA
- No real recipient email literals in configs/docs

---

## Phase 4A Components

### 1. Backup Cadence Audit Helper

**Script**: `scripts/gcp/phase4a_audit_backup_cadence.sh`

Audits the backup timer schedule and recent backup artifacts on the VM. Read-only — does not mutate state.

**What it checks**:
- Backup timer schedule (`ferrumgate-offsite-backup.timer`)
- Last 5 backup objects in GCS bucket
- Backup artifact age and size
- Whether last backup is within expected RPO window

**Blocked until**: Phase 3H GCS offsite backup deployed

### 2. Offsite Restore Drill Helper

**Script**: `scripts/gcp/phase4a_offsite_restore_drill.sh`

Executes a restore drill from GCS offsite backup to a temp file on VM and verifies SQLite integrity. Read-only evidence gathering — does not overwrite production data.

**What it does**:
- Lists latest backup objects in GCS bucket
- Copies latest backup to temp file
- Runs `PRAGMA integrity_check` on restored database
- Reports TABLE_COUNT and SIZE_BYTES
- Cleans up temp file

**Blocked until**: Phase 3H GCS offsite backup deployed

### 3. Metrics Baseline Capture Helper

**Script**: `scripts/gcp/phase4a_capture_metrics_baseline.sh`

Captures a metrics baseline snapshot from the running VM. Read-only — curls /v1/metrics and /v1/readyz/deep and emits sanitized evidence.

**What it captures**:
- `/v1/metrics` output snapshot
- `/v1/readyz/deep` response
- Current timestamp
- Scrape URL in use (DuckDNS or nip.io)

**Blocked until**: Phase 3J DuckDNS TLS working (or nip.io fallback)

### 4. SendGrid AlertManager Bridge Template

**Config**: `configs/monitoring/alertmanager-sendgrid-bridge.example.yaml`

AlertManager webhook receiver template that forwards alerts to SendGrid email API. Uses placeholder API key and recipient.

**VM-local secret handling**:
- SendGrid API key must be stored at `/etc/ferrumgate/secrets/sendgrid-api-key` on the VM (not in version control)
- Operator copies `alertmanager-sendgrid-bridge.example.yaml` to `/etc/ferrumgate/monitoring/alertmanager-config.yaml` and replaces placeholders
- Alternatively, use environment variable `SENDGRID_API_KEY` in systemd service context

**Inputs required**:
- SendGrid API key (operator provides, stored VM-locally)
- Alert recipient email (operator provides, NOT committed to repo)
- Alert sender email (operator provides)

**Blocked until**: Operator provides SendGrid API key and recipient email; AlertManager deployed (Phase 3H)

---

## Inputs Summary

### Backup Cadence Audit

| Input | Status | Description |
|-------|--------|-------------|
| GCS bucket name | **REQUIRED** | Same bucket used in Phase 3H |
| VM access | **REQUIRED** | gcloud SSH access to ferrumgate-nonprod |

### Offsite Restore Drill

| Input | Status | Description |
|-------|--------|-------------|
| GCS bucket name | **REQUIRED** | Same bucket used in Phase 3H |
| VM access | **REQUIRED** | gcloud SSH access to ferrumgate-nonprod |

### Metrics Baseline

| Input | Status | Description |
|-------|--------|-------------|
| Metrics URL | **OPTIONAL** | Auto-detected from Caddyfile (DuckDNS or nip.io) |
| VM access | **REQUIRED** | gcloud SSH access to ferrumgate-nonprod |

### SendGrid Bridge

| Input | Status | Description |
|-------|--------|-------------|
| SendGrid API key | **REQUIRED (operator)** | Stored at `/etc/ferrumgate/secrets/sendgrid-api-key` — NOT in repo |
| Alert recipient | **REQUIRED (operator)** | Stored in deployed config only — NOT in repo |
| Alert sender | **REQUIRED (operator)** | Verified sender in SendGrid |
| AlertManager deployed | **REQUIRED** | Phase 3H local AlertManager |

---

## Current Environment (DuckDNS)

| Field | Value | Notes |
|-------|-------|-------|
| Project | `fairy-b13f4` | GCP project |
| Region | `asia-southeast1` | GCP region |
| Zone | `asia-southeast1-a` | GCP zone |
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Static IP | `34.158.51.8` | External IP |
| TLS Domain | `ferrumgate.duckdns.org` | DuckDNS free DNS — **TLS SUCCESS** (Phase 3J retry) |
| HTTPS URL | `https://ferrumgate.duckdns.org` | Primary endpoint |
| Database | SQLite single-node | Not PostgreSQL |
| Monitoring | Local-only (Prometheus + AlertManager on VM) | Phase 3H deployed |
| Alert Contact | **None** | Local-only mode |
| GCS Bucket | `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/` | Phase 3H configured |

---

## Live Execution Notes

### Helper Script Bugs Found and Fixed During Live Execution

Three bugs were discovered and fixed during live execution (documented in `artifacts/2026-05-10-phase4a-live-hardening-evidence.md`):

1. **URL parsing bug** (`phase4a_audit_backup_cadence.sh`): `gsutil ls -l` output format is `SIZE DATE gs://URL` (URL not at line start). Fixed to `grep` for the line containing `gs://` first, then extract fields from that line.

2. **Age computation bug** (`phase4a_audit_backup_cadence.sh`): RPO age initially used filename timestamp. Fixed to use GCS mtime from `gsutil ls -l` field 2.

3. **Multi-line COPY_SUCCESS bug** (`phase4a_offsite_restore_drill.sh`): `gsutil cp` outputs multi-line logs. Fixed to check for `COPY_SUCCESS` anywhere in output using `grep -q` instead of relying on final-line position. Also added cleanup on copy failure.

---

## Script Usage Patterns

### Backup Cadence Audit (read-only)

```bash
bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
```

### Offsite Restore Drill (read-only evidence)

```bash
bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
```

### Metrics Baseline Capture (read-only)

```bash
bash scripts/gcp/phase4a_capture_metrics_baseline.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod
```

### SendGrid Bridge Deployment (operator-owned, not automated)

```bash
# 1. Copy template to deployed location
# 2. Replace placeholders with operator-provided values
# 3. Store API key at /etc/ferrumgate/secrets/sendgrid-api-key
# 4. Reload AlertManager: curl -X POST http://localhost:9093/-/reload
```

---

## SendGrid Bridge Template Overview

The `alertmanager-sendgrid-bridge.example.yaml` configures AlertManager to send email via SendGrid REST API instead of SMTP.

**Key design points**:
- Uses AlertManager `webhook_configs` receiver pointing to SendGrid API
- API key read from VM-local secret file (not env var in config, not in version control)
- Template uses SendGrid personalizations for per-recipient routing
- Supports `send_resolved: true` to notify on alert resolution
- All placeholders clearly marked; no real values stored

---

## Rollback

| Script/Config | Rollback/Cleanup |
|---------------|-----------------|
| Backup cadence audit | Read-only; no rollback needed |
| Offsite restore drill | Temp file auto-cleaned on success; manual cleanup on interrupt |
| Metrics baseline | Read-only; no rollback needed |
| SendGrid bridge | Remove custom receiver from alertmanager-config.yaml; reload AlertManager |

---

## References

| Document | Purpose |
|----------|---------|
| [101-phase3g-ops-hardening-plan.md](./101-phase3g-ops-hardening-plan.md) | Phase 3G ops hardening scaffolds |
| [2026-05-09-gcp-phase3h-offsite-monitoring.md](./artifacts/2026-05-09-gcp-phase3h-offsite-monitoring.md) | Phase 3H monitoring deployment |
| [2026-05-09-gcp-phase3j-duckdns-tls-attempt.md](./artifacts/2026-05-09-gcp-phase3j-duckdns-tls-attempt.md) | Phase 3J DuckDNS TLS success |
| [100-phase3f-conditional-sqlite-pilot-authorization.md](./100-phase3f-conditional-sqlite-pilot-authorization.md) | Phase 3F conditional pilot authorization |
| [67-production-readiness-roadmap.md](./67-production-readiness-roadmap.md) | Production readiness roadmap |

---

## Addendum: Production Blocker Cross-Reference (2026-05-15)

Phase 4A SendGrid bridge is the technical template for **Block B — Off-VM Alerting** in the production blocker review.
The bridge remains a scaffold until the operator provides a real provider, API key, and contacts.

| Phase 4A Item | Production Blocker | Runbook / Policy |
|---------------|-------------------|------------------|
| SendGrid bridge template | Block B | [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block B |
| Alert rotation policy | Block B | [`artifacts/2026-05-15-r1-alerting-rotation-policy.md`](./artifacts/2026-05-15-r1-alerting-rotation-policy.md) |

---

## Document History

| Date | Change |
|------|--------|
| 2026-05-09 | Initial Phase 4A ops hardening and alert bridge plan. Scaffolds only. No live GCP mutation. No SendGrid API key stored. |
| 2026-05-15 | Added production blocker cross-reference addendum linking Block B to R1 and R4. No live changes. |

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT PostgreSQL, NOT HA. DuckDNS TLS SUCCESS (Phase 3J). Helper scripts are read-only evidence tools. SendGrid API key placeholder only — stored VM-locally, not in version control.
