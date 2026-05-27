# 00b — Tier 1.5 Domainless Production Infrastructure

> **Status**: PLANNED / NOT COMPLETE. This doc defines the Tier 1.5 framework only; no completion is claimed.
> **Owner**: Engineering
> **Last updated**: 2026-05-27
> **Parent**: [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md)
> **Scope**: [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md)
> **Completion tracker**: [`docs/production-readiness-v2/13-tier-1.5-completion-status.md`](./13-tier-1.5-completion-status.md)

---

## Goal

Define Tier 1.5 as the optional, final intermediate tier between Tier 1 (domainless production-candidate) and Tier 2 (production-ready / domain-backed). Tier 1.5 represents "Domainless Production Infrastructure Complete": the infrastructure layer is hardened for production semantics, but the system-level production-ready claim remains gated by real domain and full G2 re-signoff.

Tier 1.5 is explicitly the final intermediate tier. No further subtier (e.g., Tier 1.75) may be introduced without a written ADR and explicit operator acknowledgment.

---

## Tier 1.5 definition

Tier 1.5 is **not** production-ready. It is an infrastructure-layer milestone that says:

- Engineering work for PostgreSQL production deployment, HA multi-node topology, and automated failover is complete.
- Evidence artifacts for that infrastructure scope exist and are reviewable.
- The system is a credible candidate for production deployment **once** a real domain is added and Tier 2 gates are satisfied.
- `production-ready = NO` remains true at Tier 1.5.
- `full G2 = NOT COMPLETE` remains true at Tier 1.5.
- `Block A = WAIVED/CONDITIONAL` remains true at Tier 1.5.

### What Tier 1.5 includes (infrastructure-layer scope)

- **PostgreSQL production deployment**: Target/staging PostgreSQL provisioned, ferrumd starts with production DSN, `/v1/readyz/deep` reports PG health, migration completes with row-count and content-hash validation, TLS/SSL DSN guidance followed, PgBouncer or equivalent connection-pooling story operational, backup/restore drill passes with RPO/RTO measured, alert rules deployed and validated.
- **HA multi-node topology**: At least two-node PostgreSQL primary/standby streaming replication deployed in an operator environment, read/write routing documented and validated, replication lag measured and within acceptable bounds, fencing/split-brain prevention designed and documented.
- **Automated failover**: Failover occurs without manual `pg_promote`, ferrumd reconnects to the new primary without manual restart, RTO/RPO measured and documented, no split-brain observed, incident log generated, at least three drills performed with evidence.

### What Tier 1.5 explicitly does NOT include

| Non-claim | Status at Tier 1.5 |
|-----------|------------------|
| **production-ready** | **NO** — Tier 1.5 is not production-ready. |
| **full G2** | **NOT COMPLETE** — G2.1–G2.8 remain signed for conditional pilot only. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain still required for Tier 2. |
| **Sustained SLO window** | **NO** — bounded runs and drills only; no 7–30 day observation window. |
| **Real domain** | **NO** — Tier 1.5 remains explicitly domainless. |
| **Operator final signoff** | **NO** — Tier 1.5 requires operator acknowledgment, not final production signoff. |

---

## Tier 1.5 acceptance gates

### PostgreSQL production deployment gates

| Gate | Criterion | Evidence required |
|------|-----------|-------------------|
| PG-P.1 | Target PostgreSQL provisioned and reachable from ferrumd host | PG target deployment signoff |
| PG-P.2 | ferrumd starts with production postgres DSN and `/v1/readyz/deep` returns 200 | Deployment evidence artifact |
| PG-P.3 | TLS/SSL encrypted DSN validated or operator waiver documented | TLS DSN evidence artifact |
| PG-P.4 | PgBouncer or equivalent connection-pooling story operational | PgBouncer evidence artifact |
| PG-P.5 | Backup/restore drill passes with row counts and hash checks | Backup/restore drill evidence |
| PG-P.6 | Alert rules deployed to live Prometheus and validated | Alert deployment evidence artifact |

### HA multi-node topology gates

| Gate | Criterion | Evidence required |
|------|-----------|-------------------|
| HA-M.1 | At least two-node PostgreSQL primary/standby streaming replication deployed | HA topology evidence artifact |
| HA-M.2 | Read/write routing documented and validated | Routing validation evidence |
| HA-M.3 | Replication lag measured and within acceptable bounds | Lag measurement log |
| HA-M.4 | Fencing or split-brain prevention mechanism designed and documented | Fencing design doc |

### Automated failover gates

| Gate | Criterion | Evidence required |
|------|-----------|-------------------|
| HA-A.1 | Failover occurs without manual `pg_promote` | Failover drill log |
| HA-A.2 | ferrumd reconnects to new primary without manual restart | Reconnect drill log |
| HA-A.3 | RTO and RPO measured and documented | RTO/RPO measurement log |
| HA-A.4 | No split-brain observed during or after failover | Incident log / consistency check |
| HA-A.5 | At least three failover drills performed with pass evidence | Drill evidence artifacts (×3) |

---

## Tier progression rules

1. **Tier 1 → Tier 1.5**: Engineering completes PostgreSQL production deployment, HA multi-node topology, and automated failover evidence. Operator acknowledges Tier 1.5 scope and non-claims. No real domain required.
2. **Tier 1.5 → Tier 2**: Operator procures real domain; engineering re-runs L1–L5 with real domain; sustained SLO window observed; G2 re-signed; operator final signoff obtained.
3. **No skip**: Tier 2 cannot be claimed without passing through Tier 1.5 evidence completeness (or an explicit waiver documented in an ADR).
4. **No retroactive upgrade**: Tier 1.5 attainment does not alter the status of any prior conditional signoff or Tier 1 non-claim.
5. **Final intermediate tier**: Tier 1.5 is the last intermediate tier. Any future subtier requires a written ADR and explicit operator acknowledgment before the framework is amended.

---

## Relationship to legacy terminology

- **Legacy `production-ready`**: Maps exclusively to Tier 2. It must never be used for Tier 1, Tier 1.5, or Tier 0.
- **Tier 1 `domainless production-candidate`**: Remains valid and unchanged. Tier 1.5 builds on Tier 1 by adding infrastructure-layer completeness.
- **Tier 1.5 `domainless production infrastructure complete`**: Use this exact phrase when describing Tier 1.5.

---

## Non-claims

- **Tier 1.5 is not production-ready**.
- **Tier 1.5 does not close Block A**.
- **Tier 1.5 does not complete full G2**.
- **Tier 1.5 does not validate a sustained SLO observation window**.
- **Tier 1.5 does not require or claim a real owned domain**.
- **Tier 1.5 does not replace operator final signoff**.

---

## Required new evidence artifacts (to be created when Tier 1.5 is pursued)

| Artifact | Purpose | Owner |
|----------|---------|-------|
| PostgreSQL production deployment signoff | Evidence that PG-P.1–PG-P.6 are satisfied | Engineering + Operator |
| HA multi-node topology evidence pack | Evidence that HA-M.1–HA-M.4 are satisfied | Engineering + Operator |
| Automated failover drill evidence | Evidence that HA-A.1–HA-A.5 are satisfied | Engineering + Operator |
| Tier 1.5 operator acknowledgment | Explicit operator authorization of Tier 1.5 scope and non-claims | Operator |
| Tier 1.5 complete end-state | Final declaration listing all gates satisfied and all non-claims preserved | Engineering |

---

## Related docs

- [`00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md) — Canonical tiered readiness model.
- [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md) — Scope boundaries and master non-claims table.
- [`13-tier-1.5-completion-status.md`](./13-tier-1.5-completion-status.md) — Tier 1.5 completion tracker.
- [`docs/implementation-path/01-current-state.md`](../implementation-path/01-current-state.md) — Current state with Tier 1 completion status.
- [`docs/ROADMAP.md`](../ROADMAP.md) — Milestone 0.75 (Tier 1.5 framework) and phased completion roadmap.

---

*End of file — Tier 1.5 Domainless Production Infrastructure definition. Tier 1.5 is PLANNED / NOT COMPLETE.*
