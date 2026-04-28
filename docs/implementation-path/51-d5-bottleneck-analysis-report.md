# 51 — D5 Phase 3 Bottleneck Analysis Report

> **Status**: Completed 2026-04-28
> **Phase**: Phase 3 — Analysis Only
> **Scope**: FerrumGate v1 single-node SQLite (RC-ready/conditional)
> **Constraint**: This document is analysis-only. It does not recommend Phase 2 reimplementation, does not claim production-ready status, and does not describe PostgreSQL/multi-node implementation.

---

## Purpose

This document is D5 of the Phase 3 feature audit defined in `45-current-feature-audit.md`. It maps and analyzes the primary bottleneck domains affecting FerrumGate v1 single-node SQLite throughput and scalability. D6 (priority list for extension) is **complete** — see `52-d6-priority-expansion-list.md`.

---

## Canonical References

- `45-current-feature-audit.md` — Phase 3 D5/D6 audit plan; this document fulfills D5
- `PERFORMANCE_OPTIMIZATION_PLAN.md` — Three-phase performance plan (Phase 1 implemented, Phase 2 deferred/regressed, Phase 3 deferred)
- `40-out-of-tree-sqlite-performance-candidate.md` — Local performance findings and Phase 1 results
- `27-production-evaluation-plan.md` — Production evaluation framework (Dimension 1: Performance)
- `50-p4-postgres-store-facade-adr.md` — PostgreSQL deferred implementation plan (ADR-50)
- `33-feature-completion-backlog.md` — Must/Should/Production-only categorization of incomplete features
- `18-single-node-operations-runbook.md` — Operations runbook including backup/restore limits

---

## Bottleneck Domain 1 — SQLite Single-Writer Throughput Ceiling

### Description

SQLite's single-writer architecture imposes a hard throughput ceiling of approximately 8–10 writes/second when multiple connections compete for the write lock. This is a fundamental SQLite limitation, not a code defect.

### Evidence

| Source | Finding |
|--------|---------|
| `PERFORMANCE_OPTIMIZATION_PLAN.md` §Problem Statement | "SQLite's single-writer architecture creates a hard throughput ceiling of ~8-10 writes/s" |
| `40-out-of-tree-sqlite-performance-candidate.md` §SQLite Single-Writer Performance | Pre-write-queue stress test showed S7 (50 workers): ~8.1 req/s, p50 latency 3.26s, 0% errors (queued behind lock) |
| `27-production-evaluation-plan.md` §1.2 | "Phase 1 SQLite write queue is appropriate for: Low-to-medium write throughput (≤300 writes/s sustained)" |

### Analysis

The ceiling is a physical I/O constraint. SQLite serializes all write transactions through a single writer, regardless of connection pool size. Under high concurrency, requests queue behind the writer lock, increasing latency without increasing throughput.

**Phase 1 mitigation**: Write queue eliminates lock thrashing by serializing writes through a single dedicated writer task, reducing contention and retry churn. Measured results: ~289–305 req/s sustained throughput. See `40-out-of-tree-sqlite-performance-candidate.md` §Phase 1 results.

**Remaining constraint**: Phase 1 removes retry overhead and lock-thrashing, but does not change SQLite's single-writer serialization model. The throughput ceiling remains ~300 writes/s sustained for mixed workloads per `27-production-evaluation-plan.md` §1.2.

---

## Bottleneck Domain 2 — Write Queue Architecture

### Description

The Phase 1 write queue uses an in-process `tokio::sync::mpsc` channel with a single dedicated writer task. All write operations (INSERT, UPDATE) from handlers route through this channel and are processed sequentially.

### Evidence

| Source | Finding |
|--------|---------|
| `PERFORMANCE_OPTIMIZATION_PLAN.md` §P1.1 | Write-queue replaces N competing connections with 1 dedicated writer task via `mpsc::Sender<WriteOp>` |
| `40-out-of-tree-sqlite-performance-candidate.md` §Phase 1 | "Phase 1 did NOT implement true cross-operation transaction batching. Writes are serialized through a single writer task, but each write is its own transaction." |
| `27-production-evaluation-plan.md` §3.2 | "Phase 1 uses an in-process `mpsc` write queue with 20-connection pool, 5000ms busy_timeout, WAL mode, and PRAGMA tuning" |

### Analysis

The write queue correctly eliminates SQLITE_BUSY errors by ensuring only one write executes at a time. Each write operation is its own transaction (no cross-operation batching). The queue is bounded (channel capacity 256) and provides backpressure when full.

**Non-obvious constraint**: The queue serializes writes but does not batch them. A pipeline execution requiring 6 sequential writes still acquires/releases the write lock 6 times. Phase 2 attempted explicit transaction batching but regressed and was deferred. See `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 2.

---

## Bottleneck Domain 3 — FK Chain Write Amplification

### Description

The cascading FK chain (`intents → proposals → capabilities → executions → rollback_contracts`) requires multiple sequential writes per execution pipeline. Each parent record must be inserted before its children due to referential integrity constraints.

### Evidence

| Source | Finding |
|--------|---------|
| `PERFORMANCE_OPTIMIZATION_PLAN.md` §Problem Statement | "cascading FK chain (`intents → proposals → capabilities → executions → rollback_contracts`), this produces: S5 execution-pipeline: ~1.1 req/s, ~73% error rate" |
| `27-production-evaluation-plan.md` §3.1 | "The FK chain (`intents → proposals → capabilities → executions → rollback_contracts`) is enforced synchronously. All FK parent inserts use the write queue and return 200 only after persistence." |
| `45-current-feature-audit.md` §Nhóm 2 | Lineage query marked "Trung bình" priority; FK chain is primary write amplification driver |

### Analysis

Each execution pipeline requires 6+ discrete write operations due to the FK dependency chain. These cannot be parallelized within a single execution context. The write queue serializes these, meaning one slow pipeline delays all subsequent pipelines.

**Amplification factor**: An execution pipeline doing 6 sequential writes at ~1ms each would take ~6ms in serial. Under high load with queue depth, p50 latency degrades non-linearly due to FIFO queuing.

---

## Bottleneck Domain 4 — Adapter Compensation Non-Uniformity

### Description

The `PlannableAdapter` trait provides a compensation interface, but the actual compensation behavior varies by adapter implementation. Some adapters may return success without performing external undo.

### Evidence

| Source | Finding |
|--------|---------|
| `27-production-evaluation-plan.md` §3.6 | "Compensate may be noop-backed: `POST /v1/executions/{execution_id}/compensate` may return 200 without performing external undo depending on adapter implementation and rollback class." |
| `33-feature-completion-backlog.md` §P6 | "Adapter compensation guarantees vary by adapter; `compensate` endpoint may be noop-backed. Gap: No uniform compensation guarantee across all adapters." |
| `45-current-feature-audit.md` §Nhóm 3 | "adapter compensation guarantees phụ thuộc adapter (không đồng nhất)" — Gap G2 |

### Analysis

Non-uniform compensation is a correctness and predictability bottleneck rather than a throughput bottleneck. If an adapter's compensate() is a no-op, the compensation path provides no actual rollback, forcing operators to rely on manual restore.

**Impact on scaling**: Non-uniform compensation does not limit write throughput, but it limits the safety of high-throughput deployments where failures are more frequent.

---

## Bottleneck Domain 5 — Adapter Surface Boundedness

### Description

Adapter surfaces (fs, sqlite, maildraft, git, http) are implemented but have bounded verified slices. Real side-effect integrations are not production-verified.

### Evidence

| Source | Finding |
|--------|---------|
| `45-current-feature-audit.md` §Nhóm 5 | "Adapter surfaces (fs, sqlite, maildraft, git, http) — crate/API shape only, no real side-effect integrations" |
| `33-feature-completion-backlog.md` §P3–P6 | fs adapter (P3): verified local slice only; git adapter (P4): verified local slice only; http adapter (P5): bounded replay only |
| `18-single-node-operations-runbook.md` §1 | "Adapter surfaces (fs, sqlite, maildraft, git, http) — crate/API shape only, no real side-effect integrations" |

### Analysis

The adapter surface is a bottleneck for production use cases requiring real side-effects. The current adapters are verified for the local operations that the test suite exercises, but broader surface areas (permissions, cross-filesystem, remote operations, TLS trust) are unverified.

**Scaling implication**: As workload complexity increases, the probability of hitting an unverified adapter path increases. This constrains the workload profile fit described in `27-production-evaluation-plan.md` §1.2.

---

## Bottleneck Domain 6 — Backup/Restore Limits

### Description

`ferrumctl backup` provides a bounded SQLite-only backup/restore workflow. There is no incremental backup, no built-in scheduling, no retention policy, and no encryption.

### Evidence

| Source | Finding |
|--------|---------|
| `27-production-evaluation-plan.md` §3.5 | "Limitations: SQLite-only (no PostgreSQL backup), no incremental backup, no built-in scheduling, no built-in retention policy, no encryption." |
| `18-single-node-operations-runbook.md` §5.4 | "FerrumGate v1 does not include a built-in backup scheduler. Backup scheduling and retention policy are operator-owned concerns." |
| `33-feature-completion-backlog.md` §S3 | "Gap: No built-in scheduling, no retention policy, no encryption, no incremental backup, no cross-instance restore" |

### Analysis

Backup/restore limits are an operational bottleneck for data-intensive production deployments. The absence of incremental backup means any restore point is as old as the last full backup. The absence of built-in scheduling requires operator-implemented external cron/systemd timers.

**RPO impact**: Recovery Point Objective is determined entirely by backup frequency. For high-write-throughput deployments, a large gap between backups may result in significant data loss on restore.

---

## Bottleneck Domain 7 — Lineage Query at Scale

### Description

Lineage queries traverse the event graph via `ferrum-graph` (BFS ancestor/descendant traversal). At scale, large execution histories may produce slow lineage queries.

### Evidence

| Source | Finding |
|--------|---------|
| `45-current-feature-audit.md` §Nhóm 2 | "Lineage query (execution + multi-hop): ✅ Đã triển khai — lineage handlers tại gateway; `ferrum-graph` (BFS ancestor/descendant traversal)" |
| `27-production-evaluation-plan.md` §1.2 | "Not appropriate: large execution history with complex lineage traversal" |
| `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 3 | PostgreSQL recommended for "large execution history with complex lineage traversal" |

### Analysis

The lineage query is implemented correctly, but SQLite lacks the query optimization infrastructure (parallel query, read replicas) that PostgreSQL provides. For deployments with large execution histories and frequent multi-hop lineage traversal, SQLite becomes a query bottleneck.

**Note**: This is not measured as a bottleneck in current stress tests (S1–S9), which focus on write throughput. Query-at-scale is a workload-specific concern.

---

## Bottleneck Domain 8 — Rate Limiting Under Load

### Description

Rate limiting is implemented via `tower-governor` (2 req/s sustained, burst 50 per IP). Under high concurrent load, the rate limiter may become a bottleneck for legitimate traffic.

### Evidence

| Source | Finding |
|--------|---------|
| `45-current-feature-audit.md` §Nhóm 4 | "Rate limiting: ✅ Đã triển khai — `tower-governor`; default 2/sec per IP, burst 50" |
| `27-production-evaluation-plan.md` §2.2 | "Built-in `tower_governor`: 2 req/s sustained, burst of 50. Applied per-IP via `GovernorLayer`. Periodic cleanup every 60s." |
| `33-feature-completion-backlog.md` §M2 | "Gap: No dedicated stress/load test suite for rate limiting behavior under sustained load" |

### Analysis

The 2 req/s sustained rate limit is conservative. Under burst conditions, the 50-request burst allowance absorbs short spikes. However, for workloads with consistent high legitimate traffic, the rate limit itself becomes a throughput ceiling independent of SQLite.

**Interaction with write queue**: Rate limiting is applied at the HTTP/gateway layer before writes reach the queue. A 429 response does not consume write queue capacity, so rate limiting and write throughput are orthogonal constraints.

---

## Bottleneck Domain 9 — Health Check Depth

### Description

`GET /v1/healthz` and `GET /v1/readyz` are shallow health checks that confirm the server process is alive but do not validate the store, migrations, or governance loop.

### Evidence

| Source | Finding |
|--------|---------|
| `27-production-evaluation-plan.md` §4.2 | "`GET /v1/healthz` and `GET /v1/readyz` are shallow — they confirm the server process is alive and the HTTP endpoint is reachable, but do not validate the store, migrations, or governance loop." |
| `45-current-feature-audit.md` §Nhóm 5 | "healthz/readyz shallow (không deep health check)" — Gap G9 resolved (S2 improved) |
| `18-single-node-operations-runbook.md` §4 | "healthz and readyz are shallow checks. They confirm the server process is alive and the HTTP endpoint is reachable. Do not rely on healthz/readyz alone for governance loop health." |

### Analysis

The shallow health checks are not a throughput bottleneck, but they are an operational bottleneck for detecting partial failures. If the store is unreachable but the HTTP endpoint is responsive, healthz/readyz return 200 while the governance loop is broken.

**Available mitigation**: `GET /v1/readyz/deep` provides a bounded store probe (SELECT 1). Operators should use this for readiness determination rather than relying on shallow checks.

---

## Bottleneck Domain 10 — PostgreSQL Migration Path Deferred

### Description

PostgreSQL is the documented path to eliminating the SQLite single-writer bottleneck and enabling multi-node/HA deployments. The implementation is not started and is deferred per ADR-50.

### Evidence

| Source | Finding |
|--------|---------|
| `50-p4-postgres-store-facade-adr.md` §1 | "No `PostgresStore` or `MySqlStore` implementation exists. Oracle has issued a NO-GO verdict for full implementation at this time." |
| `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 3 | "Phase 3: PostgreSQL Migration — Goal: Eliminate single-writer bottleneck entirely. Target: 1000+ writes/s, 200+ pipelines/s." |
| `27-production-evaluation-plan.md` §Decision Tree | "YES → Are you also ready to start Phase 3 (PostgreSQL/multi-node implementation)? ... NO → Cut RC tag / publish release notes for v1 single-node SQLite only" |

### Analysis

PostgreSQL deferral means the SQLite single-writer ceiling is the hard limit for any deployment requiring >300 writes/s sustained throughput or multi-node topology. The gap is documented and accepted, not a defect.

**Constraint**: PostgreSQL migration is not a current implementation bottleneck — it is a future architectural bottleneck if v1 single-node SQLite is insufficient for target workload.

---

## Bottleneck Summary Table

| # | Bottleneck Domain | Type | Phase 1 Mitigation | Residual Constraint | Documented In |
|---|---|---|---|---|---|
| 1 | SQLite single-writer throughput ceiling | Physical I/O | Write queue eliminates lock thrashing | ~300 writes/s sustained max | `PERFORMANCE_OPTIMIZATION_PLAN.md` §1; `27-production-evaluation-plan.md` §1.2 |
| 2 | Write queue architecture | Design | Serializes writes, eliminates retries | Each write is its own transaction; no cross-op batching | `40-out-of-tree-sqlite-performance-candidate.md` §Phase 1 |
| 3 | FK chain write amplification | Schema | Write queue reduces contention | 6+ sequential writes per pipeline; non-linear latency under load | `PERFORMANCE_OPTIMIZATION_PLAN.md` §Problem Statement |
| 4 | Adapter compensation non-uniformity | Correctness | Bounded compensation implemented | No uniform guarantee; compensate may be noop | `27-production-evaluation-plan.md` §3.6; `33-feature-completion-backlog.md` §P6 |
| 5 | Adapter surface boundedness | Coverage | Local operations verified | Broader surface (permissions, remote, TLS) unverified | `45-current-feature-audit.md` §Nhóm 5; `33-feature-completion-backlog.md` §P3–P5 |
| 6 | Backup/restore limits | Operational | Bounded SQLite backup exists | No incremental/scheduling/retention/encryption | `27-production-evaluation-plan.md` §3.5; `18-single-node-operations-runbook.md` §5 |
| 7 | Lineage query at scale | Query | Implemented correctly | SQLite lacks parallel query; large histories may be slow | `27-production-evaluation-plan.md` §1.2 |
| 8 | Rate limiting under load | HTTP/Gateway | tower-governor active | 2 req/s sustained may limit high-throughput legitimate traffic | `27-production-evaluation-plan.md` §2.2; `33-feature-completion-backlog.md` §M2 |
| 9 | Health check depth | Operational | readyz/deep store probe available | Shallow checks do not validate governance loop | `27-production-evaluation-plan.md` §4.2; `18-single-node-operations-runbook.md` §4 |
| 10 | PostgreSQL migration path deferred | Architectural | N/A (not implemented) | Single-node SQLite ceiling is hard limit for scale | `50-p4-postgres-store-facade-adr.md`; `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 3 |

---

## Non-Scale Bottlenecks (Out of Scope)

The following are noted in `45-current-feature-audit.md` but are not primary scale bottlenecks:

- **Invariant gaps** (I1, I5, I6, I7, I11): Verified and resolved in Phase 2 (D3/D4). See `45-current-feature-audit.md` §Phase 2 D3+D4.
- **DLP stub**: Post-v1 scope. See `33-feature-completion-backlog.md` §S1.
- **cancel_execution**: Implemented. See `33-feature-completion-backlog.md` §M3.

---

## Relationship to Phase 2

Phase 2 (transaction batching) was attempted and reverted due to performance regression. The write queue architecture (Phase 1) remains the production target. Phase 2 is **not recommended for reimplementation** based on benchmark evidence. See `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 2 status note.

---

## D6 Status

D6 (priority list for extension/scaling) is **complete** — see `52-d6-priority-expansion-list.md`. D6 ranks the extension items derived from this bottleneck analysis and defines the Phase 3 entry gates.

---

## Production Posture

This analysis supports the RC-ready/conditional single-node SQLite posture documented in:

- `27-production-evaluation-plan.md` — Production evaluation framework
- `23-production-readiness-assessment.md` — RC-ready declaration
- `19-v1-single-node-support-contract.md` — Support constraints

**No production-ready claim is made in this document.** Deployments exceeding Phase 1 capacity (≤300 writes/s sustained, single-node) require PostgreSQL migration (Phase 3) or alternative architecture.

---

*Document completed: 2026-04-28. D5 of Phase 3 feature audit per `45-current-feature-audit.md`.*
