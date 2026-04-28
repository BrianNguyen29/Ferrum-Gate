# 33 — Feature Completion Backlog

> **Status**: Created 2026-04-28 — P6/P7 documentation refresh
> **Scope**: Single-node v1 SQLite unless labeled post-v1
> **Purpose**: Categorized backlog of incomplete/partial features for v1 and post-v1
> **Supersedes**: Historical stale descriptions in roadmap-v1/v2 docs that characterized adapters as "skeleton/no implementation"

---

## Overview

This document categorizes features that are incomplete, partial, or deferred. It supplements:
- [01-current-state.md](./01-current-state.md) — phase status + test coverage matrix
- [32-feature-completeness-audit.md](./32-feature-completeness-audit.md) — route/API reconciliation
- [45-current-feature-audit.md](./45-current-feature-audit.md) — Phase 1/2 analysis complete; Phase 3 D5 bottleneck analysis report complete ([51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md)); D6 priority list complete ([52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md))

**Guiding principles**:
- Implemented ≠ v1-supported. See `19-v1-single-node-support-contract.md`.
- Partial implementation does not expand v1 scope.
- Post-v1 features are not v1 defects.

---

## Must Fix (v1 RC evidence / hardening gaps)

These items are bounded gaps within the v1 single-node SQLite scope.

### M1 — Output sanitization: bounded wiring v1 design complete

| Aspect | Detail |
|--------|--------|
| **Current state** | Bounded wiring complete per `48-i11-output-sanitization-design.md`: `sanitize_output` wired to 7 targeted endpoints (revoke_capability, delete_policy_bundle, set_policy_bundle_active, get_execution, get_execution_lineage, query_lineage, list_bridge_tools); 2 integration tests pass |
| **Design** | Hybrid v1: sanitize reflected errors + targeted high-risk endpoints; full middleware deferred post-v1 |
| **Gap** | None — bounded wiring complete per design note |
| **Risk** | MED — bounded scope verified |
| **Invariant** | I11 VERIFIED per `45-current-feature-audit.md` §I11 |
| **Production ready** | **No** — bounded v1 design only; full production deferred |
| **Next action** | None for v1 — design decision on full middleware is post-v1 |

### M2 — Rate-limit test depth

| Aspect | Detail |
|--------|--------|
| **Current state** | 5 integration tests: `test_rate_limit_returns_429_when_exceeded`, `test_rate_limit_allows_requests_under_limit`, `test_rate_limit_per_ip_isolation`, `test_rate_limit_recovery_after_cooldown`, plus concurrent burst coverage |
| **Gap** | No dedicated stress/load test suite for rate limiting behavior under sustained load |
| **Risk** | LOW — core rate limiting (tower-governor, 2/sec per IP, burst 50) implemented |
| **Production ready** | **No** — test depth improved but production stress suite not in v1 scope |
| **Next action** | Sustained-load rate-limit tests if production use planned |

### M3 — cancel_execution HTTP endpoint

| Aspect | Detail |
|--------|--------|
| **Current state** | `POST /v1/executions/{execution_id}/cancel` implemented with route/handler, state transition (Running/Prepared/AwaitingVerification → Canceled), provenance event (SideEffectRolledBack), and 4 integration tests |
| **Gap** | None — endpoint implemented and tested |
| **Risk** | LOW — bounded implementation |
| **Production ready** | **Bounded** — terminal states (Committed, Compensated, etc.) rejected with 409; non-terminal states (Running, Prepared, etc.) can be canceled |
| **Next action** | None |

---

## Should Fix (v1 polish / production hardening)

These items improve production posture but are not v1 RC blockers.

### S1 — DLP: docs-only stub, no v1 implementation planned

| Aspect | Detail |
|--------|--------|
| **Current state** | `dlp_findings` stub returns empty findings; honest stub per design docs |
| **Gap** | No actual DLP scanning implemented — intentional post-v1 deferral |
| **Risk** | LOW — stub is explicit, not a silent bypass |
| **Invariant** | G5 in `45-current-feature-audit.md` §Gaps |
| **Production ready** | **No** — stub only, post-v1 scope |
| **Next action** | Implement DLP scanning post-v1 if data loss prevention is a requirement |

### S2 — healthz/readyz: shallow probes

| Aspect | Detail |
|--------|--------|
| **Current state** | `/v1/healthz` and `/v1/readyz` are shallow (process alive only); `/v1/readyz/deep` implemented with store probe; `/v1/metrics` exposes bounded health/metrics counters and store up/down gauge |
| **Gap** | Broader observability remains bounded: no latency histograms, broad per-route error counters, WAL/page gauges, or pool saturation metrics |
| **Verification** | ✅ S2 improved — 2026-04-28: bounded tests added covering unhealthy store response (503/degraded/healthy=false/component error) |
| **Risk** | LOW — deep readiness exists and verified for failure mode |
| **Production ready** | **Partial** — deep probe verified for store-unhealthy path; broader failure modes (adapter failure, corruption) remain post-v1 scope |
| **Next action** | Future: add latency/error/WAL/pool metrics if Phase 3 or pilot operations require them |

### S3 — Backup/restore: bounded SQLite-only; external scheduling is v1 architecture

| Aspect | Detail |
|--------|--------|
| **Current state** | `ferrumctl backup create/verify/restore` exists; offline/local workflow; SQLite-only |
| **v1 architecture** | Backup scheduling and retention are **operator-owned external concerns** — no built-in scheduler, no retention engine, no encryption in v1. Operators use cron, systemd timers, or external backup tools to drive `ferrumctl backup`. |
| **Gap** | No built-in scheduling, no retention policy, no encryption, no incremental backup, no cross-instance restore |
| **Risk** | MED for production data durability |
| **Production ready** | **No** — production deployments need operator-implemented scheduling + retention; no production claim is made for automated backup |
| **Next action** | None for built-in implementation — operator implements external scheduling per runbook examples; built-in scheduler/retention/encryption remain deferred post-v1 |

### S4 — Policy bundle / bridge support boundary

| Aspect | Detail |
|--------|--------|
| **Current state** | Policy bundle CRUD + activate fully implemented in gateway; `RuntimeBridge` trait + `McpBridge` implemented; GET `/v1/bridges` + GET `/v1/bridges/{id}/tools` endpoints exist |
| **Gap** | ✅ Clarified — Doc 19 §2.4 explicitly lists policy bundle (6 routes) and bridge (2 routes) as implemented but outside v1 support contract; Doc 45 gap table updated with S4 entry |
| **Risk** | LOW — documented as post-v1/experimental |
| **Invariant** | G8 in `45-current-feature-audit.md` §Gaps; `32-feature-completeness-audit.md` §4 |
| **Production ready** | **No** — governance admin and runtime bridge integrations are experimental/internal |
| **Next action** | None — boundary clarification complete; no production claim made or intended |

---

## Production-only (post-v1 scope — not implemented / deferred)

These require PostgreSQL, multi-node, or broader adapter surface — not in v1 scope.

### P1 — PostgreSQL store backend

| Aspect | Detail |
|--------|--------|
| **Current state** | Not implemented |
| **Gap** | SQLite single-writer bottleneck; write-queue Phase 1 eliminates lock thrash but not raw throughput ceiling |
| **Phase 3 path** | `PERFORMANCE_OPTIMIZATION_PLAN.md` §Phase 3 documents design; no implementation exists |
| **Risk** | HIGH for sustained high-write production |
| **Production ready** | **No** — not implemented |
| **Recommendation** | PostgreSQL for production scale; see `PERFORMANCE_OPTIMIZATION_PLAN.md` |

### P2 — Multi-node / HA / read-replica

| Aspect | Detail |
|--------|--------|
| **Current state** | Not implemented; out of v1 scope per support contract |
| **Gap** | Single-node only; no HA, no read-replica, no cross-instance coordination |
| **Risk** | HIGH for multi-node production deployments |
| **Production ready** | **No** |
| **Recommendation** | Design multi-node architecture if HA is required |

### P3 — fs adapter: remaining surface (post-v1)

| Aspect | Detail |
|--------|--------|
| **Current verified slice** | 135 tests — FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod + PlannableFsAdapter + cross-filesystem |
| **Remaining surface** | Permissions/ownership/symlink handling; cross-filesystem or mount-point boundary handling; boundedness guarantees for non-transactional fs operations |
| **Risk** | MED |
| **Production ready** | **Partial** — verified local slice is bounded; broader surface is post-v1 |

### P4 — git adapter: remaining surface (post-v1)

| Aspect | Detail |
|--------|--------|
| **Current verified slice** | 86 tests — GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete + rollback fail-closed for push/fetch |
| **Remaining surface** | Remote push/pull recovery with existing local ref tracking; remote branch operations (upstream tracking); submodule/subtree recovery patterns; partial checkout or sparse-checkout recovery |
| **Risk** | MED-HIGH |
| **Production ready** | **Partial** — verified local slice is bounded; broader surface is post-v1 |

### P5 — http adapter: remaining surface (post-v1)

| Aspect | Detail |
|--------|--------|
| **Current verified slice** | 103 tests — bounded HttpMutation prepare/execute/verify + PUT/PATCH replay + connection pooling/retry; strict `http.replay_v1` POST/PUT/PATCH with exact URL/digest binding |
| **Remaining surface** | Broader request replay and idempotency-key handling beyond narrow one-step `http.replay_v1`; response snapshotting beyond digest-only; connection keepalive management; retry/backoff with rollback semantics; timeout and cancellation; TLS trust/cert pinning |
| **Risk** | MED-HIGH |
| **Production ready** | **Partial** — bounded replay is verified; broader surface is post-v1 |

### P6 — Adapter compensation guarantees (non-uniform)

| Aspect | Detail |
|--------|--------|
| **Current state** | Adapter compensation guarantees vary by adapter; `compensate` endpoint may be noop-backed |
| **Gap** | No uniform compensation guarantee across all adapters |
| **Risk** | MED |
| **Production ready** | **No** — manual verification per adapter required |
| **Next action** | Per-adapter compensation audit before production use |

---

## Summary Table

| ID | Category | Feature | Status | v1 RC Blocker |
|----|----------|---------|--------|---------------|
| M1 | Must | Output sanitization (bounded wiring) | **Bounded wiring complete** (7 endpoints); I11 VERIFIED | No |
| M2 | Must | Rate-limit test depth | **5 tests** (concurrent burst, per-IP isolation, recovery) | No |
| M3 | Must | cancel_execution HTTP endpoint | **Implemented** with route/handler/tests | No |
| S1 | Should | DLP stub | Docs-only stub; post-v1 | No |
| S2 | Should | Deep health check coverage | Partial | No |
| S3 | Should | Backup/restore (scheduling/retention) | Bounded SQLite-only | No |
| S4 | Should | Policy bundle/bridge boundary | Post-v1/experimental | No |
| P1 | Production-only | PostgreSQL backend | Not implemented | N/A |
| P2 | Production-only | Multi-node/HA | Not implemented | N/A |
| P3 | Production-only | fs adapter remaining surface | Post-v1 | N/A |
| P4 | Production-only | git adapter remaining surface | Post-v1 | N/A |
| P5 | Production-only | http adapter remaining surface | Post-v1 | N/A |
| P6 | Production-only | Adapter compensation uniformity | Non-uniform | N/A |

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `33-feature-completion-backlog.md` | `01-current-state.md` | Phase status + test coverage |
| `33-feature-completion-backlog.md` | `32-feature-completeness-audit.md` | Route/API boundary |
| `33-feature-completion-backlog.md` | `45-current-feature-audit.md` | Phase 3 D5+D6 complete |
| `33-feature-completion-backlog.md` | `52-d6-priority-expansion-list.md` | Priority ranking derived from D5 bottleneck analysis |
| `33-feature-completion-backlog.md` | `19-v1-single-node-support-contract.md` | v1 support boundary |
| `33-feature-completion-backlog.md` | `PERFORMANCE_OPTIMIZATION_PLAN.md` | Phase 2/3 perf path |
| `01-current-state.md` | `33-feature-completion-backlog.md` | Backlog cross-link |
| `32-feature-completeness-audit.md` | `33-feature-completion-backlog.md` | Backlog cross-link |
| `45-current-feature-audit.md` | `33-feature-completion-backlog.md` | Backlog cross-link |

---

*Document created: 2026-04-28. Part of P6/P7 documentation refresh.*
