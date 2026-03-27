# 23 — Production Readiness Assessment and Phased Hardening Plan

## Overview

This document assesses FerrumGate's current production-readiness posture and
provides a phased plan for closing the remaining gaps. Claims are grounded in
current repo state only.

**Supported single-node flow has solid automated-test evidence.** The SQLite-backed
gateway flow, firewall enforcement, adapter-backed recovery, and provenance chain
have passing integration tests. Without observability and TLS, the posture is
best described as **pilot-ready / solid-evidence** rather than defensibly
production-ready.

**Broader production posture (HA, multi-node sync, observability) remains
incomplete.** The gaps below are honestly documented.

---

## 1. What Is Production-Ready (Supported Single-Node)

The following have automated-test evidence. They represent a solid foundation;
they do not constitute a defensibly production-ready deployment without the
P1 observability and TLS items in Section 2 being addressed first:

### 1.1 Core Gateway Flow

| Capability | Status | Evidence |
|------------|--------|----------|
| Intent compile + trust context | Supported | `test_compile_intent_persists_and_emits_provenance` |
| Proposal evaluate (Allow/Deny/Quarantine/RequireApproval/AllowDraftOnly) | Supported | `test_evaluate_proposal_loads_real_intent_and_persists` |
| Capability mint + authorize + single-use | Supported | `test_full_happy_path_flow_compile_evaluate_mint_authorize_prepare` |
| Execute + verify + auto-commit (R0) | Supported | `test_full_happy_path_execute_verify_auto_commit` |
| Execute + verify + explicit commit (R2) | Supported | `test_r2_no_auto_commit_verify_then_explicit_commit` |
| R3 RequireApproval governance path | Supported | `test_evaluate_proposal_with_r3_intent_requires_approval` |
| Rollback + compensate terminal paths | Supported | Multiple rollback tests in `integration_gateway_flow.rs` |
| Provenance lineage chain persistence | Supported | `integration_lineage_chain.rs` (5/5 pass) |
| Fail-closed provenance query endpoint | Supported | `POST /v1/provenance/query` at `server.rs:2192` |

### 1.2 Firewall / Policy Enforcement

| Enforcement | Status | Evidence |
|-------------|--------|----------|
| Trust labeling at compile time | Supported | `integration_poisoned_context.rs` |
| Taint propagation into evaluate | Supported | `test_taint_propagates_into_evaluate_decisions` |
| Read-only contradiction blocking | Supported | `test_read_only_intent_fails_closed_against_mutation_proposal` |
| MCP scope contradiction blocking | Supported | `test_mcp_scope_constraints_fail_closed_with_poisoned_context` |
| Execution-time File enforcement | Supported | `test_file_path_mismatch_denies`, `test_file_traversal_denied` |
| Execution-time Http enforcement | Supported | `test_http_host_method_header_mismatch_denies` |
| Execution-time Sqlite enforcement | Supported | `test_sqlite_db_path_table_violations_denied` |
| Execution-time Git enforcement | Supported | `test_git_repo_path_ref_violations_denied` |
| Execution-time EmailDraft enforcement | Supported | `test_email_draft_recipient_send_violations_denied` |
| DLP redact/detect | Supported | `test_dlp_redacts_secrets_in_output` |
| Poisoned-context quarantine | Supported | `test_quarantine_path_blocks_execution_advance` (5/5 pass) |

### 1.3 Adapter-Backed Recovery

| Adapter | Recovery Evidence | Notes |
|---------|------------------|-------|
| Filesystem | file create/delete, overwrite/restore | Full parity |
| Sqlite | row restore via transaction | Full parity |
| Maildraft | draft create/delete recovery | Draft-only; `allow_send=true` denied at prepare-time |
| Git | local ref restore | Full parity |
| HTTP | GET/POST/PUT/PATCH/DELETE execute/verify | Full parity execute path; rollback is conservative no-op |

### 1.4 Durable Persistence

- SQLite-backed persistence for all core domain objects
- Restart-safe capability persistence
- Provenance edges persisted to `provenance_edges` table
- Ledger hash-chain with live append-time verification (Commits A-C complete)

---

## 2. Known Production Gaps

The following are NOT in production-ready state for single-node deployment:

### 2.1 TLS / Ingress (P1 — Unresolved)

| Gap | Impact | Workaround |
|-----|--------|------------|
| No in-process TLS listener | No TLS termination inside process | External terminator (nginx, cloud LB) required |
| No mTLS story | No mutual-auth between nodes | N/A — cross-node not implemented yet |

**Source**: `docs/15-deployment-and-operations.md` lines 15, 55

### 2.2 Observability / Telemetry (P1 — Unresolved)

| Gap | Impact |
|-----|--------|
| No structured logging (tracing) | Hard to debug in production |
| No metrics / Prometheus endpoint | No visibility into gateway behavior |
| No distributed trace context | Cannot correlate cross-node operations |
| No alerting rules | No automated failure notification |

**Source**: derived from current `server.rs` observability surface

### 2.3 HTTP Rollback / Compensation (Ratified Boundary — No-Op)

HTTP adapter rollback is intentionally conservative **no-op**. Remote mutation
rollback requires manual operator R3 compensation. This is a ratified boundary
per `16a-slice-16-a-boundary-ratification.md`.

**Source**: `crates/ferrum-adapter-http/src/lib.rs:1079`, `crates/ferrum-gateway/src/server.rs:2660`

### 2.4 EmailSend Governed Path (Ratified Boundary — Explicit Deny)

`EmailDraft allow_send=true` is **explicitly denied at gateway prepare-time**
(PolicyDenied 403). EmailSend mutation recovery is not in scope for v1.
This is a ratified boundary per `16a-slice-16-a-boundary-ratification.md`.

**Source**: `crates/ferrum-gateway/src/server.rs:1149`

### 2.5 Cross-Node Sync (P2 — Planned, Not Implemented)

The following are **planning documents only**; no implementation exists:

| Slice | Document | Status |
|-------|----------|--------|
| Sync-0 safety contract discovery | `18-cross-node-ledger-sync-plan.md` | Plan only |
| Sync-1 protocol sketch | `19-sync-1-protocol-sketch.md` | Plan only |
| Sync-2 read-only preflight + diff classifier | `20-sync-2-read-only-preflight-diff-classifier.md` | Plan only |
| Sync-3 transport sketch | `21-sync-3-transport-sketch.md` | Plan only |
| Sync-3a read-only transport probe | `22-sync-3a-read-only-transport-probe.md` | **Implemented in `ferrum-sync` crate** |
| Sync-3a.1 probe API boundary | `22a-sync-3a1-probe-api-boundary.md` | Plan only |

The `ferrum-sync` crate provides the Sync-3a read-only transport probe
(`ProbeFacade`, `TransportProbe`, `FakeLeaderTransport`). This is a
**diagnostic-only** tool; it does not implement the write-path, consensus,
or two-way merge.

### 2.6 Generic Provenance Query / Replay Fabric (P2 — Core Done, Advanced TBD)

Core query surface is implemented:
- `POST /v1/provenance/query` with filters
- `ferrum-graph` read-model helpers (`terminal_events`, `walk_backwards_from`, `walk_forwards_from`)
- `GET /v1/provenance/lineage/{execution_id}`

Advanced replay/query fabric tooling remains P2 backlog.

**Source**: `crates/ferrum-proto/src/provenance.rs:86`, `crates/ferrum-store/src/sqlite/provenance.rs:142`

### 2.7 HA / Multi-Node Control Plane (P2 — Not Planned)

No HA story exists. Single-node SQLite persistence is the only supported
deployment model.

---

## 3. Phased Hardening Plan

For detailed ordered slices, status checkboxes, dependencies, and verification
expectations, see `24-p1-p2-p3-execution-plan.md`. The summary below is
grounded in current repo reality.

### Phase P1 — Single-Node Production Hardening (0-2 weeks)

Immediate items to close before single-node production deployment:

1. **Observability baseline**
   - `tracing` structured logging exists on gateway hot paths (DONE)
   - Prometheus metrics endpoint exists at `/metrics` (DONE)
   - Request counts, latency histograms, and error-rate metrics are instrumented (DONE)
   - No distributed trace context needed yet (single-node)

2. **TLS / Ingress story**
   - TLS ingress runbook exists at `docs/runbooks/ops-tls-ingress-runbook.md` (DONE)
   - Runbook is consistent with `configs/ferrumgate.prod.toml` (DONE)
   - External terminator requirements are documented in `15-deployment-and-operations.md` (DONE)

3. **Operational runbook**
   - Startup failure diagnostics (already in `17-troubleshooting.md`) (DONE)
   - SQLite backup/restore procedures exist in `docs/runbooks/ops-sqlite-backup-runbook.md` (DONE)
   - Capacity planning notes exist in `docs/runbooks/ops-sqlite-backup-runbook.md` (DONE)

4. **Poisoned-context fixture breadth** (P1 backlog)
   - 5/5 pass on curated regression suite (DONE)
   - Expanded fixture library beyond 5 curated scenarios (DONE; 26 tokio tests in `tests/integration_poisoned_context.rs`)
   - Target: keep the broader suite stable and extend scoring formalization later if needed

**Exit criteria**: Metrics instrumentation is useful beyond an empty endpoint;
TLS/ingress and SQLite ops runbooks stay aligned with code/config reality; pilot
deployment is repeatable without bespoke debugging.

### Phase P2 — Multi-Node Preparation (2-8 weeks)

Items to prepare for multi-node deployment without implementing full sync:

1. **Sync-3a/Sync-3a.1 reconciliation**
   - `ferrum-sync` crate implements Sync-3a probe (DONE)
   - Sync-3a.1 boundary status needs reconciliation between doc and code (TODO)
   - Complete remaining Sync-3a.1 probe API boundary work (TODO)

2. **Observability for multi-node**
   - Distributed trace context (W3C TraceContext) (future)
   - Node identity + topology discovery docs (future)

3. **HA readiness analysis**
   - Document SQLite read-replica use cases (TODO)
   - Analyze leader election requirements for future implementation (TODO)

**Exit criteria**: Multi-node architecture is documented; sync implementation can begin.

### Phase P3 — Sync Implementation (8+ weeks)

Only after Phase P2 is complete:

1. Implement Sync-1 protocol (one-way fast-forward model)
2. Implement Sync-2 read-only preflight + diff classifier
3. Implement Sync-3 transport (write-path out of scope for v1)
4. Consensus / leader election (future work)

**Exit criteria**: Cross-node read sync works with at least one transport.

---

## 4. Honest Assessment Summary

| Dimension | Status | Notes |
|-----------|--------|-------|
| Single-node core flow | **Pilot-ready** | Solid automated-test evidence; needs observability before production |
| Single-node observability | **Gap** | No structured logging or metrics (P1) |
| Single-node TLS | **Gap** | External terminator required; in-process TLS not planned |
| Adapter coverage | **Evidence complete** | fs/sqlite/maildraft/git/http all have recovery evidence |
| Multi-node sync | **Gap** | Sync-3a probe done; write-path, consensus not started |
| HA / multi-leader | **Gap** | Not planned |
| Generic provenance tooling | **Core done, advanced TBD** | Query surface exists; replay fabric P2 |

**Bottom line**: The supported single-node flow has solid automated-test evidence
and is **pilot-ready**. It is not defensibly production-ready until observability
(tracing + Prometheus) and TLS/ingress runbook (P1) are closed. The remaining gaps
(HTTP rollback no-op, EmailSend explicit deny) are ratified boundaries, not
open defects.

---

## 5. Key Source Links

| Topic | Source |
|-------|--------|
| Phase F evidence | `docs/implementation-path/11-phase-f-evidence.md` |
| Phase F evidence pack | `docs/18-phase-f-evidence-pack.md` |
| Success criteria + KPIs | `docs/91-phase-success-criteria-and-kpis.md` |
| Release checklist | `docs/16-release-checklist.md` |
| Remaining tasks | `docs/implementation-path/11-remaining-tasks.md` |
| Next issue backlog | `docs/implementation-path/08-next-issue-backlog.md` |
| Sync plan documents | `docs/implementation-path/18-cross-node-ledger-sync-plan.md` through `22a-sync-3a1-probe-api-boundary.md` |
| P1/P2/P3 execution plan | `docs/implementation-path/24-p1-p2-p3-execution-plan.md` |
| Slice 16-A boundary ratification | `docs/implementation-path/16a-slice-16-a-boundary-ratification.md` |
| ferrum-sync crate | `crates/ferrum-sync/README.md` |
