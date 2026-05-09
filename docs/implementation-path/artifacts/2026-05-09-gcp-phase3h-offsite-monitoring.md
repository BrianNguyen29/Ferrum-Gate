# GCP Phase 3H Offsite Backup and Local Monitoring Execution Artifact

**Date**: 2026-05-09

**Scope**: Phase 3H execution — GCS offsite backup setup, local monitoring deployment, metrics-format fix

**Status**: **NON-PROD execution evidence**. Offsite backup and local monitoring deployed for nonprod VM. NOT production-ready, NOT production alerting.

---

## Non-Claims

This Phase 3H execution does **not** claim:

- production-ready status
- full production posture
- production alerting capability
- real domain deployment (nip.io temporary domain still in use)
- full G2 completion beyond Phase 3F conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3H operator signoff

Phase 3H deployed **local-only monitoring** and **key-based GCS offsite backup** for the nonprod VM. Real domain remains blocked. Production alert contact remains none/local-only.

---

## Overview

Phase 3H executed the following on the nonprod VM:

1. **GCS Offsite Backup**: Created dedicated backup service account, configured GCS bucket access, deployed automated backup via systemd timer
2. **Local Monitoring**: Installed Prometheus and AlertManager, deployed monitoring configs to `/etc/ferrumgate/monitoring/`
3. **Metrics Format Fix**: Fixed leading spaces in `ferrumgate_request_duration_seconds` metric lines in `crates/ferrum-gateway/src/server.rs`, rebuilt and redeployed

---

## Target Environment

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` (temporary - **real domain blocked**) |
| HTTPS URL | `https://34-158-51-8.nip.io` |
| App Port | `19080` (localhost only, behind Caddy) |
| Database | SQLite single-node |
| Auth Mode | Bearer token |
| Monitoring | Local-only (Prometheus + AlertManager on VM) |
| Alert Contact | **None** (local-only mode, no production alerting) |

---

## Pre-Existing Infrastructure (Phase 3A through 3E)

This artifact assumes Phase 3A, 3B, 3C, 3D, and 3E have been previously executed:

- Phase 3A: GCP VM created, binaries deployed, ferrumgate service running, backup timer enabled
- Phase 3B: Caddy installed, TLS configured for nip.io domain
- Phase 3C: Live rehearsal, health/auth checks, monitoring validated
- Phase 3D: G2 readiness checklist, restore drill, metrics snapshot
- Phase 3E: SQLite pilot evidence gathering (read-only)

Reference artifacts:
- [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)
- [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)
- [2026-05-09-gcp-phase3g-scaffold-review.md](./2026-05-09-gcp-phase3g-scaffold-review.md) (scaffold review, not execution)

---

## GCS Offsite Backup Execution

### Service Account Creation

The VM's **default attached service account** was **not usable** for GCS access because:

- OAuth scopes assigned to the VM did not include GCS scopes
- `gsutil` returned `403 Provided scope(s) are not authorized` when attempting to use the attached SA

**Solution**: Created a **dedicated backup service account** for GCS bucket access:

1. Created a dedicated backup service account (identifier intentionally omitted from committed docs)
2. Granted `roles/storage.objectAdmin` on the target bucket
3. Created service account key (JSON)
4. Uploaded key to VM at `/etc/ferrumgate/gcs-service-account.json`
5. Activated service account on VM via `gcloud auth activate-service-account`
6. Removed local temp key after upload

**Note**: Service account key material was handled securely — key was uploaded to VM only and local temp copy was removed. Key is not stored in this artifact.

### GCS Bucket

| Field | Value |
|-------|-------|
| Bucket | `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/` |
| Region | asia-southeast1 |
| Storage Class | Standard |
| Service Account | Dedicated backup service account (identifier intentionally omitted) |

### Initial Backup Sync

Successfully synced first backup file to GCS:

```
Source: /var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db
Destination: gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/ferrumgate_20260508_154446.db
```

Verified via: `gsutil ls gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/`

### Automated Backup (systemd)

Created systemd service and timer for automated backup:

- **Service**: `/etc/systemd/system/ferrumgate-offsite-backup.service`
- **Timer**: `/etc/systemd/system/ferrumgate-offsite-backup.timer`

**Issue encountered**: `gsutil` was not in PATH for systemd service execution.

**Fix**: Updated service to use full path `/snap/bin/gsutil` instead of just `gsutil`.

**Timer status**:
```
Timer: ferrumgate-offsite-backup.timer
Status: enabled, active
Next run: 2026-05-10 05:03:25 UTC
```

**Service last run**: Success (after gsutil path fix)

---

## Local Monitoring Deployment

### Packages Installed

Installed on VM:
- `prometheus` (monitoring backend)
- `prometheus-alertmanager` (alert routing)

### Configuration Deployed

Deployed monitoring configs to `/etc/ferrumgate/monitoring/`:

| File | Purpose |
|------|---------|
| `prometheus-scrape.yaml` | Prometheus scrape job for `/v1/metrics` |
| `alertmanager-config.yaml` | AlertManager routing (localhost webhook receiver) |
| `ferrumgate-alerts.yaml` | Alert rules for ferrumgate service |

### Prometheus Wiring

Updated `/etc/prometheus/prometheus.yml`:
- Included `/etc/prometheus/ferrumgate-alerts.yaml` in `rule_files`
- Configured scrape job for `ferrumgate` target at `34-158-51-8.nip.io:443`

### AlertManager Wiring

Updated `/etc/prometheus/alertmanager.yml`:
- Configured to use `/etc/ferrumgate/monitoring/alertmanager-config.yaml`

**AlertManager mode**: Local-only (localhost webhook receiver)

**Alert Contact**: **None** — local monitoring stack only, no production alerting configured

---

## Metrics Format Fix

### Issue

Prometheus target initially failed with invalid metric exposition:

```
invalid metric name or label names: {__name__=" ferrumgate_request_duration_seconds", ...}
```

Additionally, metric lines in `/v1/metrics` output had **leading spaces** before `ferrumgate_request_duration_seconds`, causing Prometheus to reject them as invalid.

### Root Cause

In `crates/ferrum-gateway/src/server.rs`, metric output format strings had incorrect leading spaces:

```rust
// BEFORE (incorrect - leading space before metric name)
" ferrumgate_request_duration_seconds{...}"

// AFTER (correct - no leading space)
"ferrumgate_request_duration_seconds{...}"
```

### Fix Applied

1. Removed leading spaces from metric registration lines in `crates/ferrum-gateway/src/server.rs`
2. Rebuilt release binaries: `cargo build --release`
3. Deployed `ferrumd` and `ferrumctl` to VM
4. Restarted ferrumgate service: `sudo systemctl restart ferrumgate`

### Verification

After fix:
- Prometheus config check: **passed**
- Target status: **up**
- Query `up{job="ferrumgate"}` returned: **1** for instance `34-158-51-8.nip.io:443`

---

## Final Phase 3E Evidence Rerun

After all Phase 3H deployments and fixes, re-ran Phase 3E evidence script to verify no regressions:

### HTTPS Endpoint Statuses

| Endpoint | Expected | Observed |
|----------|----------|----------|
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz/deep` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | 200 |

### Metrics Snapshot

| Metric | Expected | Observed |
|--------|----------|----------|
| `ferrumgate_store_health_up` | 1 | 1 |
| `ferrumgate_write_queue_depth` | 0 | 0 |

### Auth Probe Results

| Probe | Expected | Observed |
|-------|----------|----------|
| `GET /v1/approvals` without token | 401 | 401 |
| `GET /v1/approvals` with VM-local bearer token | 200 | 200 |

### Service Statuses

| Service | Expected | Observed |
|---------|----------|----------|
| `caddy.service` | active | active |
| `ferrumgate.service` | active | active |
| `ferrumgate-backup.timer` | enabled | enabled |
| `ferrumgate-offsite-backup.timer` | enabled | enabled |

### Backup Files (Read-Only Listing)

```
ferrumgate_20260508_154446.db
```

### Monitoring Stack Status

| Component | Status |
|-----------|--------|
| Prometheus | active |
| AlertManager | active |
| ferrumgate-offsite-backup.timer | enabled, active |
| Prometheus target | up |

---

## What Phase 3H is NOT

Phase 3H does NOT:

- Claim production-ready or full production posture
- Include production alerting capability (local-only mode)
- Deploy real domain (nip.io temporary domain still in use)
- Include service account keys, tokens, or email literals
- Modify Phase 3F conditional pilot scope
- Replace or supersede Phase 3E operator signoff

---

## Remaining Blockers

| Item | Status | Blocker |
|------|--------|---------|
| Real domain TLS | **BLOCKED** | No real domain provided |
| Production alerting | **BLOCKED** | No alert contact; local-only mode |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |

---

## References

- Phase 3G plan: [101-phase3g-ops-hardening-plan.md](../101-phase3g-ops-hardening-plan.md)
- Phase 3G scaffold review: [2026-05-09-gcp-phase3g-scaffold-review.md](./2026-05-09-gcp-phase3g-scaffold-review.md)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Phase 3E evidence: [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)
- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C artifact: [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D artifact: [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)
- Operator signoff: [54-operator-signoff-packet.md](../54-operator-signoff-packet.md)
- Pilot readiness: [59-pilot-readiness-evidence-packet.md](../59-pilot-readiness-evidence-packet.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT real domain deployment. Phase 3H nonprod execution only. Local monitoring in use. Real domain and production alert contact remain blocked.
