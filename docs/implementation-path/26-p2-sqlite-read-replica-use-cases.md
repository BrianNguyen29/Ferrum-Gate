# 26 — P2.4a SQLite Read-Replica Use-Case Analysis

## Overview

This document analyzes SQLite read-replica deployment as a post-v1 preparation
step for FerrumGate multi-node readiness. It is **analysis/planning only**;
no implementation, no consensus design, no write-path, and no transport
design beyond stated non-goals.

ASCII only.

**Scope**: Bounded to P2.4a analysis slice. Leader-election analysis (P2.4b)
is a separate future work item and is out of scope here.

**Context**: FerrumGate v1 is scoped to single-node SQLite deployment.
P2.4a is a pre-implementation analysis step that should complete before any
future read-replica or HA implementation work begins. This doc is not a
blocker for v1 RC.

---

## 1. Sanctioned Use Cases

FerrumGate's SQLite database stores:

- Intent definitions and versions
- Proposal records and state
- Execution logs and terminal events
- Provenance edges
- Ledger entries (append-only hash chain)
- Sync metadata (leader tips, sync state, allowlist)

A read-replica (SQLite WAL follower or Litestream-style continuous replica)
can serve the following **sanctioned read-only use cases** without directly
changing write-path correctness:

### 1.1 Read-Scaling Query Load

**Use case**: Offload provenance query, lineage walk, and intent inspection
to a replica node to reduce read pressure on the write node.

| Aspect | Detail |
|--------|--------|
| Reads served | Provenance queries (`POST /v1/provenance/query`), lineage walks (`walk_backwards_from`, `walk_forwards_from`), intent/proposal/execution reads |
| Writes | Never forwarded to replica; writes go to primary only |
| Consistency | Replica may lag (ms to s); read-your-writes not guaranteed on replica |
| Conflict with sync work | None — these reads do not interact with sync protocol |

**Verification**: Read-only queries are idempotent and do not modify sync
metadata or ledger state.

### 1.2 Operator Reporting and Inspection

**Use case**: Run operator-facing inspection and reporting reads against a
replica without adding read pressure to the write node.

| Aspect | Detail |
|--------|--------|
| Reads served | Approval inspection, provenance inspection/export, execution history reads |
| Writes | None |
| Risk | Low - reporting reads are non-critical for core correctness |

**Verification**: Reporting and inspection reads are read-only; no state modification.

### 1.3 Backup / Forensic Query

**Use case**: Run ad-hoc SQL queries for audit or forensic analysis on a
read-replica without touching the write node.

| Aspect | Detail |
|--------|--------|
| Reads served | Audit queries, provenance replay reads, execution history |
| Writes | None |
| Risk | Query churn does not affect write node |
| Consistency | Point-in-time snapshot only; no transactional cross-replica guarantees needed |

**Verification**: All queries are provenance reads and are already gated by
`POST /v1/provenance/query` endpoint authorization.

### 1.4 Development / Staging Mirror

**Use case**: Mirror production SQLite to a local or staging replica for
development workflows and integration testing.

| Aspect | Detail |
|--------|--------|
| Reads served | Full local read access |
| Writes | None in dev/staging |
| Risk | Isolated to non-production environment |

**Verification**: Development use is out-of-band from production write path.

---

## 2. Non-Goals (Explicitly Out of Scope)

The following are **not sanctioned use cases** for P2.4a and are explicitly
deferred beyond this slice:

| Non-Goal | Reason |
|----------|--------|
| Write forwarding from replica to primary | Write path not designed; SQLite does not support native multi-writer; requires custom sync write-path |
| Two-way sync / multi-leader | Consensus not designed; write-path out of scope for v1 |
| Read-slicing for capability evaluation | All evaluation writes go through primary; read-replica cannot serve policy decisions |
| Distributed transaction spanning replica + primary | No cross-node transaction support; SQLite transactions are local |
| HA automatic failover | Failover requires consensus or leader-election design (P2.4b); not analyzed here |
| Write-path leader election | P2.4b is separate analysis item |
| Real transport design | Transport design belongs to P3 sync implementation |
| Distributed ledger proof continuity | Full remote proof continuity is P3; local chain verification is interim only |

---

## 3. Risks and Limitations

### 3.1 Replica Lag

**Risk**: Read-replica may serve stale data. Provenance queries on replica
may return results that do not reflect the latest write on primary.

**Mitigation**: Clearly document that replica reads are eventually consistent;
do not use replica for any correctness-critical reads (evaluation decisions,
capability state).

**Severity**: Medium for operator reporting; Low for audit/forensic.

### 3.2 WAL Hot-Standby Semantic Gaps

**Risk**: SQLite WAL mode allows concurrent reads but a long-running read
transaction on the replica can block checkpoint progress, causing replica lag
to accumulate.

**Mitigation**: Keep replica read transactions short and monitor checkpoint lag.

**Severity**: Medium.

### 3.3 No Write Path Independence

**Risk**: Read-replica provides no write scalability. Any write pressure
immediately hits the primary.

**Mitigation**: This is a read-scale only solution. Write scaling requires
full multi-node sync with consensus (beyond P2 scope).

**Severity**: Informational.

### 3.4 Operational Complexity

**Risk**: Adding a replica adds deployment complexity: replica provisioning,
monitoring lag, promotion procedures, and failure handling.

**Mitigation**: Document runbook steps for replica setup and promotion;
define dedicated replica-lag monitoring before any rollout.

**Severity**: Low for observability use cases; Medium for read-scaling.

### 3.5 Provenance Query Authorization Boundary

**Risk**: Replica serves provenance queries that bypass primary's authorization
middleware if served on a different endpoint.

**Mitigation**: Replica queries must go through the same gateway authorization
as primary; never expose raw SQLite port publicly.

**Severity**: High if not mitigated; Low if mitigated.

---

## 4. Implementation Requirements for Replica Readiness

These are **not implementation tasks**; they are analysis checkpoints that
must be satisfied before any future replica implementation can begin:

| Requirement | Description | Status |
|-------------|-------------|--------|
| R1 | Read-only SQLite connections are isolated from write connections in `ferrum-store` connection pool | Analysis complete; implementation N/A for P2.4a |
| R2 | Provenance query endpoint can be served from read-only replica (no write side effects) | Analysis complete; implementation N/A for P2.4a |
| R3 | Sync metadata tables (`leader_tips`, `sync_state`, `leader_allowlist`) are not queried by read-replica use cases | Analysis complete; implementation N/A for P2.4a |
| R4 | Replica promotion procedure is documented (promote replica to primary on primary failure) | **TODO** - belongs in a future replica ops runbook |
| R5 | Replica lag monitoring exists (Prometheus metric for replica lag) | **TODO** - belongs in a future replica ops runbook |

---

## 5. Done Criteria

This slice is **done** when:

- [x] All sanctioned use cases (Section 1) are documented with clear read/write
  boundaries
- [x] All non-goals (Section 2) are explicitly listed and referenced as
  out-of-scope
- [x] All risks (Section 3) are documented with severity and mitigation
- [x] All implementation requirements (Section 4) are documented as analysis
  checkpoints
- [x] This doc is referenced from:
  - `24-p1-p2-p3-execution-plan.md` (P2.4a status row)
  - `23-production-readiness-assessment.md` (HA readiness section)
  - `docs/implementation-path/README.md` (document index)
- [x] P2.4b (leader-election analysis) is tracked separately

**This is an analysis-only slice. No code changes, no transport design,
no consensus design, and no write-path design are in scope.**

---

## 6. Relationships to Other Documents

| Document | Relationship |
|----------|--------------|
| `24-p1-p2-p3-execution-plan.md` | P2.4a is a row in the P2.4 HA readiness table; this doc is the analysis output |
| `23-production-readiness-assessment.md` | HA readiness section (2.7) references this doc as the analysis output for P2.4a |
| `18-cross-node-ledger-sync-plan.md` | SQLite read-replica is orthogonal to sync protocol; replica serves reads, sync serves cross-node writes |
| `19-sync-1-protocol-sketch.md` | Write-path out of scope here; Sync-1 is one-way fast-forward from leader to follower |
| `22a-sync-3a1-probe-api-boundary.md` | Sync-3a.1 probe is read-only and can be served from any node; replica analysis is separate |

---

## 7. Future Work (Out of Scope for P2.4a)

The following are **explicitly deferred** and are not part of this slice:

| Item | Phase | Notes |
|------|-------|-------|
| Replica implementation (Litestream, custom WAL follower) | Future | Requires ops runbook (R4, R5) first |
| Leader-election analysis | P2.4b | Separate doc; required before HA can proceed |
| Write-path / apply-path design | P3 | Explicitly out of scope for v1 |
| Consensus / multi-leader design | Beyond P3 | Requires Raft or equivalent; not designed |
| Cross-node transaction spanning replica + primary | Beyond P3 | Not supported by SQLite |
| Transport design for read-replica promotion | P3 | Transport belongs to sync implementation |
