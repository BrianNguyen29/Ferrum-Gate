# 24 — P1 / P2 / P3 Phased Execution Plan

Single authoritative tracking document for FerrumGate implementation order.
Grounded in current repo reality (tracing exists; metrics do not; TLS ingress
runbook exists but production posture needs consolidation; Sync-3a probe exists
in `ferrum-sync` crate; Sync-3a.1 status needs reconciliation; write-path and
consensus not implemented).

ASCII only. Docs-only changes; no Rust/code edits.

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
| P1.1b | Add Prometheus metrics endpoint | **TODO** | `GET /metrics` returns Prometheus text format |
| P1.1c | Add request-count, latency-histogram, error-rate metrics | **TODO** | Metrics endpoint exposes `ferrum_gateway_http_requests_total`, `ferrum_gateway_http_request_duration_seconds`, `ferrum_gateway_errors_total` |
| P1.1d | Add capacity planning notes to ops runbook | **TODO** | `docs/runbooks/ops-tls-ingress-runbook.md` or new runbook covers DB size, connection limits |

**Dependencies:** P1.1a is already done; P1.1b must precede P1.1c.

**Touchpoints:**
- `crates/ferrum-gateway/src/server.rs`
- `crates/ferrum-gateway/src/metrics.rs` (new)

### P1.2 — TLS / Ingress Story

**Goal:** Document external terminator requirements and operationalize deployment.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.2a | TLS ingress runbook exists | DONE | `docs/runbooks/ops-tls-ingress-runbook.md` covers nginx TLS termination |
| P1.2b | Confirm runbook is consistent with current `configs/ferrumgate.prod.toml` | **TODO** | Runbook commands match actual config keys and CLI flags |
| P1.2c | Add cert rollover + verification steps to runbook | **TODO** | Runbook section covers `openssl s_client` verification after reload |
| P1.2d | Document external terminator requirements in `15-deployment-and-operations.md` | **TODO** | Section 2 of `15-deployment-and-operations.md` explicitly states "no in-process TLS listener" |

**Dependencies:** P1.2a is done; all other P1.2 slices are P1 backlog.

**Touchpoints:**
- `docs/runbooks/ops-tls-ingress-runbook.md`
- `docs/15-deployment-and-operations.md`
- `configs/ferrumgate.prod.toml`

### P1.3 — Operational Runbook

**Goal:** Repeatable production deployment without bespoke debugging.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.3a | Startup failure diagnostics exist | DONE | `docs/17-troubleshooting.md` has startup-failure section |
| P1.3b | Add backup/restore procedures for SQLite persistence layer | **TODO** | Runbook covers `sqlite3 .backup` and restore procedure |
| P1.3c | Add capacity planning notes (DB size, connection limits) | **TODO** | Runbook covers expected DB growth, `max_connections` config |

**Dependencies:** P1.3a is done; P1.3b and P1.3c are P1 backlog.

**Touchpoints:**
- `docs/17-troubleshooting.md`
- New runbook section or `docs/runbooks/ops-tls-ingress-runbook.md`

### P1.4 — Poisoned-Context Fixture Breadth

**Goal:** >= 80% catch rate on expanded fixture set.

| Slice | Description | Status | Verification |
|-------|-------------|--------|--------------|
| P1.4a | 5/5 pass on curated poisoned-context regression suite | DONE | `tests/integration_poisoned_context.rs` passes |
| P1.4b | Expand fixture library beyond 5 curated scenarios | **TODO** | Test count >= 20; average fixture breadth score >= 80% |

**Dependencies:** None (backlog item).

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
| P2.2a | Assess `ferrum-sync` crate: does it satisfy Sync-3a, Sync-3a.1, or neither? | **TODO** | `crates/ferrum-sync/README.md` accurately reflects what is implemented |
| P2.2b | Update `22a-sync-3a1-probe-api-boundary.md` status to reflect actual implementation | **TODO** | Doc status line reflects "Implemented in ferrum-sync crate" or "Plan only" accurately |
| P2.2c | If gaps exist, complete Sync-3a.1 implementation | **TODO** | `ProbeFacade` satisfies all facade contract items in `22a-sync-3a1-probe-api-boundary.md` |

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
| P2.3d | Identify gaps between Sync-1 sketch and implementable protocol | **TODO** | List of open questions from Sync-1, Sync-2, Sync-3 marked "deferred" with owner/time estimates |
| P2.3e | Begin Sync-1 protocol implementation | **TODO** | `ferrum-sync` gains `Sync1Protocol` type; unit tests for preflight + decision table |

**Dependencies:** P2.2 (Sync-3a.1 reconciliation) should complete before P2.3e begins.

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
    P2.2a -> P2.2b -> P2.2c
  P2.3 Sync-1 prep
    P2.3a-c DONE -> P2.3d -> P2.3e
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
| Prometheus metrics (P1 todo) | None | No metrics endpoint yet |
| TLS ingress runbook (P1 done) | `docs/runbooks/ops-tls-ingress-runbook.md` | nginx termination; needs consolidation |
| Sync-3a probe (done) | `crates/ferrum-sync/src/facade.rs` | `ProbeFacade` implemented |
| Sync-3a.1 boundary (needs reconciliation) | `crates/ferrum-sync/README.md` vs `22a-sync-3a1-probe-api-boundary.md` | Doc says "no implementation"; code has `ProbeFacade` |
| Write-path (not started) | None | Out of scope for v1 |
| Consensus (not started) | None | Out of scope for v1 |

---

## References

- Production readiness assessment: `23-production-readiness-assessment.md`
- Current state: `01-current-state.md`
- Next issue backlog: `08-next-issue-backlog.md`
- Remaining tasks: `11-remaining-tasks.md`
- Sync plan docs: `18-cross-node-ledger-sync-plan.md` through `22a-sync-3a1-probe-api-boundary.md`
