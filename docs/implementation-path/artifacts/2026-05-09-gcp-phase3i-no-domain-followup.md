# GCP Phase 3I No-Domain Follow-Up Artifact

**Date**: 2026-05-09

**Scope**: Phase 3I no-domain follow-up — GCS offsite restore drill, local pre-target gate, and discovered operational deltas

**Status**: **NON-PROD evidence only**. No infrastructure/configuration changes executed in this phase. Documents Phase 3H operational discoveries, read-only restore validation, and local validation.

---

## Non-Claims

This Phase 3I follow-up does **not** claim:

- production-ready status
- full production posture
- production alerting capability
- real domain deployment (nip.io temporary domain still in use)
- full G2 completion beyond Phase 3F conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3I operator signoff
- PostgreSQL runtime, HA, or multi-node deployment

Phase 3I documents **local validation evidence** and **Phase 3H operational deltas**. Real domain remains blocked.

---

## Overview

Phase 3I captures:

1. **GCS Offsite Restore Drill**: Verified offsite backup integrity by restoring from GCS to temp file
2. **Local Pre-Target Gate**: Full local validation before any target work
3. **Phase 3H Operational Deltas**: Operational lessons learned from Phase 3H execution

---

## GCS Offsite Restore Drill

Executed on-VM restore drill to validate GCS offsite backup integrity:

```
GCS Bucket: gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
RESTORE_OBJECT=ferrumgate_20260508_154446.db
INTEGRITY=ok
TABLE_COUNT=14
SIZE_BYTES=241664
```

**Result**: PASSED — offsite backup integrity confirmed. Drill copied latest object from GCS to temp file and verified SQLite integrity.

---

## Service Evidence (Phase 3H Post-Deployment)

### Systemd Services

| Service | Status |
|---------|--------|
| `ferrumgate.service` | active |
| `caddy.service` | active |
| `prometheus` | active |
| `prometheus-alertmanager` | active |
| `ferrumgate-offsite-backup.timer` | enabled, active |
| `ferrumgate-offsite-backup.service` | Result=success, ExecMainStatus=0 |

---

## HTTPS Endpoint Evidence

| Endpoint | Status |
|----------|--------|
| `/v1/healthz` | 200 |
| `/v1/readyz` | 200 |
| `/v1/readyz/deep` | 200 |
| `/v1/metrics` | 200 |

### Key Metrics

| Metric | Value |
|--------|-------|
| `ferrumgate_store_health_up` | 1 |
| `ferrumgate_write_queue_depth` | 0 |

---

## Prometheus Evidence

| Query | Result |
|-------|--------|
| `up{job="ferrumgate"}` | 1 (instance `34-158-51-8.nip.io:443`) |

Prometheus scrape target for `ferrumgate` job: **UP**

---

## Local Pre-Target Gate Results

Executed local pre-target gate before any target/remote work:

```
ALL LOCAL CHECKS PASSED
```

### Gate Components

| Check | Result |
|-------|--------|
| cargo fmt check | passed |
| cargo workspace compile check | passed (known corrupt incremental warning only) |
| config examples validation | passed |
| local restore drill | passed |
| evidence generator syntax | passed |
| required docs/configs present | passed |
| local bearer-auth smoke | 7/7 passed |

---

## Phase 3H Operational Deltas

Phase 3H execution revealed three operational issues that required fixes:

### Delta 1: VM OAuth Scope Issue (service-account-key mode required)

**Issue**: VM's default attached service account had insufficient OAuth scopes for GCS access. `gsutil` returned `403 Provided scope(s) are not authorized`.

**Fix**: Created dedicated backup service account with service account key JSON uploaded to VM. Used `service-account-key` auth mode instead of `vm-service-account`.

**Documentation**: Updated `phase3g_configure_offsite_backup.sh` prerequisites to note VM attached SA may lack GCS scopes.

### Delta 2: gsutil PATH in systemd

**Issue**: `gsutil` not in PATH when executed from systemd service context.

**Fix**: Used full path `/snap/bin/gsutil` in systemd unit file.

**Documentation**: Updated `phase3g_configure_offsite_backup.sh` to note snap-installed gsutil requires `/snap/bin/gsutil` path in systemd context.

### Delta 3: Prometheus Monitoring Wiring

**Issue**: Monitoring configs deployed to `/etc/ferrumgate/monitoring/` needed explicit wiring into `/etc/prometheus/prometheus.yml` and `/etc/prometheus/alertmanager.yml`.

**Fix**: Manually wired:
- Added `rule_files` entry for `/etc/prometheus/ferrumgate-alerts.yaml` to prometheus.yml
- Added scrape job for `ferrumgate` target
- Wired AlertManager to use `/etc/ferrumgate/monitoring/alertmanager-config.yaml`

**Documentation**: Updated `phase3g_configure_monitoring.sh` to include explicit wiring steps.

---

## Target Environment (Unchanged)

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` (temporary — **real domain BLOCKED**) |
| HTTPS URL | `https://34-158-51-8.nip.io` |
| Database | SQLite single-node |
| Auth Mode | Bearer token |
| Monitoring | Local-only (Prometheus + AlertManager on-VM) |
| Alert Contact | **None** (local-only mode) |

---

## What Phase 3I is NOT

Phase 3I does NOT:

- Claim production-ready or full production posture
- Include production alerting capability (local-only mode)
- Deploy real domain (nip.io temporary domain still in use)
- Include service account keys, tokens, or email literals
- Modify Phase 3F conditional pilot scope
- Replace Phase 3E operator signoff

---

## Remaining Blockers (Unchanged from Phase 3H)

| Item | Status | Blocker |
|------|--------|---------|
| Real domain TLS | **BLOCKED** | No real domain provided |
| Production alerting | **BLOCKED** | No alert contact; local-only mode |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |

---

## References

- Phase 3G plan: [101-phase3g-ops-hardening-plan.md](../101-phase3g-ops-hardening-plan.md)
- Phase 3G scaffold review: [2026-05-09-gcp-phase3g-scaffold-review.md](./2026-05-09-gcp-phase3g-scaffold-review.md)
- Phase 3H offsite monitoring: [2026-05-09-gcp-phase3h-offsite-monitoring.md](./2026-05-09-gcp-phase3h-offsite-monitoring.md)
- Phase 3I no-domain follow-up: [2026-05-09-gcp-phase3i-no-domain-followup.md](./2026-05-09-gcp-phase3i-no-domain-followup.md) (this artifact)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Phase 3E evidence: [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT real domain deployment. Phase 3I nonprod evidence only. Local monitoring in use. Real domain and production alert contact remain blocked.
