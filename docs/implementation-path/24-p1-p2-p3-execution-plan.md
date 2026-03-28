# 24 — P1 / P2 / P3 Phased Execution Plan

Single authoritative tracking document for FerrumGate implementation order.
Grounded in current repo reality (tracing exists; metrics do not; TLS ingress
runbook exists but production posture needs consolidation; Sync-3a probe exists
in `ferrum-sync` crate; Sync-3a.1 facade boundary is fully implemented and
closed; Sync-1 decision kernel is implemented; write-path and
consensus not implemented).

ASCII only.

---

## Status Summary

| Phase | Name | Status |
|-------|------|--------|
| P1 | Single-node production hardening | **In progress** |
| P2 | Provenance tooling + sync prep | **Planned** |
| P3 | Sync implementation | **Planned** |

---

## P1 — Single-Node Production Hardening

**Must-have before any production deployment.** Estimated 0-2 weeks.

### P1.1 — Observability Baseline

**Goal:** Structured logging + metrics endpoint for single-node production.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.1a | Confirm `tracing` exists on gateway hot paths | DONE | `crates/ferrum-gateway/src/server.rs` has tracing spans |
| P1.1b | Prometheus metrics endpoint exists | DONE | `GET /metrics` returns Prometheus text format; defined at `server.rs:112`, auth-protected via bearer middleware |
| P1.1c | Request-count, latency, and error metrics are instrumented | DONE | Metrics endpoint exposes `ferrum_gateway_http_requests_total`, `ferrum_gateway_http_request_duration_seconds`, `ferrum_gateway_errors_total` |
| P1.1d | Capacity planning notes exist in ops runbook | DONE | `docs/runbooks/ops-sqlite-backup-runbook.md` covers DB growth, disk headroom, and concurrency guidance |

**Dependencies:** P1.1a through P1.1d are DONE (2026-03-27 gateway instrumentation + ops docs).

**Touchpoints:**
- `crates/ferrum-gateway/src/server.rs`
- `crates/ferrum-gateway/src/metrics.rs` (new)

### P1.2 — TLS / Ingress Story

**Goal:** Document external terminator requirements and operationalize deployment.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.2a | TLS ingress runbook exists | DONE | `docs/runbooks/ops-tls-ingress-runbook.md` covers nginx TLS termination |
| P1.2b | Confirm runbook is consistent with current `configs/ferrumgate.prod.toml` | DONE | Runbook prerequisites reference `configs/ferrumgate.prod.toml`; all CLI flags and config keys match actual prod config |
| P1.2c | Cert rollover + verification steps in runbook | DONE | Runbook Certificate rollover section (lines 156-160) covers `openssl s_client` verification after reload |
| P1.2d | External terminator requirements documented | DONE | `docs/15-deployment-and-operations.md` lines 15, 55 explicitly state "no in-process TLS listener"; TLS ingress runbook line 5 confirms |

**Dependencies:** P1.2a through P1.2d are all DONE (2026-03-27 doc reconciliation).

**Touchpoints:**
- `docs/runbooks/ops-tls-ingress-runbook.md`
- `docs/15-deployment-and-operations.md`
- `configs/ferrumgate.prod.toml`

### P1.3 — Operational Runbook

**Goal:** Repeatable production deployment without bespoke debugging.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.3a | Startup failure diagnostics exist | DONE | `docs/17-troubleshooting.md` has startup-failure section |
| P1.3b | SQLite backup/restore runbook | DONE | `ops-sqlite-backup-runbook.md` covers `sqlite3 .backup`, restore, and verification steps |
| P1.3c | Capacity planning notes | DONE | `ops-sqlite-backup-runbook.md` covers expected DB growth, disk headroom, and concurrency guidance |

**Dependencies:** P1.3a through P1.3c are all DONE (2026-03-27 docs update).

**Touchpoints:**
- `docs/17-troubleshooting.md`
- New runbook section or `docs/runbooks/ops-tls-ingress-runbook.md`

### P1.4 — Poisoned-Context Fixture Breadth

**Goal:** >= 80% catch rate on expanded fixture set.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.4a | 5/5 pass on curated poisoned-context regression suite | DONE | `tests/integration_poisoned_context.rs` passes |
| P1.4b | Expanded poisoned-context fixture library | DONE | `tests/integration_poisoned_context.rs` now contains 26 tokio tests covering broader poisoned-context cases |

**Dependencies:** P1.4a and P1.4b are DONE (2026-03-27 test expansion); breadth-score formalization can stay future-facing if needed.

**Touchpoints:**
- `tests/integration_poisoned_context.rs`

---

## P2 — Provenance Tooling + Sync Prep

**After P1 is complete.** Estimated 2-8 weeks.

### P2.1 — Provenance Query / Read-Model Enhancement

**Goal:** Advanced replay/query fabric tooling.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P2.1a | Core query surface implemented | DONE | `POST /v1/provenance/query` with filters; `ferrum-graph` helpers (`terminal_events`, `walk_backwards_from`, `walk_forwards_from`) |
| P2.1b | Advanced replay/fabric tooling | **TODO** | New endpoints or CLI commands for replay, graph traversal, audit export |

**Dependencies:** P2.1a is done; P2.1b is P2 backlog.

**Touchpoints:**
- `crates/ferrum-proto/src/provenance.rs`
- `crates/ferrum-store/src/sqlite/provenance.rs`
- `crates/ferrum-gateway/src/server.rs`

### P2.2 — Sync-3a.1 Probe API Boundary Reconciliation

**Goal:** Resolve discrepancy between doc (says "no implementation") and code
(`ferrum-sync` crate has `ProbeFacade`).

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P2.2a | Assess `ferrum-sync` crate: does it satisfy Sync-3a, Sync-3a.1, or neither? | **DONE** | Assessment recorded: Sync-3a is done; Sync-3a.1 is partial and has explicit remaining gaps |
| P2.2b | Update `22a-sync-3a1-probe-api-boundary.md` status to reflect actual implementation | **DONE** | Doc status line reflects partial implementation plus remaining gaps |
| P2.2c | Close remaining Sync-3a.1 gaps: add `leader_address`, enforce `timeout_per_probe_ms` per-call via `tokio::time::timeout` wrapping every transport call (maps to A7 on expiry), narrow transport re-exports to facade boundary | **DONE** | `cargo test -p ferrum-sync` passes; `facade_timeout_per_probe_ms_enforced_through_facade` proves timeout fires through the full facade path; `lib.rs` re-exports only `LeaderTip` from transport; docs updated to reflect completion |

**Dependencies:** None (can run in parallel with other P2 work).

**Touchpoints:**
- `crates/ferrum-sync/src/facade.rs`
- `crates/ferrum-sync/README.md`
- `docs/implementation-path/22a-sync-3a1-probe-api-boundary.md`

### P2.3 — Sync-1 Protocol Implementation Prep

**Goal:** Prepare for Sync-1 protocol implementation after Sync-3a.1 is reconciled.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P2.3a | Sync-0 safety contract plan exists | DONE | `18-cross-node-ledger-sync-plan.md` defines L1-L3, C1-C3, F1-F4, EC1-EC5 |
| P2.3b | Sync-1 protocol sketch exists | DONE | `19-sync-1-protocol-sketch.md` defines one-way fast-forward protocol |
| P2.3c | Sync-2 read-only preflight + diff classifier sketch exists | DONE | `20-sync-2-read-only-preflight-diff-classifier.md` defines DiffClass enum + decision table |
| P2.3d | Identify gaps between Sync-1 sketch and implementable protocol | **DONE** | Gap inventory added below; Sync-1/Sync-2/Sync-3 open questions marked "deferred" with owner and rough next-step estimates |
| P2.3e | Begin Sync-1 protocol implementation (decision kernel) | **DONE** | `ferrum-sync` has `decide()` function in `decision.rs`; exhaustive unit tests for DONE / SYNC / FAST_FORWARD / ABORT rows |
| P2.3f | Sync-2 groundwork: read-only preflight (PF1-PF8) + diff classifier (`DiffClass`) + bridge to Sync-1 | **DONE** | `ferrum-sync` has `preflight.rs` with `classify()`, `run_preflight()`, `diff_class_to_decision()`, `PreflightInput`, and roundtrip tests against `decide()` |

**Dependencies:** P2.2 (Sync-3a.1 reconciliation) should complete before P2.3e begins.

### P2.3 Gap Inventory (Sync-1 / Sync-2 / Sync-3 Deferred Items)

The following gaps remain after P2.3a-P2.3f. Each is tagged with owner and
rough next-step estimate:

| Gap | Owner | Phase | Estimate | Next Step |
|-----|-------|-------|----------|-----------|
| Sync-1: hash-path continuity check requires actual ledger query | P3 | Sync-1 impl | 1-2 days | Wire `hash_path_valid` field to `verify_chain()` result from `ferrum-ledger` |
| Sync-2: PF1/PF5 wired via `SqliteSyncPreflightRepo` in ferrum-store | P2 | Sync-2 impl | DONE | `verify_local_chain()` implemented |
| Sync-2: PF2/PF6/PF7 state wired via `SqliteSyncPreflightRepo` in ferrum-store | P3 | Sync-2 impl | DONE | `sync_state` table (migration 003) added; `read_local_state()` now returns real `LocalPreflightState`; `set_sync_flags_test_only()` provided for test scenarios |
| Sync-2: PF8 leader tip cache | P2 | Sync-2 impl | DONE | `leader_tips` table (migration 002) added; `SqliteSyncPreflightRepo::read_leader_tip()` wired; cache write path provided for transport layer |
| Sync-2: PF4 capability model enforcement (leader authorization) | P3 | Sync-2 impl | 2-3 days | Wire to `ferrum-capability` store; check leader authorization |
| Sync-2: PF3 transport-boundary helper (pure, fail-closed on empty address) | P2 | Sync-2 impl | DONE | `PreflightTransportInput::evaluate()` in `ferrum-sync/src/transport.rs`; actual leader address acquisition still requires transport probe |
| Sync-2: PF3/PF8 require transport-based leader tip acquisition (actual probe) | P3 | Sync-3 impl | 5-8 days | Implement HTTP/gRPC transport probe; cache result via `LeaderTipCache::write()` |
| Sync-2: PF7 sync session tracking (stateful, not read-only) | P3 | Sync-1 impl | 1-2 days | Add in-memory `AtomicBool` or DB-backed session flag |
| Sync-3: real HTTP/gRPC transport (not FakeTransport) | P3 | Sync-3 impl | 5-10 days | Implement `Transport` trait with `reqwest` or `tonic`; integration tests |
| Sync-1: entry apply/write-path (follower side) | P3 | Write-path | 10+ days | Design doc first; then implement atomic entry application with rollback |
| Sync-1: retry/backoff on transient failure | P3 | Sync-3 impl | 2-3 days | Add exponential backoff to transport layer |
| Sync-2: `LeaderAheadEmpty` variant unused in `classify()` | P3 cleanup | Sync-2 impl | 0.5 days | Either wire into classify or remove; currently `Bootstrap` covers the case |
| Consensus / leader election | Future | Beyond P3 | Unknown | Requires Raft or similar; full design doc needed first |

**Touchpoints:**
- `crates/ferrum-sync/src/`
- `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- `docs/implementation-path/19-sync-1-protocol-sketch.md`
- `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`

### P2.4 — HA / Multi-Node Readiness Analysis

**Goal:** Document SQLite read-replica use cases and leader-election requirements.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P2.4a | Document SQLite read-replica use cases | **TODO** | New section in `docs/implementation-path/` or `docs/15-deployment-and-operations.md` |
| P2.4b | Analyze leader-election requirements for future implementation | **TODO** | Decision doc: leader-election approach (Raft, etc.) with rationale |

**Dependencies:** None (exploratory/analysis only).

**Touchpoints:**
- `docs/15-deployment-and-operations.md`
- `docs/implementation-path/18-cross-node-ledger-sync-plan.md`

---

## P3 — Sync Implementation

**Only after P2 is complete.** Estimated 8+ weeks.

### P3.1 — Sync-1 Protocol Implementation

**Goal:** One-way fast-forward model with preflight + decision table.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P3.1a | Implement Sync-1 preflight checks (PF1-PF8) | **TODO** | `Sync1Protocol::preflight()` returns `PreflightResult` |
| P3.1b | Implement Sync-1 decision table | **TODO** | `Sync1Protocol::decide()` maps DiffClass to Sync1Decision |
| P3.1c | Implement abort semantics (A1-A8, S1-S2) | **TODO** | All abort triggers return `Sync1AbortCode`; local chain unchanged |
| P3.1d | Integration tests with `FakeLeaderTransport` | **TODO** | `ferrum-sync` tests pass with `FakeLeaderTransport` injected |

**Dependencies:** P2.3e.

**Touchpoints:**
- `crates/ferrum-sync/src/protocol.rs` (new)
- `crates/ferrum-sync/src/facade.rs`

### P3.2 — Sync-2 Read-Only Preflight + Diff Classifier Implementation

**Goal:** Implement DiffClass classifier for local-only decision support.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P3.2a | Implement `DiffClassifier::classify()` | **TODO** | Unit tests cover all DiffClass variants |
| P3.2b | Implement extended preflight (PF5-PF8) | **TODO** | All checks are local-only queries; no network calls |

**Dependencies:** P3.1a.

**Touchpoints:**
- `crates/ferrum-sync/src/classifier.rs` (new)

### P3.3 — Sync-3 Transport Implementation

**Goal:** Implement leader-tip + proof retrieval with fail-closed error mapping.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P3.3a | Implement `Transport` trait for HTTP/gRPC | **TODO** | `Transport` impl communicates with leader node |
| P3.3b | Implement leader-tip retrieval | **TODO** | `LeaderTipRequest/Response` round-trip works |
| P3.3c | Implement proof retrieval + local verification | **TODO** | `verify_proof()` correctly detects valid/invalid proofs |
| P3.3d | Implement error mapping (TransportError -> Sync1AbortCode) | **TODO** | All `TransportError` variants map to correct abort code per `21-sync-3-transport-sketch.md` |

**Dependencies:** P2.2 (Sync-3a.1 reconciliation confirms contract).

**Touchpoints:**
- `crates/ferrum-sync/src/transport.rs`
- `crates/ferrum-sync/src/error.rs`

### P3.4 — Write-Path Implementation (Future / Out of Scope for v1)

**Goal:** Apply entries on follower side after Sync-1 decision table says SYNC.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P3.4a | Write-path design doc | **TODO** | `docs/implementation-path/24-write-path-design.md` exists |
| P3.4b | Implement write-path | **TODO** | Entries written to local SQLite ledger atomically |

**Note:** Write-path is explicitly deferred beyond current v1 scope.
Consensus and two-way merge are also out of scope for v1.

---

## Phase Ordering and Dependencies

```
P1 (0-2 weeks)
  P1.1 observability
    P1.1a DONE -> P1.1b -> P1.1c -> P1.1d
  P1.2 TLS/ingress
    P1.2a DONE -> P1.2b -> P1.2c -> P1.2d
  P1.3 runbook
    P1.3a DONE -> P1.3b -> P1.3c
  P1.4 fixture breadth (backlog)
    P1.4a DONE -> P1.4b

P2 (2-8 weeks, after P1)
  P2.1 provenance tooling
    P2.1a DONE -> P2.1b
  P2.2 Sync-3a.1 reconciliation
    P2.2a DONE -> P2.2b DONE -> P2.2c DONE
  P2.3 Sync-1 prep
    P2.3a-c DONE -> P2.3d DONE -> P2.3e DONE (decision kernel) -> P2.3f DONE (Sync-2 groundwork)
  P2.4 HA analysis (parallel)

P3 (8+ weeks, after P2)
  P3.1 Sync-1 implementation
    P2.3e -> P3.1a -> P3.1b -> P3.1c -> P3.1d
  P3.2 Sync-2 implementation
    P3.1a -> P3.2a -> P3.2b
  P3.3 Sync-3 implementation
    P2.2 -> P3.3a -> P3.3b -> P3.3c -> P3.3d
  P3.4 write-path (v2 / future)
    P3.3 -> P3.4
```

---

## Key Repo State References

| Topic | File | Notes |
|-------|------|-------|
| Tracing (P1 done) | `crates/ferrum-gateway/src/server.rs` | Structured logging exists |
| Prometheus metrics (P1.1b done) | `crates/ferrum-gateway/src/server.rs:112` | `GET /metrics` returns Prometheus text format; bearer-auth protected like other non-health endpoints |
| TLS ingress runbook (P1.2 done) | `docs/runbooks/ops-tls-ingress-runbook.md` | nginx termination; cert rollover and external terminator requirements documented; consistent with `ferrumgate.prod.toml` |
| Sync-3a probe (done) | `crates/ferrum-sync/src/facade.rs` | `ProbeFacade` implemented |
| Sync-3a.1 boundary (complete) | `crates/ferrum-sync/src/facade.rs` | Facade complete: `leader_address`, per-call timeout enforcement, and narrower crate-root transport surface are in place |
| Sync-1 decision kernel (done) | `crates/ferrum-sync/src/decision.rs` | Pure `decide()` function; exhaustive unit tests |
| Sync-2 groundwork (done) | `crates/ferrum-sync/src/preflight.rs` | `classify()`, `run_preflight()`, `diff_class_to_decision()` with unit and roundtrip tests |
| Sync-2 repo port | `crates/ferrum-sync/src/repo.rs` + `crates/ferrum-store/src/sqlite/sync_preflight.rs` | `SyncPreflightRepo` trait, `LocalPreflightState`, `SyncRepoError`, `InMemorySyncPreflightRepo` (test double) in ferrum-sync; `SqliteSyncPreflightRepo` (PF1+PF5+PF2+PF6+PF7+PF8 real, PF4 fail-closed) in ferrum-store |
| Sync-2 PF3/PF8 transport-boundary helpers | `crates/ferrum-sync/src/transport.rs` | `PreflightTransportInput`, `PreflightTransportFlags`, `PreflightTransportInput::evaluate()` — pure, fail-closed helpers for converting leader address + cached tip into PF3/PF8 booleans |
| Sync-2 PF8 leader tip cache | `crates/ferrum-store/src/sqlite/leader_tip_cache.rs` + `migrations/002_add_leader_tips.sql` | `LeaderTipCache` struct; `leader_tips` table (leader_address PK, sequence, hash, fetched_at); `read()`, `write()`, `delete()` methods |
| Sync-2 PF2/PF6/PF7 sync state | `crates/ferrum-store/src/sqlite/sync_preflight.rs` + `migrations/003_add_sync_state.sql` | `sync_state` table (id=1, has_inflight_commits, has_uncommitted_entries, sync_in_progress); `read_local_state_async()` and `set_sync_flags_test_only()` in `SqliteSyncPreflightRepo` |
| Write-path (not started) | None | Out of scope for v1 |
| Consensus (not started) | None | Out of scope for v1 |

---

## References

- Production readiness assessment: `23-production-readiness-assessment.md`
- Current state: `01-current-state.md`
- Next issue backlog: `08-next-issue-backlog.md`
- Remaining tasks: `11-remaining-tasks.md`
- Sync plan docs: `18-cross-node-ledger-sync-plan.md` through `22a-sync-3a1-probe-api-boundary.md`
