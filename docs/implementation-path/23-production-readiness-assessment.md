# 23 — Production Readiness Assessment and Phased Hardening Plan

## Overview

This document assesses FerrumGate's current production-readiness posture and
provides a phased plan for closing the remaining gaps. Claims are grounded in
current repo state only.

**v1 Scope Freeze — Single-Node Only.** FerrumGate v1 is explicitly scoped to
single-node deployment. Multi-node sync (write-path, two-way merge, consensus),
HA/multi-leader, distributed trace context, in-process TLS, and alerting rules
are all post-v1 non-goals. See Section 5 for the complete v1 scope-freeze list.

**Supported single-node flow has solid automated-test evidence.** The SQLite-backed
gateway flow, firewall enforcement, adapter-backed recovery, and provenance chain
have passing integration tests. With the P1 observability baseline and TLS
ingress runbook in place, the posture is best described as **pilot-ready /
single-node production-candidate** rather than broadly production-ready.

**Broader production posture (HA, multi-node sync, advanced observability)
remains incomplete.** The gaps below are honestly documented.

---

## 1. What Is Supported for Single-Node v1 (v1 Scope Freeze)

**v1 closure target is single-node only.** The items in Section 1 constitute
the complete v1 supported surface. Section 2 items are out of scope for v1
and are not blockers for v1 RC sign-off.

The following have automated-test evidence. They represent the supported
single-node v1 surface. They do not constitute a broadly production-ready or
multi-node-ready deployment; the remaining gaps are documented in Section 2.

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

### 2.1 TLS / Ingress (P1 — Runbook Complete, In-Process TLS Out of Scope)

P1.2a through P1.2d are DONE (2026-03-27 TLS ingress runbook + ops docs).

| Gap | Status | Notes |
|-----|--------|-------|
| No in-process TLS listener | No TLS termination inside process | External terminator (nginx, cloud LB) required; runbook documents this |
| No mTLS story | No mutual-auth between nodes | N/A — cross-node not implemented yet |

**Source**: `docs/15-deployment-and-operations.md` lines 15, 55

### 2.2 Observability / Telemetry (P1 — Baseline Complete)

P1.1a through P1.1d are DONE (2026-03-27 gateway instrumentation + ops docs).
Distributed trace context and alerting rules are P2 future work (not needed for single-node).

| Gap | Status | Notes |
|-----|--------|-------|
| Structured logging (tracing) on gateway hot paths | **DONE** | `server.rs` has tracing spans on all gateway endpoints |
| Prometheus metrics endpoint at `/metrics` | **DONE** | Bearer-auth protected; exposes request count, latency histogram, error rate metrics |
| Distributed trace context (W3C TraceContext) | **Future (P2)** | Single-node; not needed for v1 |
| Alerting rules | **Future (P2)** | Exploratory/analysis only |
| Capacity planning notes in ops runbook | **DONE** | `ops-sqlite-backup-runbook.md` covers DB growth, disk headroom, concurrency |

**Source**: `crates/ferrum-gateway/src/server.rs:112` (metrics endpoint), `crates/ferrum-gateway/src/server.rs` (tracing spans)

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

### 2.5 Cross-Node Sync (P2 — Groundwork Present, Write-Path Not Implemented)

Sync planning docs exist for Sync-0 through Sync-3a. Several read-only or
decision-only slices are now implemented in `ferrum-sync`, but there is still
no write/apply path, no real transport adapter, no consensus, and no full
multi-node sync implementation.

| Slice | Document | Status |
|-------|----------|--------|
| Sync-0 safety contract discovery | `18-cross-node-ledger-sync-plan.md` | Plan only |
| Sync-1 protocol sketch | `19-sync-1-protocol-sketch.md` | Plan only |
| Sync-1 decision kernel | `19-sync-1-protocol-sketch.md` | **Implemented in `ferrum-sync` crate** (`decision.rs`: `decide()`) |
| Sync-2 read-only preflight + diff classifier | `20-sync-2-read-only-preflight-diff-classifier.md` | **Groundwork in `ferrum-sync` crate** (`preflight.rs`: `classify()`, `run_preflight()`, `diff_class_to_decision()`, `DiffClass`, `PreflightInput`, `PreflightResult`) |
| Sync-3 transport sketch | `21-sync-3-transport-sketch.md` | Plan only |
| Sync-3a read-only transport probe | `22-sync-3a-read-only-transport-probe.md` | **Implemented in `ferrum-sync` crate** |
| Sync-3a.1 probe API boundary | `22a-sync-3a1-probe-api-boundary.md` | **Implemented in `ferrum-sync` crate** (`ProbeFacade` with `leader_address`, per-call timeout enforcement, and narrower crate-root transport re-exports) |

The `ferrum-sync` crate now provides the Sync-3a read-only transport probe
(`ProbeFacade`, `TransportProbe`, `FakeLeaderTransport`), the Sync-3a.1 facade
boundary (`ProbeFacadeRequest`/`ProbeFacadeResponse` with `leader_address`,
per-call timeout enforcement via `tokio::time::timeout`, abort-code-only
failures, and narrower crate-root transport re-exports), a pure Sync-1
decision kernel (`decide()`), and partial Sync-2 groundwork
(`classify()`, `run_preflight()`, `diff_class_to_decision()` in `preflight.rs`).
These are **diagnostic/decision-only** tools; they do not implement the
write-path, real transport, consensus, or two-way merge. The Sync-2
groundwork remains partial: actual repo queries, transport-based tip
acquisition, sync session tracking, and capability model enforcement are
deferred to P3.

### 2.6 Generic Provenance Query / Replay Fabric (P2 — Core Done, Advanced TBD)

Core query surface is implemented:
- `POST /v1/provenance/query` with filters
- `ferrum-graph` read-model helpers (`terminal_events`, `walk_backwards_from`, `walk_forwards_from`)
- `GET /v1/provenance/lineage/{execution_id}`

Advanced replay/query fabric tooling remains P2 backlog.

**Source**: `crates/ferrum-proto/src/provenance.rs:86`, `crates/ferrum-store/src/sqlite/provenance.rs:142`

### 2.7 HA / Multi-Node Control Plane (P2 — Analysis Complete)

Single-node SQLite is the only supported deployment model. P2.4a SQLite
read-replica use-case analysis is complete at
`26-p2-sqlite-read-replica-use-cases.md` (sanctioned read-only use cases,
risks, non-goals, done criteria). P2.4b leader-election analysis is complete at
`27-p2-leader-election-requirements-analysis.md` (requirements LE1-LE6, NI1-NI3,
PR1-PR3; option comparison; Raft recommendation; minimal interface contract).
Full HA implementation is post-P2.

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
    - Sync-3a.1 boundary status reconciled between doc and code (DONE)
    - Sync-3a.1 remaining gaps closed: `leader_address`, timeout threading, transport DTO isolation (DONE)

2. **Observability for multi-node**
   - Distributed trace context (W3C TraceContext) (future)
   - Node identity + topology discovery docs (future)

 3. **HA readiness analysis**
   - Document SQLite read-replica use cases (DONE -- `26-p2-sqlite-read-replica-use-cases.md`)
   - Analyze leader election requirements for future implementation (DONE -- `27-p2-leader-election-requirements-analysis.md`)

**Exit criteria**: Multi-node architecture is documented; sync implementation can begin.

### Phase P3 — Sync Implementation (8+ weeks)

Only after Phase P2 is complete:

1. Implement Sync-1 protocol (one-way fast-forward model)
2. Implement Sync-2 read-only preflight + diff classifier
3. Implement Sync-3 transport (write-path out of scope for v1)
4. Consensus / leader election (future work)

**Exit criteria**: Cross-node read sync works with at least one transport.

---

## 4. v1 Scope Freeze — Post-v1 Non-Goals

The following are **explicitly out of scope for v1** and are not blockers for
v1 RC sign-off:

| Item | Status | Notes |
|------|--------|-------|
| Multi-node sync write-path | Post-v1 (P3) | Sync-3a probe done; apply/write-path, two-way merge not started |
| Two-way merge | Post-v1 | Not designed |
| Consensus / leader election | Post-v1 | Analysis complete; implementation deferred beyond P3 |
| HA / multi-leader | Post-v1 (P2 analysis complete) | SQLite read-replica and leader-election analysis are done; implementation still deferred |
| In-process TLS termination | Post-v1 | External terminator (nginx/cloud LB) required; documented in runbook |
| Distributed trace context (W3C TraceContext) | Post-v1 (P2) | Single-node; not needed for v1 |
| Alerting rules | Post-v1 (P2) | Exploratory/analysis only |
| Generic provenance replay/fabric tooling | Post-v1 (P2) | Core query surface DONE; advanced replay tooling P2 backlog |

**Bottom line:** v1 closure = single-node only. All items above are P2/P3
post-v1 work and do not block the single-node RC evidence pass.

---

## 5. Honest Assessment Summary

| Dimension | Status | Notes |
|-----------|--------|-------|
| Single-node core flow | **Pilot-ready** | Solid automated-test evidence; supported v1 scope is single-node only |
| Single-node observability | **P1 baseline complete** | Tracing + Prometheus DONE (P1); distributed trace + alerting P2 |
| Single-node TLS | **Gap (runbook done)** | External terminator required; in-process TLS out of scope for v1 |
| Adapter coverage | **Evidence complete** | fs/sqlite/maildraft/git/http all have recovery evidence |
| Multi-node sync | **Gap** | Sync-3a probe done; write-path, consensus not started |
| HA / multi-leader | **Gap** | Not planned |
| Generic provenance tooling | **Core done, advanced TBD** | Query surface exists; replay fabric P2 |

**Bottom line**: The supported single-node flow has solid automated-test evidence
and is **pilot-ready**. P1 observability baseline (tracing + Prometheus) and
TLS/ingress runbook are complete. Remaining gaps (distributed trace context,
alerting rules, in-process TLS, write-path sync) are future P2/P3 work out of
scope for v1. The ratified boundaries (HTTP rollback no-op, EmailSend explicit
deny) are confirmed design decisions, not open defects.

---

## 5b. v1 Closure Decision

**Single-node v1 is ready to close.** All RC evidence gates are green:
clippy passes, tests pass, startup guard passes, smoke server responds correctly,
readiness endpoint returns 200, metrics endpoint enforces auth correctly, and
SQLite backup/integrity checks pass. Post-v1 items (multi-node sync write-path,
HA/multi-leader, in-process TLS, distributed trace context, alerting rules,
generic provenance replay fabric) are explicitly out of scope and do not block
v1 RC sign-off.

---

## 6. Key Source Links

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
