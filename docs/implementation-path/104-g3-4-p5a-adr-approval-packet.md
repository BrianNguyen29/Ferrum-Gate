# 104 — G3.4 P5a ADR Approval Packet

> **Status**: Approved for P5a design/ADR only by explicit user authorization on 2026-05-11. G3.4 is satisfied; P5b–P5e remain gated.
> **Scope**: P5a design/ADR review only. No P5b–P5e implementation authorization. No production-ready claim.
> **Constraint**: Approval recorded only from explicit user instruction in chat. Do not authorize P5 implementation.
> **Purpose**: Structured approval packet for G3.4 (ADR-50 P5a design review) per `31-release-paths-todo.md` §Path 3 Gate.

---

## Purpose

This packet provides a structured review and approval workflow for G3.4:

> **G3.4**: ADR-50 P5a design review approved — `50-p4-postgres-store-facade-adr.md` §3.5 P5a

P5a is the design/ADR phase for PostgreSQL production readiness. Approving this packet
means the approver has reviewed the P5a design scope, risks, operator decisions (D1–D6),
and verification gates, and agrees that the design is sufficient to proceed with P5a
deliverables. **It does NOT authorize P5b–P5e implementation.**

**Operator-owned**: This packet requires explicit signoff. Do not mark approved on behalf
of the approver.

---

## Explicit Non-Claims

- **No production-ready claim**: Approving P5a does NOT make FerrumGate production-ready.
- **No P5 implementation authorization**: P5b–P5e implementation remains gated on G3.5–G3.6
  and operator D1–D3 signoff.
- **No HA/multi-node authorization**: P5d HA/clustering design is explicitly out of v1 scope.
- **No PostgreSQL production deployment**: P5a is design-only; production deployment requires
  P5b–P5e completion + P6 assessment.
- **No G2 completion claim**: G2.1–G2.8 are signed for conditional single-node SQLite pilot only.
- **Do not sign on behalf of the approver**: All signature fields remain blank until explicitly signed.

---

## Prerequisites for G3.4 Review

Before reviewing this packet, confirm the following are satisfied:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | G3.1 (v1 RC tag) complete | RC tag `v0.1.0-rc.1` at `5fce844d` | ☑ DONE |
| R2 | G3.2 (conditional pilot signed) complete | `59-pilot-readiness-evidence-packet.md` signed 09/05/2026 | ☑ DONE |
| R3 | G3.3 (P1–P4.4 implementation) complete | ADR-50 §6; `cargo test --workspace --features postgres` passes | ☑ DONE |
| R4 | P5a design doc reviewed | `50-p4-postgres-store-facade-adr.md` §3.5 read and understood | ☐ Pending (reviewer) |
| R5 | Risk register reviewed | This packet §Risk Register reviewed | ☐ Pending (reviewer) |
| R6 | D1–D6 decision framework reviewed | This packet §D1–D6 Decision Preview reviewed | ☐ Pending (reviewer) |

---

## Decision Summary

### What P5a Is

P5a produces the design artifacts required before any P5b–P5e implementation begins:

| Deliverable | Location | Status |
|---|---|---|
| P5a design doc / ADR section | `50-p4-postgres-store-facade-adr.md` §3.5 | ✅ Draft complete |
| Risk register (P5-specific) | This packet §Risk Register | ✅ Draft complete |
| Verification gates (P5b–P5e) | This packet §Verification Gates | ✅ Draft complete |
| Operator decision framework (D1–D6) | This packet §D1–D6 Decision Preview | ✅ Draft complete |
| Non-claims language review | This packet §Explicit Non-Claims | ✅ Draft complete |

### What P5a Is NOT

- **NOT** P5 implementation: No code changes for pool tuning, backup/restore, HA, or migration grade-up
- **NOT** a production-ready claim: Design approval ≠ production authorization
- **NOT** an operator D1–D3 signoff: D1–D3 decisions are previewed in P5a but signed separately for P5b–P5e
- **NOT** a budget/capacity commitment: Effort estimates are planning figures only
- **NOT** a technology selection lock-in: D1–D6 defaults can be revised before P5b–P5e

---

## D1–D6 Decision Preview

P5a drafts the decision framework. **Final D1–D3 signoff for P5b–P5e occurs separately
under G3.5.** D4–D6 are engineering/operator collaborative decisions.

| Decision | Question | Previewed Default | P5b–P5e Signoff Gate |
|---|---|---|---|
| D1 | Target topology | Single-node PostgreSQL | G3.5 (operator) |
| D2 | Backup strategy | `pg_dump` logical | G3.5 (operator) |
| D3 | Failover requirement | None (single-node) | G3.5 (operator) |
| D4 | Pool sizing model | Dynamic based on pilot data | G3.6 (engineering, needs pilot data) |
| D5 | Migration grade | MVP until P5e authorized | G3.5 (engineering + operator) |
| D6 | Production claim timeline | Deferred beyond P5 | G3.5 (engineering + operator) |

**Approver acknowledgment**: I have reviewed the D1–D6 decision preview and understand
that D1–D3 will require separate signoff under G3.5 before P5b–P5e implementation begins.

---

## Risk Register

| Risk ID | Risk | Impact | Likelihood | Mitigation | Owner | Residual Risk |
|---|---|---|---|---|---|---|
| P5a-R1 | Pool exhaustion under pilot workload | Connection timeouts, throughput collapse | Medium | Size pool from G2 pilot metrics; add circuit breaker | Engineering | Low (if G3.6 data available) |
| P5a-R2 | Failover gap not modeled in design | Data loss if replication lag exceeds RPO | Medium | Define max acceptable replication lag; operator accepts risk | Operator | Medium (acceptable for single-node default) |
| P5a-R3 | Backup inconsistency during concurrent writes | Restored state diverges from expected | Low | Use PostgreSQL consistent snapshot or schedule backups during low-write windows | Operator | Low |
| P5a-R4 | Migration content-hash mismatch | SQLite → PostgreSQL lineage not equivalent | Medium | Content-hash validation planned for P5e; MVP count+ID only until then | Engineering + Operator | Medium (MVP risk accepted) |
| P5a-R5 | Operator D1–D3 not signed before P5b starts | P5b–P5e proceeds with unapproved topology/backup/failover | Low | Hard gate on G3.5; engineering lead must verify signoff before implementation | Engineering lead | Low (process control) |
| P5a-R6 | P5a approval misinterpreted as P5 implementation go-ahead | Unscoped work begins before G3.5/G3.6 satisfied | Low | Explicit non-claims in this packet; separate G3.5/G3.6 gates required | Engineering lead + Approver | Low (documentation control) |

---

## Verification Gates

P5a approval is contingent on the following verification gates passing review:

| Gate ID | Criterion | Evidence Location | Reviewer Check |
|---|---|---|---|
| P5a.V1 | D1–D6 decision framework is documented and complete | This packet §D1–D6 Decision Preview | [ ] |
| P5a.V2 | Risk register contains at least 6 P5-specific risks with mitigations | This packet §Risk Register | [ ] |
| P5a.V3 | P5b–P5e verification gates are defined with pass/fail criteria | This packet §P5b–P5e Verification Gates | [ ] |
| P5a.V4 | Non-claims language is present and reviewed | This packet §Explicit Non-Claims | [ ] |
| P5a.V5 | Cross-links to canonical docs (ADR-50, doc31, doc59) are present and accurate | This packet §Cross-References | [ ] |
| P5a.V6 | Signoff fields are present and not pre-filled | This packet §Approval Signoff | [ ] |

**All gates must be checked before signoff.**

---

## P5b–P5e Verification Gates (Preview)

These gates are defined in P5a but are **not** verified until P5b–P5e implementation:

### P5b — Pool Tuning

| Gate | Criterion | Evidence |
|---|---|---|
| P5b.V1 | Pool config validated in local Docker stress test | Benchmark ≥1000 writes/s with tuned pool |
| P5b.V2 | No connection leaks in 30-min stress test | `sqlx` pool metrics or custom detector |
| P5b.V3 | Circuit breaker triggers within 5s on exhaustion | Integration test or manual verification |

### P5c — Backup / Restore

| Gate | Criterion | Evidence |
|---|---|---|
| P5c.V1 | Backup produces consistent snapshot | `pg_dump` with snapshot or equivalent |
| P5c.V2 | Restore drill completes successfully | Operator drill log |
| P5c.V3 | RPO/RTO operator-accepted for PostgreSQL | Signed operator acknowledgment |

### P5d — HA / Clustering

| Gate | Criterion | Evidence |
|---|---|---|
| P5d.V1 | HA topology documented and operator-approved | Architecture diagram + signoff |
| P5d.V2 | Staging multi-node deployment passes integration tests | Test evidence from staging |
| P5d.V3 | Failover procedure tested in staging | Operator drill log |

### P5e — Migration Grade-Up

| Gate | Criterion | Evidence |
|---|---|---|
| P5e.V1 | Migration is idempotent (rerunnable without duplication) | Integration test with repeated runs |
| P5e.V2 | Content-hash validation passes for all migrated records | Hash comparison log |
| P5e.V3 | Large dataset (≥1M records) streams without OOM | Memory profile or benchmark evidence |

---

## Evidence References

| Document | Purpose |
|---|---|
| [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md) §3.5 P5a | Canonical P5a design doc with deliverables, decisions, risks |
| [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3 Gate | G3.1–G3.6 gate definitions and statuses |
| [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 signed conditional pilot evidence |
| [`100-phase3f-conditional-sqlite-pilot-authorization.md`](./100-phase3f-conditional-sqlite-pilot-authorization.md) | Conditional pilot authorization with signed parameters |
| [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) | Production evaluation framework to re-run after P5e |
| [`23-production-readiness-assessment.md`](./23-production-readiness-assessment.md) | RC-ready declaration; to be refreshed after P6 |

---

## Approval Signoff

> **Approver instruction**: Review all sections above, check all P5a.V1–V6 gates, and sign below.
> **Do not sign if any P5a.V gate is unchecked or any risk is unacceptable without compensating control.**
> **This approval is for P5a design only. It does NOT authorize P5b–P5e implementation.**

### Approver Information

| Field | Value |
|---|---|
| Approver name | BrianNguyen (authorized via user chat instruction) |
| Role | [x] Engineering lead / [ ] Operator / [ ] Architect / [ ] Other: _______ |
| Date | 2026-05-11 |
| Review duration | Async review; approval recorded by assistant per explicit user instruction |

### Approval Checklist

| # | Check | Status |
|---|---|---|
| A1 | I have read `50-p4-postgres-store-facade-adr.md` §3.5 P5a | [x] |
| A2 | I have reviewed the D1–D6 decision preview in this packet | [x] |
| A3 | I have reviewed the risk register (6 risks) and find mitigations acceptable | [x] |
| A4 | I have reviewed the P5b–P5e verification gates preview | [x] |
| A5 | I have reviewed the explicit non-claims and understand P5a ≠ P5 implementation | [x] |
| A6 | I understand that P5b–P5e requires separate G3.5–G3.6 signoff | [x] |
| A7 | I understand that full production-ready requires P6 assessment after P5e | [x] |

### Approval Statement

> **Select ONE:**

- [x] **APPROVED** — P5a design/ADR review is approved. P5a deliverables may proceed.
  P5b–P5e implementation remains gated on G3.5–G3.6.
- [ ] **APPROVED WITH CONDITIONS** — P5a approved subject to the following conditions:
  - Condition 1: _____________________________________________________________
  - Condition 2: _____________________________________________________________
- [ ] **DECLINED** — P5a not approved. Reason: __________________________________

### Signature

| Role | Signature | Date |
|---|---|---|
| Approver | BrianNguyen (authorized via user chat instruction; recorded by assistant) | 2026-05-11 |
| Witness (optional) | _________________________ | _________________________ |

---

## Next Steps After G3.4 Approval

Once G3.4 is approved, the next gated step is **G3.5: Operator D1–D3 signoff**.

| Step | Document | Purpose | Owner |
|---|---|---|---|
| G3.5 | [`105-g3-5-operator-d1-d3-signoff-packet.md`](./105-g3-5-operator-d1-d3-signoff-packet.md) | Operator selects D1 (topology), D2 (backup), D3 (failover) with impact analysis | Operator |
| G3.6 | Path 2 pilot metrics/logs | G2 pilot data for P5b pool-tuning input | Operator |
| Eng.1 | Engineering planning | Confirm capacity for selected topology effort | Engineering lead |

**P5b–P5e implementation remains gated until G3.5, G3.6, and Eng.1 are all satisfied.**

---

## Cross-References

| This Doc | Links To | Purpose |
|---|---|---|
| `104-g3-4-p5a-adr-approval-packet.md` | `50-p4-postgres-store-facade-adr.md` §3.5 P5a | Canonical P5a design source |
| `104-g3-4-p5a-adr-approval-packet.md` | `31-release-paths-todo.md` §Path 3 Gate | G3.4 gate definition |
| `104-g3-4-p5a-adr-approval-packet.md` | `59-pilot-readiness-evidence-packet.md` | G2 signed evidence |
| `104-g3-4-p5a-adr-approval-packet.md` | `100-phase3f-conditional-sqlite-pilot-authorization.md` | Conditional pilot authorization |
| `31-release-paths-todo.md` | This doc | G3.4 evidence reference |
| `50-p4-postgres-store-facade-adr.md` | This doc | P5a approval packet cross-reference |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | This doc | Next step after G3.4: G3.5 operator D1–D3 signoff |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-11 | Initial G3.4 P5a ADR approval packet created | Engineering |

---

*Document created: 2026-05-11. P5a design approval packet — NOT approved until signed. No production-ready claim. No P5 implementation authorization.*
