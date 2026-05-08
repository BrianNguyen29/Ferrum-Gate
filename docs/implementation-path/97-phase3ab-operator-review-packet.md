# 97 — Phase 3A/3B Operator Review Packet

**Prepared for**: BrianNguyen

**Status**: UNSIGNED — Prepared for operator review. This document is awaiting operator review and signature.

> **Important**: This packet is **NOT signed**. It serves as a review preparation document. Operator signoff is required before any production pilot authorization. Do not treat this as an authorization document.

---

## Overview

This packet consolidates Phase 3A (GCP non-prod VM target) and Phase 3B (TLS/nip.io/Caddy) evidence for operator review. It is designed to help the operator (BrianNguyen) evaluate the non-prod rehearsal results and prepare for potential future signoff.

**This is NOT production-ready evidence. This is NOT G2 completion. This is NOT pilot authorization.**

---

## Non-Claims

> **IMPORTANT**: Phase 3A and Phase 3B carry the following explicit non-claims:
> - NOT production-ready
> - NOT G2 complete (operator signoff pending)
> - NOT pilot authorized
> - NOT operator signoff
> - NOT suitable for production workloads
> - NOT a permanent domain solution (nip.io is temporary)
> - NOT replacing canonical operator docs 54/58/59/63/65
>
> Phase 3A and Phase 3B are operator-owned GCP non-prod rehearsal/evidence support only.

---

## Phase 3A Summary (GCP Non-Prod VM Target)

**Artifact**: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)

### What Was Created

| Resource | Value |
|----------|-------|
| GCP Project | `fairy-b13f4` |
| Region/Zone | `asia-southeast1` / `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` (e2-medium, Ubuntu 24.04 LTS) |
| VPC | `ferrumgate-nonprod-vpc` (/24 subnet) |
| Static External IP | `34.158.51.8` |
| Firewall | SSH (22) and app (19080) from `118.69.4.63/32` only |
| Store | SQLite at `/var/lib/ferrumgate/data/ferrumgate.db` |
| Backup | Hourly timer + manual trigger via `ferrumgate-backup.service` |

### Scripts Used

| Script | Purpose |
|--------|---------|
| `phase3a_create_nonprod_vm.sh` | Creates GCP resources (VPC, subnet, IP, firewall, VM) |
| `phase3a_deploy_binaries.sh` | Builds release, copies binaries to VM, restarts service |
| `phase3a_bootstrap_vm.sh` | Bootstraps VM: user, dirs, config, service, backup timer |
| `phase3a_destroy_nonprod_vm.sh` | Cleanup script (not executed; preserves evidence) |

### Phase 3A Observed Results

| Check | Expected | Observed |
|-------|----------|----------|
| VM created and running | Yes | Yes |
| ferrumgate.service active | Yes | Yes |
| ferrumgate-backup.timer enabled | Yes | Yes |
| `/v1/healthz` (with token) | 200 | 200 |
| `/v1/readyz` (with token) | 200 | 200 |
| `/v1/approvals` without token | 401 | 401 |
| `/v1/approvals` with token | 200 | 200 |
| Manual backup created | Yes | Yes (`ferrumgate_20260508_154446.db`) |

### Phase 3A Non-Claims

This Phase 3A run does **not** constitute:
- Production-ready deployment
- G2 gate completion
- Pilot authorization
- Operator signoff

---

## Phase 3B Summary (TLS/nip.io/Caddy)

**Artifact**: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)

### What Was Added

| Component | Value |
|-----------|-------|
| TLS Domain | `34-158-51-8.nip.io` (temporary, not for production) |
| TLS Terminator | Caddy v2.11.2 |
| HTTPS Port | 443 |
| HTTP Port | 80 (ACME challenge) |
| ferrumgate bind | `127.0.0.1:19080` (localhost only, behind Caddy) |
| Firewall | Added 80 and 443 from `0.0.0.0/0` (for ACME and HTTPS) |

### Scripts Used

| Script | Purpose |
|--------|---------|
| `phase3b_configure_tls_caddy.sh` | Installs Caddy, configures TLS, tests HTTPS endpoints |
| `phase3b_destroy_tls_caddy.sh` | Rolls back TLS config (not executed; preserves evidence) |

### Phase 3B Observed Results

| Check | Expected | Observed |
|-------|----------|----------|
| Caddy installed and active | Yes | Yes (v2.11.2) |
| ferrumgate.bind changed to localhost | Yes | Yes |
| TLS certificate provisioned | Yes | Yes (Let's Encrypt via Caddy) |
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | 200 |
| `GET /v1/approvals` without token | 401 | 401 |
| `GET /v1/approvals` with VM-local token | 200 | 200 |

### Phase 3B Non-Claims

This Phase 3B run does **not** constitute:
- Production-ready TLS configuration (nip.io is temporary)
- G2 gate completion
- Pilot authorization
- Operator signoff

---

## Phase 3C Summary (Live Rehearsal)

**Artifact**: [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)

**Live Ops Packet**: [96-gcp-compute-phase3c-live-ops-packet.md](./96-gcp-compute-phase3c-live-ops-packet.md)

### Phase 3C Script

`scripts/gcp/phase3c_live_rehearsal.sh` provides bounded non-destructive rehearsal checks:

- HTTPS health/readiness/deep/metrics probes
- Auth probes (401 without token, 200 with VM-local token)
- Service status checks (caddy, ferrumgate, backup timer)
- Firewall rule summary
- Backup timer status
- Optional manual backup trigger (requires `--confirm`)

### Phase 3C Non-Claims

This Phase 3C rehearsal does **not** constitute:
- Production-ready validation
- G2 gate completion
- Pilot authorization
- Operator signoff

---

## Phase 3D Summary (G2 Readiness)

**Artifact**: [2026-05-08-gcp-phase3d-g2-readiness.md](./artifacts/2026-05-08-gcp-phase3d-g2-readiness.md)

**G2 Readiness Checklist**: [98-phase3d-g2-readiness-checklist.md](./98-phase3d-g2-readiness-checklist.md)

### Phase 3D Evidence Collected

| Evidence Type | Result |
|---------------|--------|
| Restore drill | INTEGRITY=ok; TABLE_COUNT=14; RESTORE_COPY_REMOVED=yes |
| Metrics snapshot | store_health_up=1; write_queue_depth=0; no 503 errors |
| TLS/auth probes | NO_TOKEN=401; WITH_TOKEN=200 |
| Phase 3C smoke | PASSED (all checks) |
| Light workload smoke | 5/5 healthz, 5/5 readyz, 5/5 readyz/deep, 5/5 metrics |

### G2 Gate Readiness Summary

| Gate | Status |
|------|--------|
| G2.1 Target workload model | operator-required |
| G2.2 Bearer auth + TLS + firewall | ready |
| G2.3 Backup schedule evidence | partial |
| G2.4 Restore drill | ready |
| G2.5 RPO/RTO acceptance | operator-required |
| G2.6 Production evaluation framework | partial |
| G2.7 Accepted-risk review | partial |
| G2.8 Compensate noop risk | partial |

**Conservative conclusion**: G2 is NOT complete. Phase 3D evidence suggests the GCP non-prod target is ready for operator review only. All G2 gates remain open pending operator signoff.

### Phase 3D Non-Claims

This Phase 3D evidence collection does **not** constitute:
- Production-ready status
- G2 gate completion
- Pilot authorization
- Operator signoff

---

## Token Security

- Full bearer token stored at `/etc/ferrumgate/ferrumgate_initial_token` (root-only, mode 600)
- Scripts print only 8-character token prefix, never full token
- Full token retrieved on-VM via `sudo` for immediate probe use only
- Full token never printed to logs or committed to repository

---

## What's Still Needed for Production Pilot

Canonical G2/operator signoff requires completing and signing:

| Document | Purpose | Status |
|----------|---------|--------|
| `54-operator-signoff-packet.md` | Formal operator acceptance checklist | **Pending operator signoff** |
| `58-workload-compensation-drill-evidence-template.md` | D1-D6 drill evidence | Operator-defined |
| `59-pilot-readiness-evidence-packet.md` | G2.1-G2.8 evidence sections | Operator-defined |
| `63-path-2-target-environment-spec.md` | Target environment specification | Operator-defined |
| `65-path-2-target-questionnaire.md` | Target questionnaire | Operator-defined |

**None of the Phase 3A/3B/3C/3D evidence substitutes for the canonical operator review process.**

---

## Operator Review Checklist

Use this checklist to evaluate Phase 3A/3B/3C/3D evidence:

- [ ] Phase 3A VM creation was successful and service is stable
- [ ] Phase 3B TLS configuration works and HTTPS endpoints respond correctly
- [ ] Auth probes return expected 401 (no token) and 200 (with token)
- [ ] Backup timer is enabled and manual backup produces valid SQLite backup
- [ ] Firewall rules are reviewed and acceptable for non-prod rehearsal
- [ ] Token security protocol is understood and followed
- [ ] Phase 3C live rehearsal script passed all checks (fail-closed behavior confirmed)
- [ ] Phase 3D restore drill passed (INTEGRITY=ok, 14 tables)
- [ ] Phase 3D metrics snapshot shows healthy store (up=1, queue=0, no 503 errors)
- [ ] G2 gate readiness summary reviewed (doc 98)
- [ ] Non-prod nature is acknowledged (NOT production, NOT G2, NOT pilot)
- [ ] Canonical docs 54/58/59/63/65 are still required for production pilot signoff

---

## Signature Section (Operator Use)

> **This document is UNSIGNED. It is prepared for operator review.**

To sign, the operator must complete the appropriate sections of `54-operator-signoff-packet.md`. This document (97) is a Phase 3A/3B/3C/3D evidence summary only and does not constitute signoff.

### Phase 3A/3B/3C/3D Evidence Review

Operator name: _______________________________

Date reviewed: _______________

Evidence accepted for Phase 3A/3B/3C/3D non-prod rehearsal: [ ] Yes  [ ] No

Notes: _______________________________

Operator signature: _______________________________ Date: _______________

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial Phase 3A/3B operator review packet. UNSIGNED. Prepared for BrianNguyen review. |
| 2026-05-08 | Added Phase 3C summary. |
| 2026-05-08 | Added Phase 3D summary referencing G2 readiness checklist (doc 98) and evidence artifact. Updated review checklist. |
