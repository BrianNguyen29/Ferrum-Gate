# 52 — D6 Priority List for Extension

> **Status**: Completed 2026-04-28
> **Phase**: Phase 3 — Extension Planning
> **Scope**: FerrumGate v1 single-node SQLite (RC-ready/conditional)
> **Constraint**: This document is planning-only. It does not claim production-ready status, does not authorize implementation, and does not represent an implemented feature.

---

## Purpose

This document is D6 of the Phase 3 feature audit defined in `45-current-feature-audit.md`. It ranks the extension items that were identified as gaps in D5 (`51-d5-bottleneck-analysis-report.md`) and categorizes them by urgency and scope. D6 is the direct output of D5 bottleneck analysis and defines the extension roadmap for moving beyond single-node SQLite.

This document is **planning-only**. Its existence does not authorize code implementation, does not claim production readiness, and does not create a commitment to any specific phase. See §Non-Goals and §Production Posture.

---

## Source Inputs

D6 is derived from the following documents:

| Source | Role |
|--------|------|
| `51-d5-bottleneck-analysis-report.md` | Canonical bottleneck analysis — all 10 domains mapped; D6 ranks and categorizes the extension items identified in D5 |
| `45-current-feature-audit.md` | Phase 3 D5/D6 audit plan; this document fulfills D6 |
| `50-p4-postgres-store-facade-adr.md` | ADR-50 — PostgreSQL deferred implementation plan (P1–P5 phases); canonical reference for PostgreSQL scope |
| `33-feature-completion-backlog.md` | Must/Should/Production-only categorization of all incomplete/partial features; D6 aligns with and extends this categorization |
| `27-production-evaluation-plan.md` | Production evaluation framework; Dimension 1 (Performance) and Dimension 3 (Reliability) define the constraints that D6 extensions address |
| `31-release-paths-todo.md` | Release paths (RC tag / Path 2 pilot / Path 3 Phase 3); D6 items are the inputs to Path 3 entry decision |
| `18-single-node-operations-runbook.md` | Operations runbook including backup/restore limits referenced in D5 Domain 6 |

---

## Priority Table

Items are ranked by urgency for production-scale deployment. Urgency is derived from D5 bottleneck evidence and production evaluation framework constraints.

### Must Have — Required for Production Scale

These items are required before FerrumGate can serve production workloads that exceed single-node SQLite capacity or require multi-node topology.

| Priority | Item | D5 Domain | Evidence | Phase |
|----------|------|-----------|----------|-------|
| **1** | PostgreSQL StoreFacade | Domain 1 (SQLite single-writer ceiling), Domain 10 (PostgreSQL migration path deferred) | ~300 writes/s sustained ceiling; ADR-50 Phase P1–P5 plan exists; ~2000–3000 LOC estimated | Phase 3 |
| **2** | Multi-node / HA architecture | Domain 1, Domain 10 | Single-node only; no HA, no read-replica; scale-out requires architectural extension | Phase 3+ |

### Should Have — Pilot Hardening

These items improve production posture and operational confidence but are not blockers for pilot-scale single-node deployment.

| Priority | Item | D5 Domain | Evidence | Phase |
|----------|------|-----------|----------|-------|
| **3** | Backup automation / retention / encryption | Domain 6 (Backup/restore limits) | Bounded SQLite-only backup with opt-in retention pruning (`--retention-days N`); no scheduling or encryption in v1; S3 in `33-feature-completion-backlog.md` | Pilot (operator-owned) or Phase 3+ |
| **4** | Observability / metrics | Domain 9 (Health check depth), Dimension 4 §4.4 (Operational monitoring baseline) | Bounded `/v1/metrics` endpoint implemented with health/metrics counters, store up/down gauge, bounded governance error counters (`ferrumgate_governance_errors_total`), and public endpoint latency histograms (`ferrumgate_request_duration_seconds` for `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`); governance route latency, WAL/page gauges, and pool saturation metrics remain future/deferred work | Pilot or Phase 3+ |
| **5** | Adapter hardening | Domain 4 (Adapter compensation non-uniformity), Domain 5 (Adapter surface boundedness) | P3–P6 in `33-feature-completion-backlog.md`; compensation evidence matrix exists in `56-adapter-compensation-evidence-matrix.md`, but guarantees remain non-uniform by adapter/action | Pilot or Phase 3+ |

### Deferred — Explicit Phase 3+

These items are out of scope for v1 and pilot phases. They are tracked explicitly so they are not misread as v1 gaps.

| Priority | Item | D5 Domain | Evidence | Phase |
|----------|------|-----------|----------|-------|
| **6** | Rate-limit / load test suite | Domain 8 (Rate limiting under load) | M2 in `33-feature-completion-backlog.md`; rate limiting implemented but no dedicated stress/load test suite; LOW risk | Phase 3+ (deferred, not blocking) |

---

## Decision Gates

The following gates govern when items move from "planned" to "authorized for implementation."

### Gate 1 — RC Tag Cut (Path 1)

D6 items are **not authorized** before the RC tag is cut. The RC tag is the prerequisite for any Phase 3 work.

**Gate criterion**: RC tag exists with release notes explicitly stating single-node SQLite scope and deferred Phase 3.

### Gate 2 — Pilot Evaluation Complete (Path 2)

Items ranked Should Have (priorities 3–5) may be considered during a Path 2 pilot if the operator determines they are required for the target workload. However:

- They remain **operator-owned external concerns** in v1 architecture (per `33-feature-completion-backlog.md` S3)
- Built-in scheduling and encryption (priority 3) are explicitly documented as **not in v1 scope**; opt-in retention pruning (`--retention-days N`) is implemented
- Implementation of Should-Have items does not expand the v1 support contract

### Gate 3 — Phase 3 Entry (Path 3)

Priorities 1–2 (Must Have) require Path 3 entry. Per `31-release-paths-todo.md` §Path 3:

- G3.1: RC tag cut and Path 1 complete
- G3.2: Path 2 pilot has confirmed single-node SQLite posture is acceptable for target workload
- G3.3: Engineering capacity confirmed for ~2000–3000 LOC + migrations + container tests
- G3.4: ADR-50 Phase P1 reviewed and approved to proceed

**Do not begin Phase 3 until G3.1–G3.4 are satisfied.**

---

## Non-Goals

D6 does not:

- **Authorize implementation** of any item in this document
- **Claim production readiness** for FerrumGate v1 or any extension
- **Expand the v1 support contract** (`19-v1-single-node-support-contract.md`)
- **Commit to a specific phase timeline** — priorities are ranked but phase timing requires separate gate decisions
- **Duplicate ADR-50** — ADR-50 contains the detailed PostgreSQL phased implementation plan; D6 provides the prioritization context that ADR-50 assumes
- **Replace `33-feature-completion-backlog.md`** — D6 extends backlog categorization with explicit priority ranking derived from D5 bottleneck analysis

---

## Relationship to RC / Pilot / Phase 3

| Context | Status | D6 Relationship |
|---------|--------|-----------------|
| **v1 RC-ready** | Single-node SQLite RC-ready; no production claim | D6 items are out-of-scope for v1 RC; they define the extension horizon |
| **Path 2 Pilot** | Conditional production pilot with operator signoff | Priority 3 (backup automation) and priority 4 (observability) may be required as operator-owned compensating controls; priority 5 (adapter hardening) may be required if R1/R2/R3 compensation is critical for target workload |
| **Path 3 Phase 3** | Full production scale via PostgreSQL | Priorities 1–2 (PostgreSQL, multi-node) are the primary Phase 3 deliverables; priorities 3–5 may be addressed as part of Phase 3 hardening; priority 6 (rate-limit test suite) remains deferred |

---

## Production Posture Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- **No production-ready claim is made in this document.**
- D6 items are **not implemented** — their existence in this document does not change the implementation status of any feature.
- PostgreSQL full-production posture (priority 1) remains **not implemented** per ADR-50; local Docker/runtime/store support is implemented through the bounded P3/P4.1-P4.3 path, while P4.4 data migration and P5 production readiness remain deferred.
- Multi-node/HA (priority 2) is **not implemented** and not in v1 scope.
- Backup automation/retention/encryption (priority 3): opt-in retention pruning (`--retention-days N`) is **implemented**; scheduling and encryption are **not implemented** in v1; operator-owned per `18-single-node-operations-runbook.md`.
- Observability/metrics (priority 4): bounded `/v1/metrics` endpoint (health/metrics counters, store up/down gauge, bounded governance error counters, and public endpoint latency histograms (`ferrumgate_request_duration_seconds`)) is implemented; governance route latency, WAL/page gauges, and pool saturation metrics remain future/deferred work per `21-v1-single-node-observability-minimums.md`
- Adapter hardening (priority 5) refers to work documented in `33-feature-completion-backlog.md` P3–P6.
- Rate-limit/load test suite (priority 6) is documented in `33-feature-completion-backlog.md` M2 but deferred to Phase 3+.

Deployments requiring any D6 item must wait for Phase 3 implementation and must not claim production readiness until Phase 3 gates are satisfied.

Production posture is governed by:
- `27-production-evaluation-plan.md` — Production evaluation framework
- `23-production-readiness-assessment.md` — RC-ready declaration
- `19-v1-single-node-support-contract.md` — Support constraints
- `31-release-paths-todo.md` — Release paths and gate criteria
- `56-adapter-compensation-evidence-matrix.md` — Priority 5 adapter hardening evidence
- `57-workload-compensation-drill-plan.md` — Priority 5 operator drill plan
- `58-workload-compensation-drill-evidence-template.md` — Priority 5 drill evidence template
- `59-pilot-readiness-evidence-packet.md` — Priority 5 G2.1–G2.8 evidence packet
- `60-bounded-hardening-examples.md` — Priority 5 bounded hardening examples

---

## D5 Preservation

D6 does not modify or supersede D5 conclusions. All 10 bottleneck domains mapped in `51-d5-bottleneck-analysis-report.md` remain complete and authoritative:

- D5 Domain 1: SQLite single-writer throughput ceiling (~300 writes/s sustained) — **unchanged**
- D5 Domain 2: Write queue architecture (serialized, no cross-op batching) — **unchanged**
- D5 Domain 3: FK chain write amplification (6+ writes per pipeline) — **unchanged**
- D5 Domain 4: Adapter compensation non-uniformity — **unchanged**
- D5 Domain 5: Adapter surface boundedness — **unchanged**
- D5 Domain 6: Backup/restore limits (opt-in retention pruning available; no scheduling/encryption) — **unchanged**
- D5 Domain 7: Lineage query at scale — **unchanged**
- D5 Domain 8: Rate limiting under load (2 req/s sustained) — **unchanged**
- D5 Domain 9: Health check depth (shallow probes) — **unchanged**
- D5 Domain 10: PostgreSQL migration path deferred — **unchanged**

D6 ranks the extensions needed to address D5's conclusions. D5 remains the authoritative bottleneck analysis.

---

*Document completed: 2026-04-28. D6 of Phase 3 feature audit per `45-current-feature-audit.md`. Derived from D5 bottleneck analysis (`51-d5-bottleneck-analysis-report.md`). Planning-only — does not authorize implementation or claim production readiness.*
