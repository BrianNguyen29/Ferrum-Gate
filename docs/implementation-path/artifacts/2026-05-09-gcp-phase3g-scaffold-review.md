# GCP Phase 3G Scaffold Review Artifact

**Date**: 2026-05-09

**Scope**: Phase 3G ops hardening scaffold review — real-domain TLS, offsite backup, monitoring/alerting

**Status**: **NON-PROD scaffold review only**. No actual Phase 3G deployment executed.

---

## Non-Claims

This Phase 3G review does **not** claim:

- production-ready status
- full production posture
- actual Phase 3G deployment or GCP mutation executed
- full G2 completion beyond Phase 3F conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3G operator signoff

Phase 3G provides **scaffold review** for ops hardening only. Actual deployment is **blocked** on operator-provided inputs (real domain, GCS bucket, service account, alert contact).

---

## Overview

Phase 3G scaffolds cover three ops hardening areas:

1. **Real-domain TLS**: Replace nip.io temporary domain with production domain
2. **Offsite backup**: Configure GCS bucket for backup offsite storage
3. **Monitoring/alerting**: Deploy Prometheus scrape config and AlertManager templates

This artifact documents:
- Script validation (--help and bash -n checks)
- Current VM state (via Phase 3E evidence rerun)
- Blocked deployment inputs matrix
- Operator action items for completing deployment

---

## Target Environment (Current — Unchanged)

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` (temporary) |
| HTTPS URL | `https://34-158-51-8.nip.io` (temporary) |
| App Port | `19080` (localhost only, behind Caddy) |
| TLS Terminator | Caddy v2.11.2 |
| Database | SQLite single-node |
| Auth Mode | Bearer token |

---

## Pre-Existing Infrastructure (Phase 3A + 3B + 3C + 3D + 3E)

This artifact assumes Phase 3A, 3B, 3C, 3D, and 3E have been previously executed:

- Phase 3A: GCP VM created, binaries deployed, ferrumgate service running, backup timer enabled
- Phase 3B: Caddy installed, TLS configured for nip.io domain, ferrumgate bind changed to localhost
- Phase 3C: Live rehearsal, health/auth checks, monitoring validated
- Phase 3D: G2 readiness checklist, restore drill, metrics snapshot
- Phase 3E: SQLite pilot evidence gathering (read-only)

Reference artifacts:
- [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)
- [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)

---

## Script Validation

### Syntax Checks

```bash
bash -n scripts/gcp/phase3g_configure_real_domain.sh
bash -n scripts/gcp/phase3g_configure_offsite_backup.sh
bash -n scripts/gcp/phase3g_configure_monitoring.sh
```

Result: **passed** — all three scripts are syntactically valid.

### Help Output Verification

All three scripts pass `--help` without error:

```bash
bash scripts/gcp/phase3g_configure_real_domain.sh --help
bash scripts/gcp/phase3g_configure_offsite_backup.sh --help
bash scripts/gcp/phase3g_configure_monitoring.sh --help
```

### Script Gate Verification

| Script | --confirm Gate | Required Inputs | Gate Working |
|--------|---------------|----------------|-------------|
| `phase3g_configure_real_domain.sh` | Yes | `--real-domain` | Verified |
| `phase3g_configure_offsite_backup.sh` | Yes | `--gcs-bucket`, `--service-account` | Verified |
| `phase3g_configure_monitoring.sh` | Yes | `--alert-contact` | Verified |

All scripts enforce explicit `--confirm` gate and validate required inputs before any mutation.

---

## Live Read-Only VM Evidence (Phase 3E Rerun)

The following evidence was gathered via Phase 3E script rerun on 2026-05-09 to confirm current VM state:

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

Token handling: The script confirms VM-local bearer token availability without printing the token or token prefix. The full bearer token is retrieved on-VM via `sudo` and used only for the immediate auth probe. The full token is never printed to logs or committed.

### Service Statuses

| Service | Expected | Observed |
|---------|----------|----------|
| `caddy.service` | active | active |
| `ferrumgate.service` | active | active |
| `ferrumgate-backup.timer` | enabled | enabled |

### Backup Files (Read-Only Listing)

```
ferrumgate_20260508_154446.db
```

### Firewall Summary

| Rule | Port | Source | Observed |
|------|------|--------|----------|
| `ferrumgate-nonprod-fw-ssh` | TCP 22 | `118.69.4.63/32` | present |
| `ferrumgate-nonprod-fw-app` | TCP 19080 | `118.69.4.63/32` | present |
| `ferrumgate-nonprod-fw-http` | TCP 80 | `0.0.0.0/0` | present |
| `ferrumgate-nonprod-fw-https` | TCP 443 | `0.0.0.0/0` | present |

### Backup Timer Next Run

```
2026-05-09 05:03:25 UTC
```

---

## Blocked Deployment Inputs

Phase 3G actual deployment is **blocked** on the following operator-provided inputs:

### Real-Domain TLS (phase3g_configure_real_domain.sh)

| Input | Status | Description |
|-------|--------|-------------|
| Real domain name | **MISSING** | Domain must be provided by operator (placeholder: `OPERATOR_PROVIDED_DOMAIN`) |
| DNS A record | **MISSING** | Must point to VM IP `34.158.51.8`; operator must configure externally |

### Offsite Backup (phase3g_configure_offsite_backup.sh)

| Input | Status | Description |
|-------|--------|-------------|
| GCS bucket name | **MISSING** | Bucket must be pre-created by operator |
| Service account | **MISSING** | SA must have `roles/storage.objectAdmin` on bucket |
| Service account key | **MISSING** | Key must be downloaded and uploaded to VM |

### Monitoring/Alerting (phase3g_configure_monitoring.sh)

| Input | Status | Description |
|-------|--------|-------------|
| Alert contact | **MISSING** | Email placeholder must be provided (not literal) |
| Prometheus URL | Optional | Default `http://localhost:9090` |
| AlertManager URL | Optional | Default `http://localhost:9093` |

### Summary Matrix

| Phase 3G Component | Deployment Blocked | Blocker |
|-------------------|------------------|---------|
| Real-domain TLS | **YES** | No real domain provided |
| Offsite backup to GCS | **YES** | No GCS bucket or service account |
| Monitoring/alerting | **YES** | No alert contact provided |

---

## Phase 3G Scaffold Files

The following scaffold files were created:

### Scripts

| File | Purpose |
|------|---------|
| `scripts/gcp/phase3g_configure_real_domain.sh` | Replace nip.io with real domain TLS |
| `scripts/gcp/phase3g_configure_offsite_backup.sh` | Configure GCS offsite backup |
| `scripts/gcp/phase3g_configure_monitoring.sh` | Deploy monitoring configs to VM |

### Monitoring Config Templates

| File | Purpose |
|------|---------|
| `configs/monitoring/prometheus-scrape-config.yaml` | Prometheus scrape job template |
| `configs/monitoring/alertmanager-config.yaml` | AlertManager routing template |
| `configs/monitoring/ferrumgate-alerts.yaml` | FerrumGate alert rules |
| `configs/monitoring/README.md` | Monitoring configs README |

### Documentation

| File | Purpose |
|------|---------|
| `docs/implementation-path/101-phase3g-ops-hardening-plan.md` | Phase 3G ops hardening plan |

---

## Next Operator Actions

To complete Phase 3G deployment, the operator must provide the following:

### For Real-Domain TLS

- [ ] Obtain a real domain name (placeholder: `OPERATOR_PROVIDED_DOMAIN`)
- [ ] Configure DNS A record: `<real-domain>` → `34.158.51.8`
- [ ] Verify DNS propagation: `dig +short <real-domain>`
- [ ] Run deployment script with real domain:
  ```bash
  bash scripts/gcp/phase3g_configure_real_domain.sh --confirm \
    --real-domain OPERATOR_PROVIDED_DOMAIN
  ```

### For Offsite Backup to GCS

- [ ] Create GCS bucket (e.g., `gs://my-backup-bucket/ferrumgate/`)
- [ ] Create service account with `roles/storage.objectAdmin` on bucket
- [ ] Download service account key JSON
- [ ] Run deployment script:
  ```bash
  bash scripts/gcp/phase3g_configure_offsite_backup.sh --confirm \
    --gcs-bucket gs://OPERATOR_PROVIDED_BUCKET/ferrumgate/ \
    --service-account OPERATOR_PROVIDED_SERVICE_ACCOUNT
  ```

### For Monitoring/Alerting

- [ ] Determine alert contact (email or other receiver)
- [ ] Verify Prometheus/AlertManager endpoints available
- [ ] Run deployment script:
  ```bash
  bash scripts/gcp/phase3g_configure_monitoring.sh --confirm \
    --alert-contact OPERATOR_PROVIDED_ALERT_CONTACT
  ```

### For All Phase 3G Components

- [ ] Review all scaffold scripts and configs before deployment
- [ ] Verify inputs match production environment requirements
- [ ] After deployment, re-run Phase 3E evidence script to verify no regressions
- [ ] Update canonical docs (54, 59, 63, 65) with real domain/alert contact values if needed

---

## What Phase 3G is NOT

Phase 3G does NOT:

- Execute actual GCP mutations or deployment
- Claim production-ready or full production posture
- Include real secrets, tokens, or email literals in configs
- Deploy without explicit operator-provided inputs
- Modify VM, firewall, bucket, DNS, or any GCP resource
- Claim full G2 completion beyond Phase 3F conditional pilot scope

---

## References

- Phase 3G plan: [101-phase3g-ops-hardening-plan.md](../101-phase3g-ops-hardening-plan.md)
- Phase 3F authorization: [100-phase3f-conditional-sqlite-pilot-authorization.md](../100-phase3f-conditional-sqlite-pilot-authorization.md)
- Phase 3E evidence: [2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)
- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C artifact: [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D artifact: [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)
- Operator signoff: [54-operator-signoff-packet.md](../54-operator-signoff-packet.md)
- Pilot readiness: [59-pilot-readiness-evidence-packet.md](../59-pilot-readiness-evidence-packet.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT actual deployment executed. Phase 3G scaffold review only. Deployment blocked on operator-provided real domain, GCS bucket, service account, and alert contact.
