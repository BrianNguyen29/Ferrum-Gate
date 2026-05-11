# 108 — Eng.2 P5b–P5e Implementation Planning Packet

> **Status**: Approved via user chat authorization on 2026-05-11. Eng.2 is satisfied. G3.6 is conditionally accepted (BrianNguyen, 2026-05-11) for initial P5b planning. P5b may proceed ONLY with conservative defaults and post-deploy monitoring.  
> **Scope**: P5b–P5e implementation planning given D1=A/D2=A/D3=A operator selections and Eng.1 capacity confirmation.  
> **Constraint**: This packet authorizes P5b conservative-default planning ONLY. Full P5b–P5e implementation requires post-deploy monitoring validation. G3.6 conditional acceptance does NOT constitute full production workload validation.  
> **Purpose**: Structured implementation planning for Eng.2 per `31-release-paths-todo.md` §Path 3 Gate and `105-g3-5-operator-d1-d3-signoff-packet.md` §Prerequisites for P5b–P5e Implementation.

---

## Purpose

This packet drafts the P5b–P5e implementation plan required to satisfy **Eng.2**:

> **Eng.2**: P5b–P5e implementation plan drafted per D1–D3 selections.

The plan is constrained by:
- **D1=A**: Single-node PostgreSQL — no HA, no read replica, no clustering
- **D2=A**: `pg_dump` logical backup — external scheduler, operator-owned
- **D3=A**: None/manual recovery — no failover automation

**P5d is explicitly skipped** for D1=A/D3=A.

**Engineering-owned**: This packet requires explicit engineering lead review.
Do not mark approved without engineering lead confirmation.

---

## Explicit Non-Claims

- **No production-ready claim**: Implementation planning does NOT make FerrumGate production-ready.
- **No P5 implementation authorization**: P5b–P5e remain gated on G3.6 (pilot data) and Eng.1 (capacity confirmation).
- **No HA/multi-node authorization**: D1=A/D3=A explicitly excludes HA/clustering from this plan.
- **No PostgreSQL production deployment**: Planning is design-only; production deployment requires P5b–P5e completion + P6 assessment.
- **No schedule commitment**: Implementation timeline depends on G3.6 availability and engineering bandwidth.
- **Approved per explicit user instruction**: Approval was recorded by assistant per user chat authorization on 2026-05-11.

---

## Prerequisites

Before drafting/reviewing this plan, confirm the following:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | G3.4 (P5a design) approved | `104-g3-4-p5a-adr-approval-packet.md` signed | ☑ DONE |
| R2 | G3.5 (operator D1–D3) signed with A/A/A | `105-g3-5-operator-d1-d3-signoff-packet.md` signed | ☑ DONE (Option A/A/A via chat authorization on 2026-05-11) |
| R3 | Eng.1 capacity confirmed | `107-eng-1-capacity-confirmation-packet.md` signed | ☑ DONE (via chat authorization on 2026-05-11) |
| R4 | G3.6 pilot data available | `106-g3-6-pilot-metrics-evidence-packet.md` A1–A5 met with caveats; A6 conditionally accepted (BrianNguyen, 2026-05-11) | ☑ CONDITIONALLY ACCEPTED (operator) |

---

## P5b — Connection Pool Tuning (Implementation Gated)

### Status
**Blocked until**: G3.6 pilot data available; Eng.1 capacity confirmed.

### Scope (D1=A)
Single-node PostgreSQL requires bounded connection pool tuning only. No read replica routing. No multi-node concurrency model.

### Implementation Plan

| # | Task | Estimated LOC | Owner | Dependencies |
|---|---|---|---|---|
| P5b.1 | Review G3.6 pilot metrics (sustained write rate, connection patterns, queue depth) | 0 (analysis) | Engineering | G3.6 |
| P5b.2 | Define `max_connections` based on `concurrent_client_connections_peak` from G3.6 | ~20 LOC (config) | Engineering | G3.6 |
| P5b.3 | Define `min_idle` and `acquire_timeout` based on p50/p95 latency from G3.6 | ~20 LOC (config) | Engineering | G3.6 |
| P5b.4 | Implement connection-leak detection (pool metrics export or health check) | ~50 LOC | Engineering | P5b.2–P5b.3 |
| P5b.5 | Implement circuit-breaker on pool exhaustion (fail-fast with 503) | ~50–100 LOC | Engineering | P5b.4 |
| P5b.6 | Local Docker stress test with tuned pool (benchmark ≥1000 writes/s) | ~20 LOC (test) | Engineering | All above |
| P5b.7 | Update ADR-50 or operator runbook with tuned config values | ~10 LOC (docs) | Engineering | P5b.6 |

**Total P5b estimate**: ~170–220 LOC (within D1=A/D2=A/D3=A ~200–400 LOC budget)

### Verification Gates

| Gate | Criterion | Evidence |
|---|---|---|
| P5b.V1 | Pool config validated in local Docker stress test | Benchmark ≥1000 writes/s with tuned pool |
| P5b.V2 | No connection leaks in 30-min stress test | `sqlx` pool metrics or custom detector |
| P5b.V3 | Circuit breaker triggers within 5s on pool exhaustion | Integration test or manual verification |

---

## P5c — Backup / Restore for PostgreSQL (Implementation Gated)

### Status
**Blocked until**: Eng.1 capacity confirmed; Eng.2 plan approved; P5b design complete.

### Scope (D2=A)
`pg_dump` logical backup with external scheduler. Operator-owned scheduling and retention. No streaming replication. No external tools.

### Implementation Plan

| # | Task | Estimated LOC | Owner | Dependencies |
|---|---|---|---|---|
| P5c.1 | Document `pg_dump` backup procedure for operator runbook | ~20 LOC (docs) | Engineering + Operator | Eng.2 |
| P5c.2 | Document `pg_restore` restore drill procedure | ~20 LOC (docs) | Engineering + Operator | P5c.1 |
| P5c.3 | Define RPO/RTO targets for PostgreSQL (operator-accepted) | ~10 LOC (docs) | Operator | P5c.1–P5c.2 |
| P5c.4 | Add `pg_dump` snapshot option guidance to runbook (`--snapshot` for consistency) | ~10 LOC (docs) | Engineering | P5c.1 |
| P5c.5 | Provide example cron/systemd timer config for external scheduler | ~20 LOC (config example) | Engineering | P5c.1 |

**Total P5c estimate**: ~80 LOC (docs + config examples; within D1=A/D2=A/D3=A budget)

### Verification Gates

| Gate | Criterion | Evidence |
|---|---|---|
| P5c.V1 | Backup produces consistent snapshot | `pg_dump` with `--snapshot` or equivalent; integrity verified |
| P5c.V2 | Restore drill completes successfully | Operator drill log with restored DB verification |
| P5c.V3 | RPO/RTO operator-accepted for PostgreSQL | Signed operator acknowledgment |

---

## P5d — HA / Clustering Design

### Status
**SKIPPED** for D1=A/D3=A.

### Rationale
- D1=A selects Single-node PostgreSQL (no read replica, no clustering)
- D3=A selects None/manual recovery (no failover)
- P5d is explicitly out of v1 scope per `50-p4-postgres-store-facade-adr.md` §3.5.3

### If Operator Revises D1/D3 Later

| Trigger | Required Action |
|---|---|
| Operator selects D1=B (Read Replica) or D1=C (HA Cluster) | New G3.5 signoff required; P5d scope activated; effort increases by ~200–400 LOC |
| Operator selects D3=B (Manual Failover) or D3=C (Automated Failover) | New G3.5 signoff required; P5d scope activated; effort increases by ~50–400 LOC |

> **Note**: Any D1/D3 revision requires a new operator signoff packet. Do not begin P5d without formal G3.5 refresh.

---

## P5e — Migration Grade-Up (Implementation Gated)

### Status
**Blocked until**: Eng.1 capacity confirmed; Eng.2 plan approved; P5b–P5c design complete; P4.4 MVP baseline available.

### Scope
Upgrade P4.4 MVP migration CLI (`bins/ferrum-migrate`) to production-grade.
P4.4 MVP already implements: dry-run default, `--apply`, empty-target safety, count+ID validation.

### Implementation Plan

| # | Task | Estimated LOC | Owner | Dependencies |
|---|---|---|---|---|
| P5e.1 | Add upsert/resume support (idempotent re-run without duplication) | ~50 LOC | Engineering | P4.4 MVP |
| P5e.2 | Add checkpointing (save migration progress, resume from last checkpoint) | ~50 LOC | Engineering | P5e.1 |
| P5e.3 | Add content-hash validation for lineage equivalence (SHA-256 per record) | ~50 LOC | Engineering | P5e.1 |
| P5e.4 | Add large-dataset streaming (chunked read/write to avoid OOM) | ~50 LOC | Engineering | P5e.1 |
| P5e.5 | Integration tests for repeated runs, hash validation, and large dataset | ~50 LOC (tests) | Engineering | P5e.2–P5e.4 |

**Total P5e estimate**: ~250 LOC (incremental upgrade from P4.4 MVP; if this exceeds Eng.1 budget, defer P5e or split into post-P5b/P5c phase)

### Verification Gates

| Gate | Criterion | Evidence |
|---|---|---|
| P5e.V1 | Migration is idempotent (rerunnable without duplication) | Integration test with repeated runs |
| P5e.V2 | Content-hash validation passes for all migrated records | Hash comparison log |
| P5e.V3 | Large dataset (≥1M records) streams without OOM | Memory profile or benchmark evidence |

---

## Combined Implementation Summary

| Phase | Status | LOC Estimate | Blocked Until | Owner |
|---|---|---|---|---|
| P5b — Pool tuning | Deferred | ~170–220 | G3.6; Eng.1; Eng.2 | Engineering |
| P5c — Backup/restore | Deferred | ~80 | Eng.1; Eng.2; P5b | Engineering + Operator |
| P5d — HA/clustering | **Skipped** | ~0 | D1=A/D3=A | N/A |
| P5e — Migration grade-up | Deferred | ~250 | Eng.1; Eng.2; P5b–P5c | Engineering |
| **Total P5b–P5e (D1=A/D2=A/D3=A)** | | **~500–550** | | |

> **Budget check**: The Combined Decision Impact Matrix estimates ~200–400 LOC for D1=A/D2=A/D3=A. P5e (~250 LOC) may push total above budget. Engineering lead should decide whether to:
> 1. Accept ~500–550 LOC total
> 2. Defer P5e to a post-P5b/P5c phase
> 3. Reduce P5e scope (e.g., skip content-hash validation, keep count+ID only)

---

## Implementation Gates (Before Starting P5b–P5e)

All of the following must be satisfied before any P5b–P5e implementation begins:

| Gate | Criterion | Owner | Status |
|---|---|---|---|
| G3.5 | D1–D3 signed (A/A/A) | Operator | ☑ DONE |
| G3.6 | Pilot data available for P5b pool tuning | Operator | ☐ Pending |
| Eng.1 | Engineering capacity confirmed | Engineering lead | ☑ DONE (via chat authorization on 2026-05-11) |
| Eng.2 | This implementation plan approved | Engineering lead | ☑ DONE (via chat authorization on 2026-05-11) |
| P5a.V1–V4 | P5a design verification gates passed | Engineering lead | ☑ DONE |

**Do not begin P5b–P5e implementation until all gates above are satisfied.**

---

## Risk Register (Implementation Planning)

| Risk ID | Risk | Trigger | Impact | Mitigation | Owner |
|---|---|---|---|---|---|
| IP-R1 | P5b pool tuning requires more LOC than estimated | G3.6 shows high concurrency or unusual patterns | Budget overrun | Cap P5b at 220 LOC; defer advanced circuit-breaker features | Engineering lead |
| IP-R2 | P5e migration grade-up scope creep | Operator expects full production-grade migration immediately | Effort exceeds budget | Clearly define P5e as incremental upgrade; defer content-hash if needed | Engineering + Operator |
| IP-R3 | Operator requests D1/D3 revision mid-implementation | Workload changes or new requirements | P5d activation; schedule slip | Hard gate: new G3.5 signoff required; do not scope-creep without formal signoff | Engineering lead |
| IP-R4 | G3.6 pilot data delayed | Operator cannot collect metrics | P5b blocked indefinitely | Set G3.6 deadline; if missed, evaluate whether to proceed with conservative defaults or defer P5b | Engineering + Operator |
| IP-R5 | P5c operator runbook not reviewed | Operator does not validate backup/restore docs | P5c incomplete | Require operator acknowledgment as P5c.V3 gate | Operator |

---

## Engineering Lead Approval

> **Engineering lead instruction**: Review all sections above, confirm the plan is realistic for D1=A/D2=A/D3=A, check the budget/scope tradeoffs, and sign below.  
> **Do not approve if any phase scope is unclear or any risk is unacceptable without compensating control.**  
> **This approval does NOT authorize P5b–P5e implementation.** G3.6 is still required.

### Engineering Lead Information

| Field | Value |
|---|---|
| Name | BrianNguyen |
| Role | Engineering lead / Architect |
| Date | 11/05/2026 |
| Review duration | _________________________ |

### Approval Checklist

| # | Check | Status |
|---|---|---|
| A1 | I have reviewed P5b scope and find it realistic for single-node PostgreSQL | [x] |
| A2 | I have reviewed P5c scope and find it realistic for `pg_dump` logical backup | [x] |
| A3 | I have reviewed P5d skip rationale and agree D1=A/D3=A excludes HA/clustering | [x] |
| A4 | I have reviewed P5e scope and accept the budget/scope tradeoff | [x] |
| A5 | I have reviewed the Combined Implementation Summary and total LOC estimate | [x] |
| A6 | I have reviewed the Risk Register (5 risks) and find mitigations acceptable | [x] |
| A7 | I understand that Eng.2 approval does NOT authorize P5b–P5e implementation (G3.6 is the remaining gate) | [x] |
| A8 | I understand that full production-ready requires P5b–P5e completion + P6 assessment | [x] |

### Approval Statement

> **Select ONE:**

- [x] **APPROVED** — P5b–P5e implementation plan is approved for D1=A/D2=A/D3=A. P5b–P5e implementation remains gated on G3.6.
- [ ] **APPROVED WITH CONDITIONS** — Plan approved subject to the following conditions:
  - Condition 1: _____________________________________________________________
  - Condition 2: _____________________________________________________________
- [ ] **DECLINED** — Plan not approved. Reason: __________________________________

### Signature

| Role | Signature | Date |
|---|---|---|
| Engineering Lead | BrianNguyen (authorized via user chat instruction; recorded by assistant) | 2026-05-11 |
| Operator (acknowledgment of plan) | _________________________ | _________________________ |
| Witness (optional) | _________________________ | _________________________ |

---

## Cross-References

| This Doc | Links To | Purpose |
|---|---|---|
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 prerequisite; D1=A/D2=A/D3=A selections |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | `50-p4-postgres-store-facade-adr.md` §3.5 P5b–P5e | ADR-50 P5b–P5e verification gates and scope |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 prerequisite (P5b blocked until available) |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | `107-eng-1-capacity-confirmation-packet.md` | Eng.1 prerequisite (capacity confirmation) |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | `104-g3-4-p5a-adr-approval-packet.md` | G3.4 prerequisite (P5a design approved) |
| `31-release-paths-todo.md` | This doc | Eng.2 evidence reference |
| `50-p4-postgres-store-facade-adr.md` | This doc | Eng.2 planning packet cross-reference |
| `107-eng-1-capacity-confirmation-packet.md` | This doc | Next step after Eng.1 |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-11 | Initial Eng.2 P5b–P5e implementation planning packet drafted | Engineering |
| 2026-05-11 | Eng.2 approved via user chat authorization — plan approved for D1=A/D2=A/D3=A | Assistant (recorded per user instruction) |

---

*Document created: 2026-05-11. Eng.2 implementation planning packet — APPROVED via user chat authorization on 2026-05-11. P5d skipped for D1=A/D3=A. G3.6 conditionally accepted (BrianNguyen, 2026-05-11). No production-ready claim. P5b may proceed ONLY with conservative defaults and post-deploy monitoring.*
