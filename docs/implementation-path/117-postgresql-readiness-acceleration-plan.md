# 117 — PostgreSQL Readiness Acceleration Plan

> **Status**: Planning artifact. No execution claimed. No production-ready claim.  
> **Purpose**: Define short-term PostgreSQL readiness work that runs **after or alongside** the SQLite Path 2 conditional pilot, without claiming PostgreSQL production deployment.  
> **Scope**: Documentation, template preparation, local rehearsal, and gate definition only. No live target-host changes. No secrets.  
> **Constraint**: `production-ready = NO`. `HA/multi-node = NO`. SQLite Path 2 remains the selected near-term pilot per [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md). PostgreSQL is a **readiness-only** track.

---

## 1. Context & Baseline

### 1.1 Current State

| Item | State |
|---|---|
| SQLite Path 2 pilot | **Selected** — Option A in doc113 (2026-05-12) |
| PostgreSQL local runtime | Implemented (P1–P4.4 complete per [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md)) |
| P5b pool tuning | **Partially implemented** — conservative defaults wired; validation against real workload deferred |
| P5c backup/restore | **Design/docs complete** — local smoke passed; target-host drills **not executed** |
| P5e migration grade-up | **Implemented** (resume, checkpointing, hash validation, chunking) — exercised locally only |
| G3.6 pilot metrics | **Conditionally accepted** — compile-only/light workload; real workload validation pending |
| Production-ready claim | **NO** |
| Target-host PostgreSQL | **NO** |

### 1.2 Why Readiness Matters While SQLite Is Selected

Even though Option A (SQLite) is the active pilot, engineering maintains a **parallel readiness track** so that:

1. If Path 2 pilot workload exceeds the ~300 writes/s ceiling, the operator can revisit Option B with minimal delay.
2. P5b–P5e gaps are incrementally closed with evidence, not rushed under pressure.
3. G3.6 data collected during the SQLite pilot can be reused for PostgreSQL design inputs.
4. Migration tooling (P5e) does not rot due to schema drift.

### 1.3 Explicit Non-Claims

- **No production-ready claim**: This plan does NOT make FerrumGate production-ready.
- **No PostgreSQL production deployment**: No target-host PostgreSQL deployment is authorized by this document.
- **No HA/multi-node**: Single-node scope only. P5d remains skipped.
- **No target-host blocker closure**: All target-host evidence remains operator-owned and is not closed by this plan.
- **No alteration of doc113 path selection**: Option A (SQLite) remains selected. This plan is a **readiness buffer**, not a path switch.
- **No secret values**: No passwords, tokens, or full DSNs are recorded.
- **No fabricated evidence**: All evidence items are planned or rehearsed locally only.

---

## 2. Readiness Tracks

### Track A — Target-Host P5c Rehearsal Preparation

> **Purpose**: Prepare templates and adapted plans so that target-host P5c.V1/V2 can begin within one business day if the operator ever revisits Option B.

| # | Task | Owner | Status | Evidence |
|---|---|---|---|---|
| A.1 | Adapt [`111-p5c-local-docker-drill-plan.md`](./111-p5c-local-docker-drill-plan.md) for generic target host: replace `localhost:5432` with placeholders, add `.pgpass` / `PGPASSFILE` guidance, add scheduler verification checklist | Engineering | ☐ Ready to start | Adapted plan document |
| A.2 | Prepare environment-variable-only configuration template for target host (no hardcoded credentials) | Engineering | ☐ Ready to start | `configs/examples/postgres-target-env.template` (placeholders only) |
| A.3 | Document minimum `pg_dump`/`pg_restore` version compatibility matrix (client vs server) | Engineering | ☐ Ready to start | Compatibility table in this doc or ADR-50 |
| A.4 | Draft one-page "P5c target-host go/no-go" checklist for operator self-assessment before executing drills | Engineering | ☐ Ready to start | Checklist markdown |
| A.5 | Review [`114-target-host-p5c-drill-checklist.md`](./114-target-host-p5c-drill-checklist.md) for outdated commands or schema references; refresh if needed | Engineering | ☐ Ready to start | Refresh commit |

**Stop condition**: If populated local drill (Track 2 of doc112) reveals a migration or schema bug, **pause** Track A until the bug is fixed — no point in preparing target-host drills for a broken baseline.

---

### Track B — P5b Validation Gap Closure

> **Purpose**: Move P5b from "conservative defaults wired" to "defaults validated against realistic workload".

| # | Task | Owner | Status | Evidence |
|---|---|---|---|---|
| B.1 | Document conservative-default rationale and known limitations (`max_connections=10`, `min_idle=2`, `acquire_timeout=5s`) | Engineering | ☐ Ready to start | Doc update in ADR-50 or new appendix |
| B.2 | Define pool-exhaustion escalation thresholds: at what queue depth / acquire latency should operator consider increasing `max_connections`? | Engineering | ☐ Ready to start | Threshold table |
| B.3 | Validate conservative defaults in **populated local Docker** stress test (same test as Track 2 migration, but against PG directly) | Engineering | ☐ Blocked on Track 2 (doc112) | Benchmark log |
| B.4 | If local stress test shows >5% connection acquire failures at target load, document mitigation options (increase pool, enable circuit breaker, or recommend PG path) | Engineering | ☐ Blocked on B.3 | Analysis doc |
| B.5 | Circuit-breaker design: draft behavior spec (trip threshold, half-open retry, fallback) — **design only**, implementation gated on G3.6 | Engineering | ☐ Ready to start | Design paragraph or ADR appendix |

**Key gap**: P5b cannot be fully validated without G3.6 real workload data. Track B runs in parallel with G3.6 collection.

---

### Track C — G3.6 Data Needs for PostgreSQL Design

> **Purpose**: Extract PostgreSQL-relevant design inputs from the Path 2 pilot G3.6 real workload execution, even though the pilot runs on SQLite.

| # | Data Need | Source | PostgreSQL Design Input | Status |
|---|---|---|---|---|
| C.1 | Sustained write rate (p50/p95/p99) | Path 2 pilot load generator | If rate >250/s, P5b pool size should target ≥500/s headroom | ☐ Blocked on Path 2 pilot |
| C.2 | Peak connection concurrency observed | SQLite is single-threaded; use adapter execution parallelism as proxy | `max_connections` lower bound for PG pool | ☐ Blocked on Path 2 pilot |
| C.3 | Queue depth behavior under spike | `ferrumgate_write_queue_depth` metrics | PG write-queue adaptation strategy (batching vs streaming) | ☐ Blocked on Path 2 pilot |
| C.4 | Adapter mix % (FS, Git, HTTP, SQLite, Maildraft) | Load generator config | Which repos are hottest; informs repo-level pool partitioning if ever needed | ☐ Blocked on Path 2 pilot |
| C.5 | `readyz/deep` failure modes under load | Probe logs | PG health-check query design (lightweight `SELECT 1` vs full store probe) | ☐ Blocked on Path 2 pilot |
| C.6 | Backup duration and size at representative row count | `ferrumctl backup create` timing | PG `pg_dump` parallelism and compression expectations | ☐ Blocked on Path 2 pilot |

**Note**: C.1–C.6 are collected during Path 2 pilot execution per [`116-g36-monitoring-execution-plan.md`](./116-g36-monitoring-execution-plan.md). Engineering reviews the data and produces a one-page "PostgreSQL Design Input" summary. No additional target-host work is required.

---

### Track D — Migration Rehearsal Cadence

> **Purpose**: Repeat populated local migration drills on a regular cadence to prevent P5e tooling rot and build operator confidence.

| # | Task | Owner | Cadence | Status |
|---|---|---|---|---|
| D.1 | Re-run populated SQLite → local PostgreSQL migration using `ferrum-migrate --features postgres` | Engineering | Weekly during active pilot development | ☐ Ready to start |
| D.2 | Exercise `--resume` path (P5e.1): migrate half, interrupt, resume, verify idempotency | Engineering | Bi-weekly | ☐ Ready to start |
| D.3 | Exercise content-hash validation (P5e.3): verify `source_content_hash == target_content_hash` | Engineering | Weekly | ☐ Ready to start |
| D.4 | Exercise large-dataset streaming (P5e.4): use chunk-size 1000, verify memory stays bounded | Engineering | Monthly | ☐ Ready to start |
| D.5 | Record rehearsal outcomes in `artifacts/` with date-stamped evidence | Engineering | Per run | ☐ Ready to start |
| D.6 | If rehearsal fails, treat as P1 bug: stop, investigate, fix migration or schema | Engineering | Ad hoc | ☐ Ready to start |

**Acceptance criteria for each rehearsal**:
- Migration exits 0
- Row counts match (±0)
- Content-hash validation passes (if exercised)
- No secrets in log output

---

### Track E — Explicit Gates Before Future PostgreSQL Production Consideration

> **Purpose**: Define the exact checklist that must be satisfied before engineering or operator can even **propose** moving FerrumGate to PostgreSQL in production. These gates are **not** claims that they will be satisfied; they are prerequisites for a future decision discussion.

| Gate | Criterion | Why It Blocks Production Consideration | Current Status |
|---|---|---|---|
| **E.1** | Operator revisits [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) and formally selects Option B | Without explicit path switch, PostgreSQL remains readiness-only | ☐ Awaiting operator action |
| **E.2** | P5c.V1 target-host backup drill complete per [`114-target-host-p5c-drill-checklist.md`](./114-target-host-p5c-drill-checklist.md) | Validates that operator can produce consistent backups on real infrastructure | ☐ Blocked on E.1 + target host |
| **E.3** | P5c.V2 target-host restore drill complete with matching row counts | Validates that operator can recover from backup within RTO | ☐ Blocked on E.2 |
| **E.4** | P5b pool tuning validated against **real workload** (not compile-only) | Conservative defaults are unvalidated for production load | ☐ Blocked on G3.6 full acceptance |
| **E.5** | G3.6 **full** acceptance (not conditional) with adapter paths exercised | P5b design depends on real metrics; conditional acceptance is insufficient | ☐ Blocked on Path 2 pilot |
| **E.6** | P5e migration tested end-to-end on a DB with ≥1M rows (or operator-accepted smaller threshold) | MVP tests are small; production migration risk is unquantified | ☐ Blocked on large-dataset benchmark |
| **E.7** | P6 assessment executed and CONDITIONAL GO (or better) recorded | P5 completion alone does not authorize production deployment | ☐ Blocked on P5b–P5e completion |
| **E.8** | Operator D1–D3 refreshed for PostgreSQL (not SQLite) | Topology, backup strategy, and failover posture must be PG-specific | ☐ Blocked on E.1 |

**Important**: Even if all gates E.1–E.8 are satisfied, this plan **still does not claim production-ready**. A separate production-readiness review (P6+) is required.

---

## 3. Priorities & Sequencing

### Immediate (This Sprint)

1. **A.1–A.2** — Prepare target-host drill templates (low risk, high leverage if path switches).
2. **B.1–B.2** — Document conservative-default rationale and escalation thresholds.
3. **D.1** — First populated local migration rehearsal (validates P5e tooling against current schema).

### Parallel With Path 2 Pilot

4. **C.1–C.6** — Extract PostgreSQL design inputs from G3.6 real workload data as it becomes available.
5. **D.2–D.5** — Maintain weekly/bi-weekly migration rehearsal cadence.

### Deferred Until Operator Action

6. **E.1–E.8** — No target-host or production-consideration work until operator explicitly revisits doc113.

---

## 4. Risk Register

| ID | Risk | Likelihood | Impact | Mitigation | Owner |
|---|---|---|---|---|---|
| R1 | Migration rehearsal cadence stops due to engineering priority shifts | Medium | Medium — P5e tooling may drift from schema | Block time in sprint; make D.1 part of CI smoke | Engineering |
| R2 | Path 2 pilot never produces G3.6 real workload data | Medium | High — P5b remains unvalidated | Set explicit G3.6 deadline in pilot plan; if missed, document risk acceptance | Operator + Engineering |
| R3 | Operator assumes readiness track = authorization to deploy PG | Low | High — premature production deployment | Repeat non-claims in every doc; require explicit E.1 gate | Engineering |
| R4 | Conservative defaults are misinterpreted as production-safe | Medium | Medium — pool exhaustion under real load | B.2 escalation thresholds must be explicit and operator-visible | Engineering |
| R5 | Rehearsal data is mistaken for target-host evidence | Low | High — false confidence in backup/restore | Label all rehearsal artifacts "LOCAL ONLY"; require E.2/E.3 for target host | Engineering |

---

## 5. Cross-References

| This Plan | Links To | Purpose |
|---|---|---|
| `117-postgresql-readiness-acceleration-plan.md` | [`50-p4-postgres-store-facade-adr.md`](./50-p4-postgres-store-facade-adr.md) | PostgreSQL phased implementation status |
| `117-postgresql-readiness-acceleration-plan.md` | [`112-post-p5c-completion-execution-plan.md`](./112-post-p5c-completion-execution-plan.md) | Path 2 pilot execution context |
| `117-postgresql-readiness-acceleration-plan.md` | [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) | Selected path (Option A) and waiver |
| `117-postgresql-readiness-acceleration-plan.md` | [`114-target-host-p5c-drill-checklist.md`](./114-target-host-p5c-drill-checklist.md) | Target-host drill template (Track A) |
| `117-postgresql-readiness-acceleration-plan.md` | [`116-g36-monitoring-execution-plan.md`](./116-g36-monitoring-execution-plan.md) | G3.6 data collection (Track C) |
| `117-postgresql-readiness-acceleration-plan.md` | `111-p5c-local-docker-drill-plan.md` | Local drill basis for target adaptation |
| `117-postgresql-readiness-acceleration-plan.md` | `109-p5c-postgresql-backup-restore-runbook.md` | P5c commands and RPO/RTO |
| `117-postgresql-readiness-acceleration-plan.md` | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 baseline and conditional acceptance |

---

## 6. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial PostgreSQL readiness acceleration plan | Engineering |

---

*Document created: 2026-05-12. PostgreSQL Readiness Acceleration Plan — planning artifact only. No execution claimed. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim. SQLite Path 2 remains selected pilot.*
