# 101 — Phase 3G Ops Hardening Plan

## Overview

Phase 3G creates safe repo-side scaffolds for ops hardening: real-domain TLS configuration, offsite backup to GCS, and monitoring/alerting deployment. These are **scaffolds only** — no actual GCP mutation is performed, and deployment is explicitly blocked on real domain, GCS bucket, and alert contact inputs.

**This document does NOT claim full production-ready, full production posture, or production deployment.**

---

## Non-Claims (Phase 3G)

> **IMPORTANT**: Phase 3G carries the following explicit non-claims:
> - NOT actual GCP mutation execution
> - NOT production deployment
> - NOT full production posture claim
> - NOT full production-ready claim
> - NOT deployment without explicit operator inputs
> - Scripts require --confirm and all required inputs before any mutation
> - Actual deployment blocked on: real domain, GCS bucket, monitoring target/alert contact
> - No secrets, no email literals stored in configs or scripts

---

## Scope

### What Phase 3G IS

- Repo-side scaffolds (scripts, configs, docs) for ops hardening
- Scripts with explicit --confirm gates that require all inputs before mutating
- Monitoring config templates with placeholder values
- Documentation explaining what inputs are needed and what is blocked

### What Phase 3G is NOT

- No actual GCP VM, firewall, bucket, or DNS mutations
- No actual deployment or execution
- No production-ready claim
- No secrets or email literals in scripts/configs
- No monitoring backend deployment

---

## Phase 3G Components

### 1. Real-Domain TLS Configuration

**Script**: `scripts/gcp/phase3g_configure_real_domain.sh`

Replaces nip.io temporary domain with a real domain. Requires:
- Real domain name (must be provided by operator)
- DNS A record pointing to VM IP
- GCP project/region/zone/VM name

**Blocked until**: Real domain provided, DNS A record confirmed

### 2. Offsite Backup to GCS

**Script**: `scripts/gcp/phase3g_configure_offsite_backup.sh`

Configures automatic offsite backup from VM to GCS bucket. Supports two auth modes:
- **vm-service-account mode (default, keyless)**: Uses VM's attached service account
  - Requires VM to have a service account attached with GCS bucket write permissions
  - No service account key JSON required
- **service-account-key mode**: Uses service account key JSON file
  - Requires operator to provide and upload the key file
  - Requires service account email

**Inputs**:
- GCS bucket name (must be created by operator first)
- Auth mode: vm-service-account (default) or service-account-key
- For service-account-key mode: service account email

**Blocked until (vm-service-account mode)**: GCS bucket created, VM has attached service account with GCS write permissions
**Blocked until (service-account-key mode)**: GCS bucket created, service account key provided

### 3. Monitoring and Alerting

**Script**: `scripts/gcp/phase3g_configure_monitoring.sh`

Deploys monitoring config to VM for Prometheus scraping + alerting. Supports two modes:
- **Local-only mode (default, RECOMMENDED for non-production)**: No real alert contact required
  - Uses localhost receivers only
  - Non-production claim boundary clearly stated
  - No alert contact flag needed
- **External mode**: Requires real alert contact for external Prometheus/AlertManager
  - Alert contact placeholder required (no email literal stored)

**Inputs**:
- Mode: local-only (default) or external
- Alert contact (required only for external mode)
- Prometheus/AlertManager URLs (defaults to localhost)

**Blocked until (local-only mode)**: None — can deploy without real alert contact
**Blocked until (external mode)**: Alert contact provided, Prometheus/AlertManager endpoints available

**Config templates**: `configs/monitoring/`

**Non-production claim boundary (local-only mode)**: NOT production-ready, NOT production alerting, local stack only

---

## Inputs Summary

### Offsite Backup Inputs

| Input | Status | Description |
|-------|--------|-------------|
| GCS bucket name | **REQUIRED** | Pre-created bucket for offsite backup |
| Auth mode | **OPTIONAL** | vm-service-account (default/keyless) or service-account-key |
| Service account (key mode) | **REQUIRED (key mode only)** | Service account email for key mode |
| VM attached SA (VM mode) | **REQUIRED (VM mode)** | VM must have attached SA with GCS write |

### Monitoring Inputs

| Input | Status | Description |
|-------|--------|-------------|
| Mode | **OPTIONAL** | local-only (default) or external |
| Alert contact | **REQUIRED (external mode only)** | Placeholder (not literal) |
| Prometheus URL | **OPTIONAL** | Default: http://localhost:9090 |
| AlertManager URL | **OPTIONAL** | Default: http://localhost:9093 |

### Real-Domain TLS Inputs (Separate Script)

| Input | Status | Description |
|-------|--------|-------------|
| Real domain | **REQUIRED** | Domain name (e.g., `api.example.com`) |
| DNS A record | **REQUIRED** | Confirmed pointing to VM static IP |

---

## Current Environment (nip.io Temporary)

| Field | Current Value | Notes |
|-------|---------------|-------|
| Project | `fairy-b13f4` | GCP project |
| Region | `asia-southeast1` | GCP region |
| Zone | `asia-southeast1-a` | GCP zone |
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Static IP | `34.158.51.8` | External IP |
| TLS Domain | `34-158-51-8.nip.io` | **Temporary — to be replaced** |
| HTTPS URL | `https://34-158-51-8.nip.io` | **Temporary** |

---

## Script Usage Patterns

### Real-Domain TLS (phase3g_configure_real_domain.sh)

```bash
# BLOCKED until real domain provided
bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --real-domain api.example.com
```

### Offsite Backup (phase3g_configure_offsite_backup.sh)

```bash
# vm-service-account mode (KEYLESS - default, RECOMMENDED):
# BLOCKED until GCS bucket created and VM has attached SA with GCS write
bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/

# service-account-key mode (requires operator-provided key):
bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://my-backup-bucket/ferrumgate/ \
  --auth-mode service-account-key \
  --service-account OPERATOR_PROVIDED_SERVICE_ACCOUNT
```

### Monitoring (phase3g_configure_monitoring.sh)

```bash
# LOCAL-ONLY mode (RECOMMENDED for non-production, NO real alert contact):
# NOT blocked - can deploy without real alert contact
bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --local-only \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod

# EXTERNAL mode (BLOCKED until alert contact provided):
bash scripts/gcp/phase3g_configure_monitoring.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --alert-contact OPERATOR_PROVIDED_ALERT_CONTACT \
  --prometheus-url http://localhost:9090
```

---

## Monitoring Config Templates

Located in `configs/monitoring/`:

- `prometheus-scrape-config.yaml` — Prometheus scrape job for ferrumgate metrics
- `alertmanager-config.yaml` — AlertManager template with placeholder receivers
- `ferrumgate-alerts.yaml` — Alert rules for ferrumgate service

**All monitoring configs use placeholder values. No real secrets or email literals.**

---

## Rollback

| Script | Rollback Command |
|--------|------------------|
| Real-domain TLS | Run `phase3b_destroy_tls_caddy.sh --confirm` to restore nip.io |
| Offsite backup | Edit cron/systemd to remove gsutil sync command |
| Monitoring | Remove deployed config files from VM |

---

## References

| Document | Purpose |
|----------|---------|
| [100-phase3f-conditional-sqlite-pilot-authorization.md](./100-phase3f-conditional-sqlite-pilot-authorization.md) | Phase 3F conditional pilot authorization |
| [94-gcp-compute-phase3a-nonprod-target-plan.md](./94-gcp-compute-phase3a-nonprod-target-plan.md) | Phase 3A non-prod target plan |
| [95-gcp-compute-phase3b-domain-tls-plan.md](./95-gcp-compute-phase3b-domain-tls-plan.md) | Phase 3B TLS/nip.io plan |
| [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) | Canonical operator signoff |
| [67-production-readiness-roadmap.md](./67-production-readiness-roadmap.md) | Production readiness roadmap |

---

## Addendum: Production Blocker Cross-Reference (2026-05-15)

Phase 3G scaffolds map directly to the active production blockers documented in `67-production-readiness-roadmap.md`.

| Phase 3G Component | Production Blocker | Runbook / Policy |
|--------------------|-------------------|------------------|
| Real-Domain TLS (`phase3g_configure_real_domain.sh`) | Block A | [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block A |
| Offsite Backup (`phase3g_configure_offsite_backup.sh`) | Block C | [`artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md`](./artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md) and R4 §Block C |
| Monitoring (`phase3g_configure_monitoring.sh`) | Block B (local-only baseline) | [`artifacts/2026-05-15-r4-production-blocker-execution-runbook.md`](./artifacts/2026-05-15-r4-production-blocker-execution-runbook.md) §Block B |

---

## Document History

| Date | Change |
|------|--------|
| 2026-05-09 | Initial Phase 3G ops hardening plan. Scaffolds only. No GCP mutation. |
| 2026-05-15 | Added production blocker cross-reference addendum linking Phase 3G components to R2 and R4. No live changes. |

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT actual deployment. All scripts require --confirm and required inputs before mutating.
