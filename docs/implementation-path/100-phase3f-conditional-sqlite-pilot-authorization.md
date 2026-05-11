# 100 — Phase 3F Conditional Single-Node SQLite Pilot Authorization

## Overview

Phase 3F synthesizes signed operator documents and Phase 3A–3E evidence into a consolidated authorization decision for a bounded conditional single-node SQLite pilot on the GCP non-prod VM.

**This document does NOT claim full production-ready, PostgreSQL, HA/multi-node, or full production posture.**

---

## Authorization Decision

### YES — Conditional Single-Node SQLite Pilot Authorized

| Authorization Item | Decision | Date | Reference |
|-------------------|----------|------|-----------|
| Conditional single-node SQLite pilot | **YES** | 09/05/2026 | Doc 54 signed; Doc 99 worksheet signed |
| BrianNguyen as Operator/Owner | **YES** | 09/05/2026 | Doc 99 Part 1 |
| G2 gates satisfied (conditional scope) | **YES** | 09/05/2026 | Doc 99 Part 4, Doc 54 §Pilot Prerequisites |

### Pilot Parameters (Signed)

| Parameter | Signed Value | Reference |
|-----------|-------------|-----------|
| Sustained write rate | ≤300 writes/s | Doc 99 Part 3.4 |
| Peak write rate | ≤300 writes/s | Doc 99 Part 3.4 |
| Daily write volume | ≤1M writes/day | Doc 99 Part 3.4 |
| Single-node topology | **SQLite single-node only** | Doc 99 Part 3.4 |
| RPO | 15 minutes | Doc 99 Part 3.5 |
| RTO | 15 minutes | Doc 99 Part 3.5 |
| TLS domain | `34-158-51-8.nip.io` (temporary, non-prod only) | Doc 99 Part 3.2 |
| Backup cadence | 15-minute systemd timer | Doc 99 Part 3.3 |
| Backup retention | 7 days + offsite copy required before production | Doc 99 Part 3.3 |

---

## NO — Explicitly Not Authorized

| Item | Decision | Reason |
|------|----------|--------|
| Full production-ready claim | **NO** | FerrumGate v1 is RC-ready/conditional; full production-ready not claimed |
| PostgreSQL support | **NO** | PostgreSQL production deployment deferred; local Docker/runtime support complete. `postgres://` DSNs accepted when built with `--features postgres` |
| Multi-node/HA/replica | **NO** | Single-node only; scale-out requires a separate PostgreSQL/HA phase that is not started |
| Full production posture | **NO** | Conditional single-node SQLite pilot only; nip.io is temporary |
| Production domain/TLS | **NO** | Real domain required for production; nip.io is temporary/non-prod |
| Phase 3 PostgreSQL | **NO** | P4.1–P4.3 complete; P4.4/P5 deferred |

---

## Non-Claims (Phase 3F)

> **IMPORTANT**: Phase 3F carries the following explicit non-claims:
> - NOT full production-ready status
> - NOT G2 complete beyond conditional single-node SQLite pilot scope
> - NOT PostgreSQL/multi-node/HA validated
> - NOT full production posture
> - NOT Phase 3 PostgreSQL authorization
> - NOT production domain/TLS authorized (nip.io is temporary only)
>
> **Scope**: Conditional single-node SQLite pilot on GCP non-prod VM only.
> **Canonical docs**: Signed docs 54/59/63/65 remain the canonical authorization record.

---

## Signed Document Chain

| Document | Status | Date | Key Values Signed |
|----------|--------|------|-------------------|
| `54-operator-signoff-packet.md` | Signed by BrianNguyen | 09/05/2026 | G2 gates; pilot acceptance; SQLite limits; auth/TLS; backup; PostgreSQL deferred |
| `99-briannguyen-direct-signing-worksheet.md` | Signed by BrianNguyen | 09/05/2026 | Workload model; RPO/RTO; G2.1–G2.8 signatures; final signoff |
| `63-path-2-target-environment-spec.md` | Updated with signed values | 09/05/2026 | Target environment fields |
| `65-path-2-target-questionnaire.md` | Updated with signed values | 09/05/2026 | Operator identity; TLS/domain; workload model |

---

## Evidence Summary (Phase 3A–3E)

### Phase 3A — GCP Non-Prod VM Target

- VM `ferrumgate-nonprod` created and running
- Static IP `34.158.51.8` assigned
- `ferrumgate.service` active
- `ferrumgate-backup.timer` enabled
- Reference: [artifacts/2026-05-08-gcp-phase3a-nonprod-target.md](./artifacts/2026-05-08-gcp-phase3a-nonprod-target.md)

### Phase 3B — TLS/nip.io/Caddy

- Caddy v2.11.2 active
- TLS via Let's Encrypt on `34-158-51-8.nip.io`
- HTTPS health probe returns HTTP 200
- Reference: [artifacts/2026-05-08-gcp-phase3b-domain-tls.md](./artifacts/2026-05-08-gcp-phase3b-domain-tls.md)

### Phase 3C — Live Rehearsal

- `phase3c_live_rehearsal.sh` passed all checks
- Auth probes: no token → 401, with token → 200
- Service statuses: caddy active, ferrumgate active, backup timer enabled
- Firewall rules verified
- Reference: [artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md](./artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md)

### Phase 3D — G2 Readiness

- G2.2 (auth/TLS): Evidence ready
- G2.4 (restore drill): Passed with `PRAGMA integrity_check=ok`
- G2.1, G2.3, G2.5, G2.6, G2.7, G2.8: Conditional acceptance signed
- Reference: [artifacts/2026-05-08-gcp-phase3d-g2-readiness.md](./artifacts/2026-05-08-gcp-phase3d-g2-readiness.md)

### Phase 3E — SQLite Pilot Evidence

- Read-only evidence script `phase3e_sqlite_pilot_evidence.sh` created
- Artifact scaffold: [artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md)
- Evidence plan: [99-phase3e-sqlite-pilot-evidence-plan.md](./99-phase3e-sqlite-pilot-evidence-plan.md)

---

## Pilot Authorization Scope

### Authorized Scope

| Item | Authorized |
|------|------------|
| Single-node SQLite pilot on GCP non-prod VM | Yes |
| Workload within ≤300 writes/s, ≤1M writes/day | Yes |
| Bearer auth + TLS (nip.io temporary) | Yes |
| 15-minute RPO/RTO | Yes |
| 15-minute backup timer | Yes |
| G2 gates satisfied (conditional) | Yes |

### Not in Scope

| Item | Not Authorized |
|------|---------------|
| Full production-ready claim | No |
| Production deployment | No |
| PostgreSQL | No |
| Multi-node/HA/replica | No |
| Production domain (nip.io is temporary) | No |
| Phase 3 PostgreSQL | No |
| Scale-out beyond single-node | No |

---

## Pilot Constraints

| Constraint | Value |
|------------|-------|
| Topology | SQLite single-node only (no PostgreSQL) |
| Write ceiling | ≤300 writes/s sustained |
| Daily ceiling | ≤1M writes/day |
| RPO | 15 minutes (time since last backup) |
| RTO | 15 minutes (restore + restart + verify) |
| TLS domain | `34-158-51-8.nip.io` (temporary; real domain required for production) |
| Backup retention | 7 days + offsite copy required before production |
| HA/multi-node | Not implemented; not in scope |

---

## What This Authorization Does NOT Cover

1. **Full production-ready claim** — FerrumGate v1 remains RC-ready/conditional
2. **PostgreSQL** — Production deployment deferred; ADR-50 P4.1–P4.3 runtime support complete, P4.4 data migration deferred
3. **Multi-node/HA/replica** — Single-node only in Phase 1
4. **Production domain** — nip.io is temporary and not suitable for production
5. **Production deployment** — Conditional pilot on GCP non-prod VM only
6. **Phase 3 PostgreSQL** — P4.1–P4.3 complete; P4.4 data migration and P5 production deployment deferred

---

## References

| Document | Purpose |
|----------|---------|
| [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) | Canonical operator signoff (signed 09/05/2026) |
| [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| [63-path-2-target-environment-spec.md](./63-path-2-target-environment-spec.md) | Target environment spec (signed values) |
| [65-path-2-target-questionnaire.md](./65-path-2-target-questionnaire.md) | Target questionnaire (signed values) |
| [99-briannguyen-direct-signing-worksheet.md](./99-briannguyen-direct-signing-worksheet.md) | BrianNguyen signed worksheet (09/05/2026) |
| [99-phase3e-sqlite-pilot-evidence-plan.md](./99-phase3e-sqlite-pilot-evidence-plan.md) | Phase 3E evidence plan |
| [artifacts/2026-05-08-gcp-phase3a-nonprod-target.md](./artifacts/2026-05-08-gcp-phase3a-nonprod-target.md) | Phase 3A artifact |
| [artifacts/2026-05-08-gcp-phase3b-domain-tls.md](./artifacts/2026-05-08-gcp-phase3b-domain-tls.md) | Phase 3B artifact |
| [artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md](./artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md) | Phase 3C artifact |
| [artifacts/2026-05-08-gcp-phase3d-g2-readiness.md](./artifacts/2026-05-08-gcp-phase3d-g2-readiness.md) | Phase 3D artifact |
| [artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](./artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md) | Phase 3E artifact scaffold |

---

## Document History

| Date | Change |
|---|---|
| 2026-05-09 | Initial Phase 3F conditional single-node SQLite pilot authorization packet. Synthesizes signed docs and Phase 3A–3E evidence. |
