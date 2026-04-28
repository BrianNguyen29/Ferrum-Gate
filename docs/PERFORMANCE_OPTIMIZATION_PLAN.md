# Performance Optimization Plan — FerrumGate Write Bottleneck

> **⚠️ Out-of-tree / Unmerged-draft Status (2026-04-28):** This plan was a working draft document. Phase 1 (write-queue) is implemented in-tree and is the production target. Phase 2 was partially attempted but **regressed** and was **deferred**. Phase 3 (PostgreSQL) is the path to full production scale but is **not implemented**. Do not treat Phase 2 as available or Phase 3 as implemented.
>
> **Phase 2 Status**: Deferred/regressed. Partial implementation introduced performance regression. Phase 1 write-queue architecture restored as production target.
>
> **Phase 3 Status**: Not implemented. PostgreSQL recommended for production scale but no implementation exists. This plan is a design reference only.
>
> **Created**: 2026-04-07
> **Scope**: Resolve S4/S5/S6/S7 stress test bottlenecks caused by SQLite single-writer contention

## Problem Statement

SQLite's single-writer architecture creates a hard throughput ceiling of ~8-10 writes/s. Combined with synchronous handler writes and a cascading FK chain (`intents → proposals → capabilities → executions → rollback_contracts`), this produces:

| Scenario | Workers | Throughput | p50 Latency | Error Rate | Root Cause |
|----------|---------|------------|-------------|------------|------------|
| S4 intent-compile | 5 | ~6.5 req/s | 140ms | ~10% | Single INSERT contention |
| S5 execution-pipeline | 5 | ~1.1 req/s | 958ms | ~73% | 6 sequential writes × contention |
| S6 capability cycle | 5 | ~1.5 req/s | varies | ~33% | 3 writes + fire-and-forget |
| S7 sqlite-contention | 50 | ~8.1 req/s | 3.26s | 0%* | Pure write saturation |

---

## Phase 1: SQLite Write-Queue + Retry Cleanup + PRAGMA Tuning

**Timeline**: 1-2 days
**Status**: ✅ Completed
**Goal**: Eliminate all write errors, reduce S7 p50 from 3.26s → ~400ms, stabilize S5 errors from 73% → 0%

### Actual Phase 1 Results

Measured after implementation with release `ferrum-stress` run:

| Scenario | Before | After | Improvement |
|----------|--------|-------|-------------|
| S4 intent-compile | 6.5 req/s, 140ms, ~10% err | 305.5 req/s, 2.25ms, 0% err | ~47x throughput |
| S5 execution-pipeline | 1.1 req/s, 958ms, ~73% err | 57.6 req/s, 16.0ms, 0% err | ~52x throughput |
| S6 capability cycle | 1.5 req/s, ~33% err | 42.0 req/s, 0.30ms, 0% err | ~28x throughput |
| S7 sqlite-contention | 8.1 req/s, 3.26s, 0% err | 289.4 req/s, 29.9ms, 0% err | ~36x throughput |

Phase 1 materially outperformed the original target. The queue removed lock-thrashing rather than merely smoothing it.

### P1.1 — Write-Queue Channel (mpsc → single writer task)

**What**: Replace N connections competing for SQLite write lock with 1 dedicated writer task that processes write operations sequentially via `tokio::sync::mpsc` channel.

**Files to create/modify**:
- `crates/ferrum-store/src/sqlite/write_queue.rs` — NEW: WriteQueue struct + WriteOp enum
- `crates/ferrum-store/src/sqlite/mod.rs` — Integrate WriteQueue into SqliteStore
- `crates/ferrum-store/src/repos.rs` — No change (trait interfaces stay the same)

**Architecture (planned / effective shape)**:
```
┌──────────────────────────────────────────────────────────────┐
│                    SqliteStore (with WriteQueue)              │
│                                                              │
│  ┌─────────┐     ┌──────────────────┐     ┌──────────────┐  │
│  │ Handler  │────▶│ mpsc::Sender     │────▶│ Writer Task  │  │
│  │ (N conns)│     │ (WriteOp enum)   │     │ (1 conn)     │  │
│  └─────────┘     │                  │     │              │  │
│                   │ WriteOp::{       │     │ BEGIN IMMED  │  │
│                   │   Insert{...}    │     │ INSERT/UPDATE│  │
│                   │   Update{...}    │     │ COMMIT       │  │
│                   │   Pipeline{...}  │     └──────────────┘  │
│                   │ }                │                       │
│                   └──────────────────┘     ┌──────────────┐  │
│                                            │ Pool (reads) │  │
│                                            │ 20 conns     │  │
│                                            └──────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

**WriteOp enum design**:
```rust
enum WriteOp {
    InsertIntent {
        data: IntentEnvelope,
        reply: oneshot::Sender<Result<()>>,
    },
    InsertProposal {
        data: ActionProposal,
        reply: oneshot::Sender<Result<()>>,
    },
    InsertCapability {
        data: CapabilityLease,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateCapability {
        data: CapabilityLease,
        reply: oneshot::Sender<Result<()>>,
    },
    InsertExecution {
        data: ExecutionRecord,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateExecution {
        data: ExecutionRecord,
        reply: oneshot::Sender<Result<()>>,
    },
    InsertRollbackContract {
        data: RollbackContract,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateRollbackContract {
        data: RollbackContract,
        reply: oneshot::Sender<Result<()>>,
    },
    AppendProvenanceEvent {
        data: ProvenanceEvent,
        reply: oneshot::Sender<Result<()>>,
    },
    AppendLedgerEntry {
        data: LedgerEntry,
        reply: oneshot::Sender<Result<()>>,
    },
    // For fire-and-forget writes (revoke, background provenance)
    FireAndForget(Box<WriteOp>),  // no reply sent
    // For pipeline batching (Phase 2)
    Pipeline {
        ops: Vec<WriteOp>,
        reply: oneshot::Sender<Result<Vec<()>>>,
    },
}
```

**Key implementation details**:
1. `SqliteStore` holds: `pool: SqlitePool` (for reads) + `write_tx: mpsc::Sender<WriteOp>` (for writes)
2. Writer task is spawned in `SqliteStore::connect_with_pool_size()` and executes serialized writes against the configured SQLite pool
3. All repo `insert()` / `update()` methods send through channel and await `oneshot::Sender` reply
4. Read methods (`get()`, `list()`, `query()`) continue using pool directly — no change
5. `FireAndForget` variant: writer processes but doesn't send reply — for revoke background writes
6. Channel capacity: 256 (backpressure threshold)
7. Writer task serializes writes centrally; Phase 2 may further batch them into explicit transactions

**Retry logic becomes unnecessary** — single writer means no SQLITE_BUSY. All retry loops in handlers can be removed.

**Expected results**:
- S4: errors 10% → 0%, throughput stays ~8 req/s but predictable latency
- S5: errors 73% → 0%, throughput ~2-3 pipelines/s (sequential writes in single writer)
- S6: errors 33% → 0%, background revoke writes queue without contention
- S7: p50 3.26s → ~400ms (fair FIFO queue instead of random backoff)

**Actual result note**: The final implementation significantly exceeded the original estimate because the old bottleneck was dominated by lock contention and retry churn, not SQLite's raw single-writer throughput alone.

### P1.2 — Remove Stale Retry Logic from Handlers

**What**: Since write-queue eliminates SQLITE_BUSY, remove retry loops from handlers.

**Files to modify**:
- `crates/ferrum-gateway/src/server.rs`:
  - `mint_capability` (lines 362-380): Remove retry loop, replace with single `insert()` call
  - `authorize_execution` (lines 482-494): Remove retry loop, replace with single `insert()` call
  - Update stale comments at lines 358-361 and 479-481

**Before** (mint_capability):
```rust
let mut retries = 0u32;
loop {
    match state.runtime.store.capabilities().insert(&response.lease).await {
        Ok(()) => break,
        Err(e) => {
            retries += 1;
            if retries > 3 { return Err(ApiProblem::internal(anyhow::Error::from(e))); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
```

**After**:
```rust
state.runtime.store.capabilities()
    .insert(&response.lease)
    .await
    .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;
```

### P1.3 — PRAGMA Tuning

**What**: Optimize SQLite PRAGMA settings for write throughput.

**Files to modify**:
- `crates/ferrum-store/src/sqlite/mod.rs` — Add `after_connect` hook to `SqlitePoolOptions`

**Changes**:
```rust
SqlitePoolOptions::new()
    .max_connections(max_connections)
    .after_connect(|conn, _meta| Box::pin(async move {
        sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
        sqlx::query("PRAGMA synchronous=NORMAL").execute(&mut *conn).await?;
        sqlx::query("PRAGMA wal_autocheckpoint=1000").execute(&mut *conn).await?;
        sqlx::query("PRAGMA busy_timeout=5000").execute(&mut *conn).await?;
        sqlx::query("PRAGMA cache_size=-64000").execute(&mut *conn).await?; // 64MB cache
        Ok(())
    }))
    .connect(database_url)
    .await?;
```

**Rationale**:
- `synchronous=NORMAL`: With WAL mode, this is safe (no corruption risk) and reduces fsync frequency
- `wal_autocheckpoint=1000`: Fewer checkpoints = less I/O interruption
- `busy_timeout=5000`: Explicit (was relying on default)
- `cache_size=-64000`: 64MB page cache reduces disk reads

**Expected improvement**: ~10-30% write throughput

### P1.4 — Make revoke_capability Fire-and-Forget Use WriteQueue

**What**: Convert revoke's `tokio::spawn` to use WriteQueue's `FireAndForget` variant instead.

**Current status**: Partially deferred. Background revoke persistence still uses `tokio::spawn`, but the writes inside that task now route through queue-backed repos. This is functionally correct for Phase 1, though a true non-blocking best-effort queue path remains a cleanup item.

**Files to modify**:
- `crates/ferrum-gateway/src/server.rs` lines 434-442: Replace `tokio::spawn` with write queue fire-and-forget

**Before**:
```rust
tokio::spawn(async move {
    if let Err(e) = store.capabilities().update(&lease_clone).await { ... }
    if let Err(e) = store.provenance().append_event(&event).await { ... }
});
```

**After** (conceptual):
```rust
// Fire-and-forget via write queue — no handler blocking, no pool contention
let _ = state.runtime.store.capabilities().fire_and_forget_update(&lease_clone).await;
let _ = state.runtime.store.provenance().fire_and_forget_append(&event).await;
```

### P1.5 — Update Stress Test to Remove Client-Side Retry Hacks

**Files to modify**:
- `bins/ferrum-stress/src/main.rs`:
  - Remove client-side retry loops for mint/authorize/prepare (S5, S6, S9)
  - These were workarounds for SQLITE_BUSY; no longer needed with write-queue

### P1.6 — Tests

- Add `test_write_queue_processes_inserts` — verify single write through queue
- Add `test_write_queue_fire_and_forget` — verify fire-and-forget variant
- Add `test_write_queue_backpressure` — verify channel full behavior
- Add `test_write_queue_ordering` — verify FIFO ordering
- Update existing integration tests (should pass unchanged since trait interface is identical)
- Run full stress suite to validate improved numbers

### P1 Verification Checklist

- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] S4: error rate → 0%
- [x] S5: error rate → 0%
- [x] S6: error rate → 0%
- [x] S7: p50 < 500ms

### Phase 1 Follow-ups Carried into Phase 2

- Replace dummy/dead `oneshot` allocation pattern in queue-backed repos with a cleaner request/reply abstraction.
- Make `SqliteStore::shutdown()` actually drain and await the writer task.
- Expose a true best-effort fire-and-forget path for revoke persistence rather than relying on `tokio::spawn` + normal queued writes.
- Tighten tests to avoid timing-based sleeps where possible.

---

## Phase 2: Transaction Batching for Pipelines

**Timeline**: 2-3 days
**Goal**: Increase S5 pipeline throughput from ~2-3 → ~6-8 pipelines/s by batching writes
**Status**: **DEFERRED** — Partial implementation introduced performance regression. Phase 1 architecture restored as production target.

> **Note**: A partial Phase 2 implementation (Transaction/Pipeline batching in WriteOp enum) was attempted but benchmark testing revealed performance regression rather than improvement. The implementation has been reverted. Phase 1 write-queue architecture remains production-ready.

### P2.1 — Pipeline Transaction Support in WriteQueue

**What**: Add a `Pipeline` variant to WriteOp that executes multiple writes in a single `BEGIN IMMEDIATE ... COMMIT` transaction.

**Files to modify**:
- `crates/ferrum-store/src/sqlite/write_queue.rs` — Add Pipeline variant processing
- `crates/ferrum-store/src/repos.rs` — Add `StoreFacade::execute_in_transaction()` method (optional, or handler constructs Pipeline directly)

**Design**:
```rust
// In write_queue.rs writer task:
WriteOp::Pipeline { ops, reply } => {
    let result = conn.transaction(|| {
        for op in ops {
            execute_single_write(&conn, op)?;  // no individual replies
        }
        Ok(())
    }).await;
    let _ = reply.send(result.map(|_| vec![]));
}
```

### P2.2 — Batch S5 Pipeline Writes

**What**: Combine 6 sequential handler writes into 1-2 write-queue pipeline submissions.

**Files to modify**:
- `crates/ferrum-gateway/src/server.rs` — New `execute_pipeline` handler or refactor existing handlers

**Approach**: Instead of 6 separate HTTP requests each doing 1 write, create a "pipeline batch" endpoint or refactor the handler flow to batch writes:
- Batch 1: INSERT intent + INSERT proposal + INSERT capability (3 writes in 1 transaction)
- Batch 2: INSERT execution + INSERT rollback_contract + UPDATE execution (3 writes in 1 transaction)

This reduces lock acquisitions from 6 to 2 per pipeline.

### P2.3 — Direct UPDATE for Status Changes

**What**: Replace read-then-write patterns with direct UPDATE queries.

**Files to modify**:
- `crates/ferrum-store/src/sqlite/intents.rs` — `update_status()` → direct `UPDATE ... SET status = ? WHERE id = ?`
- `crates/ferrum-store/src/sqlite/executions.rs` — `update_state()` → direct UPDATE
- `crates/ferrum-store/src/sqlite/rollback.rs` — `update_state()` → direct UPDATE

### P2 Verification Checklist

- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] S5: throughput > 5 pipelines/s
- [ ] S7: throughput > 12 writes/s
- [ ] No regression in read-heavy scenarios (S1, S2, S3, S8)

---

## Phase 3: PostgreSQL Migration

**Timeline**: 1-2 weeks
**Goal**: Eliminate single-writer bottleneck entirely. Target: 1000+ writes/s, 200+ pipelines/s.

### P3.1 — PostgresStore Implementation

**What**: Implement `StoreFacade` trait for PostgreSQL, parallel to `SqliteStore`.

**Files to create**:
- `crates/ferrum-store/src/postgres/mod.rs` — PostgresStore struct
- `crates/ferrum-store/src/postgres/intents.rs`
- `crates/ferrum-store/src/postgres/proposals.rs`
- `crates/ferrum-store/src/postgres/capabilities.rs`
- `crates/ferrum-store/src/postgres/executions.rs`
- `crates/ferrum-store/src/postgres/rollback.rs`
- `crates/ferrum-store/src/postgres/provenance.rs`
- `crates/ferrum-store/src/postgres/approvals.rs`
- `crates/ferrum-store/src/postgres/ledger.rs`
- `crates/ferrum-store/src/postgres/migrations.rs` — PG-specific DDL

**Key differences from SQLite**:
- `TEXT` PKs → `UUID` columns
- `INTEGER` timestamps → `TIMESTAMPTZ`
- No `busy_timeout` — PG handles concurrent writes natively
- `DEFERRABLE INITIALLY DEFERRED` FK constraints for transaction batching
- Connection pool: 20 connections is appropriate for PG
- No need for write-queue — PG supports concurrent writers

### P3.2 — Schema Migration Script

**What**: Migration script from SQLite schema to PostgreSQL schema.

**Files to create**:
- `crates/ferrum-store/src/postgres/migrations/001_initial.sql`
- `tools/migrate-sqlite-to-pg.sh` — data migration helper

### P3.3 — Runtime Store Selection

**What**: Select SqliteStore or PostgresStore based on `store_dsn` scheme.

**Files to modify**:
- `crates/ferrum-gateway/src/state.rs` or wherever store is constructed
- Detect `sqlite:` vs `postgres:` DSN prefix
- Conditional feature flags in `Cargo.toml`: `features = ["sqlite"]` vs `features = ["postgres"]`

### P3.4 — Remove Retry Logic (Already Done in P1)

With PostgreSQL, retries are unnecessary. Already removed in P1.2.

### P3.5 — Update Config Files

**Files to modify**:
- `configs/ferrumgate.dev.toml` — Add PostgreSQL DSN example
- `configs/ferrumgate.prod.toml` — Default to PostgreSQL for production

### P3 Verification Checklist

- [ ] `cargo test --workspace` passes with both SQLite and PG features
- [ ] All 9 stress scenarios pass with PG backend
- [ ] S5: throughput > 200 pipelines/s
- [ ] S7: throughput > 500 writes/s with 50 workers
- [ ] Data migration script works correctly
- [ ] Config documentation updated

---

## Decision Matrix

| Criterion | Phase 1 (Write-Queue) | Phase 2 (Batching) | Phase 3 (PostgreSQL) |
|-----------|----------------------|--------------------|--------------------|
| Effort | Medium (1-2 days) | Medium (2-3 days) | Large (1-2 weeks) |
| Error elimination | 100% | +throughput | 100% + scale |
| Write throughput | ~8-10/s (same) | ~15-20/s | 1000+/s |
| S5 pipeline/s | ~2-3 | ~6-8 | 200+ |
| S7 p50 | ~400ms | ~200ms | <10ms |
| Deployment impact | None | None | New dependency |
| Risk | Low (same DB) | Low | Medium (new infra) |

## Dependency Graph

```
P1.1 (WriteQueue) ──▶ P1.2 (Remove retries) ──▶ P1.5 (Update stress tests)
P1.1 (WriteQueue) ──▶ P1.4 (Revoke F&F)
P1.3 (PRAGMA) ──────────── independent
P1.1-P1.5 ──────────────▶ P1.6 (Tests + verify)

Phase 1 complete ────────▶ P2.1 (Pipeline batch)
P2.1 ───────────────────▶ P2.2 (S5 batch writes)
P2.1 ───────────────────▶ P2.3 (Direct UPDATE)

Phase 2 complete ────────▶ P3.1 (PostgresStore)
P3.1 ───────────────────▶ P3.2 (Migration)
P3.1 ───────────────────▶ P3.3 (Runtime selection)
```
