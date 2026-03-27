# 18 - Phase F Evidence Pack

## Overview

This document is the consolidated Phase F evidence pack for FerrumGate. It
covers verified supported flows, poisoned-context evidence status, open gaps,
and handoff readiness.

This document is ASCII-only and commit-ready. Claims are grounded in existing
repo docs and tests only; no unsupported flows or evidence are invented.

---

## 1. Poisoned-Context Evidence Status

### Current Status: Partial

The Phase F KPI states: "Poisoned-context test suite pass rate: >= 80% target
on curated fixtures" (docs/91-phase-success-criteria-and-kpis.md:386).

**What is VERIFIED now:**
- `tests/integration_poisoned_context.rs` has 5/5 tests passing
- Tests cover:
  - Compile-time trust labeling catches prompt injection
  - Taint propagates into evaluate decisions
  - High-taint + non-R0 mutation quarantines
  - Read-only intent fails closed against mutation proposal
  - MCP scope constraints fail closed with poisoned context
- Phase C firewall logic exists: trust labels, taint scoring, contradiction
  checks, output sanitization, DLP, execution-time enforcement for all 5
  resource binding types (File, Http, Sqlite, Git, EmailDraft)
- Evidence: `docs/91-phase-success-criteria-and-kpis.md` line 28: "5/5 pass
  (curated poisoned-context regression suite)"

**What is KNOWN LIMITATION (P1 backlog):**
- The 5 tests in `integration_poisoned_context.rs` constitute a curated
  poisoned-context regression suite and all 5 pass (per
  `docs/91-phase-success-criteria-and-kpis.md` line 28: "5/5 pass (curated
  poisoned-context regression suite)").
- The KPI target is ">= 80% on curated fixtures"; 5/5 = 100% exceeds this.
- Remaining P1 backlog: expanding fixture library breadth to cover more
  real-world attack patterns beyond the current 5 test scenarios.

**Honest Assessment:**
The poisoned-context regression suite exists with 5/5 tests passing. The
gap is fixture breadth expansion (P1 backlog), not missing infrastructure.

---

## 2. Supported Flows (Verified)

All flows below have automated integration test evidence in the repo.

### 2.1 Gateway Governance Flow

Source: `tests/integration_gateway_flow.rs`, `tests/integration_lineage_chain.rs`

| Step | Evidence |
|------|----------|
| compile intent | `test_compile_intent_persists_and_emits_provenance` (line 147) |
| evaluate proposal | `test_evaluate_proposal_loads_real_intent_and_persists` (line 203) |
| mint capability | `test_full_happy_path_flow_compile_evaluate_mint_authorize_prepare` (line 748) |
| authorize execution | same test as above |
| prepare execution | same test as above |
| execute | `test_full_happy_path_execute_verify_auto_commit` (line 1161) |
| verify + auto-commit (R0) | same test as above |
| verify + explicit commit (R2) | `test_r2_no_auto_commit_verify_then_explicit_commit` (line 1260) |

### 2.2 Deny Path Flows

Source: `tests/integration_gateway_flow.rs`

| Scenario | Evidence |
|----------|----------|
| scope mismatch deny | `test_evaluate_proposal_loads_real_intent_and_persists` + scope enforcement at mint |
| proposal_id mismatch | `test_evaluate_proposal_id_mismatch_returns_400_and_no_events` (line 628) |
| missing intent (fail-closed) | `test_evaluate_proposal_rejects_missing_intent_fail_closed` (line 310) |
| rollback class floor enforced | `test_rollback_class_floor_prevents_downgrade_below_intent_default` (line 392) |

### 2.3 Quarantine Path

Source: `tests/integration_gateway_flow.rs`, `tests/integration_poisoned_context.rs`

| Scenario | Evidence |
|----------|----------|
| high taint + non-R0 mutation quarantines | `test_quarantine_path_blocks_execution_advance` (line 1407) |
| quarantine blocks prepare | `test_prepare_execution_blocks_quarantined_state` (line 1833) |
| high-taint + non-R0 quarantines (dedicated) | `test_high_taint_non_r0_mutation_quarantines` (line 296, poisoned_context.rs) |
| quarantine provenance event | verified in above tests |

### 2.4 Rollback and Recovery Paths

Source: `tests/integration_gateway_flow.rs`

| Scenario | Evidence |
|----------|----------|
| R0 auto-commit | `test_full_happy_path_execute_verify_auto_commit` (line 1161) |
| R2 explicit commit | `test_r2_no_auto_commit_verify_then_explicit_commit` (line 1260) |
| R3 RequireApproval | `test_evaluate_proposal_with_r3_intent_requires_approval` (line 537) |
| rollback terminal event | `test_rollback_lineage_chain_has_terminal_event` (line 507, lineage_chain.rs) |

### 2.5 Adapter-Backed Recovery

Source: `docs/15-deployment-and-operations.md`, `docs/91-phase-success-criteria-and-kpis.md` line 16

Registered adapters: fs, git, sqlite, maildraft, http, noop

Evidence for: file create/delete, file overwrite/restore, sqlite row restore,
maildraft draft create/delete recovery, git ref restore, HTTP full-parity
(GET/POST/PUT/PATCH/DELETE + body/header/query binding + auth)

### 2.6 Provenance / Lineage Chain

Source: `tests/integration_lineage_chain.rs`

| Scenario | Evidence |
|----------|----------|
| minimum lineage chain events | `test_minimum_lineage_chain_events_exist` (line 102) |
| chain contiguity | `test_lineage_chain_is_contiguous_no_missing_events` (line 401) |
| rollback terminal event | `test_rollback_lineage_chain_has_terminal_event` (line 507) |
| lineage endpoint | `test_get_execution_lineage_endpoint` (line 725) |
| lineage fail-soft for unknown exec | `test_get_lineage_unknown_execution_returns_empty_events` (line 960) |

Minimum chain per `docs/04-runtime-flow.md`: ActionProposalSubmitted ->
PolicyEvaluated -> CapabilityMinted -> ToolCallPrepared -> ToolCallExecuted ->
SideEffectPrepared -> SideEffectVerified -> terminal event

---

## 3. Open Gaps

### P1 (post-Phase-F backlog)

1. **Curated poisoned-context regression fixture breadth expansion**
   - Status: 5/5 pass already exists (100% exceeds >= 80% KPI target)
   - KPI target ">= 80% on curated fixtures" MET per `91-phase-success-criteria-and-kpis.md` line 28
   - P1 backlog: expand fixture library to cover more real-world attack patterns
   - Source: `docs/implementation-path/11-remaining-tasks.md` (updated)

 2. **Generic provenance query/replay fabric**
   - Status: Core query surface DONE; `POST /v1/provenance/query` expanded with filters on `intent_id`, `proposal_id`, `execution_id`, `capability_id`, event kind, terminal state, time range, cursor pagination; `ferrum-graph` read-model helpers implemented (`terminal_events`, `walk_backwards_from`, `walk_forwards_from`); integration tests at `tests/integration_provenance_query.rs`
   - Future P2: advanced replay/fabric tooling, cross-node ledger sync
   - Source: `crates/ferrum-proto/src/provenance.rs:86`, `crates/ferrum-store/src/sqlite/provenance.rs:142`, `crates/ferrum-gateway/src/server.rs:2192`, `crates/ferrum-graph/src/lib.rs:52`

 3. **EmailSend recovery parity** (Slice 16-A ratified boundary)
    - Status: EmailDraft allow_send=true explicitly denied at gateway prepare-time
    - EmailSend mutation recovery NOT in scope for v1; deny boundary ratified
    - Source: `crates/ferrum-gateway/src/server.rs:1149`, `crates/ferrum-adapter-maildraft/src/lib.rs:161`
    - Entry criteria for future governed-path evaluation: see `docs/implementation-path/16a-slice-16-a-boundary-ratification.md`

 4. **HTTP rollback/compensate is no-op** (Slice 16-A ratified boundary)
    - Status: HTTP adapter rollback is conservative no-op by design
    - Remote mutation rollback requires manual operator R3 compensation
    - Source: `crates/ferrum-adapter-http/src/lib.rs:1079`, `crates/ferrum-gateway/src/server.rs:2660`
    - Entry criteria for future recovery implementation: see `docs/implementation-path/16a-slice-16-a-boundary-ratification.md`

5. **TLS termination story**
   - Status: No in-process TLS listener; external termination required
   - Source: `docs/15-deployment-and-operations.md` line 15, 55

6. **Ledger hash chain**
   - Status: Initial integration slice DONE (store atomic append, gateway commit wiring, startup verify, Slice 3 tests passing at `tests/integration_gateway_flow.rs:9251,9377`)
   - Live append-time verification DONE per `docs/implementation-path/17-ledger-live-hash-verification-execution-plan.md` (Commits A-C complete); evidence: `ferrum-ledger/src/lib.rs:229`, `ferrum-store/src/sqlite/ledger.rs:22`, `ferrum-store/src/sqlite/ledger.rs:77`, `ferrum-gateway/src/server.rs:1602`, `ferrum-store/src/sqlite/tests.rs:1423`
   - Future: ledger read-model, cross-node sync remain open
   - Source: `docs/implementation-path/12-ledger-hash-chain-execution-plan.md` Commits 1-4; `docs/implementation-path/11-remaining-tasks.md` line 26

 7. **ferrumctl expanded utility**
     - Status: watch-execution and execution-control (compensate, rollback) commands merged per PR #40; both slices marked complete at `docs/implementation-path/15-ferrumctl-more-useful-execution-plan.md` lines 281, 472
     - Source: `docs/implementation-path/11-remaining-tasks.md` line 47

 8. **Runtime integration boundary (proof slice DONE)**
    - Status: Observation-only MCP bridge (`McpBridge`) with explicit anchor ingest; e2e lineage test proves internal + external events share same execution chain
    - Evidence: `crates/ferrum-integrations-mcp/src/bridge.rs` (Commit 3 public API); `tests/integration_mcp_bridge.rs:253` (`test_mcp_bridge_ingest_creates_linked_external_event`); `tests/integration_mcp_bridge.rs:399` (`test_mcp_bridge_ingest_multiple_event_types`)
    - Future P3: full MCP transport loop, auto anchor resolution, persistent dedupe, background replay worker, multiple simultaneous vendor bridges
    - Source: `docs/implementation-path/14-runtime-integration-boundary-execution-plan.md`

---

## 4. Handoff Readiness

### Release Checklist Status

Source: `docs/16-release-checklist.md`

All items marked [x] as of current branch:
- Contract integrity: updated
- Workspace quality: cargo check/fmt/clippy/test pass
- Behavior quality: scope mismatch, single-use, R3 no auto-commit, rollback,
  poisoned context tests all pass
- Operator readiness: config docs, CLI, lineage, approval flow, runbooks

### Docs Coverage

| Area | Status | Source |
|------|--------|--------|
| Core architecture | 100% | docs/03-architecture.md |
| Supported adapters | 100% | docs/15-deployment-and-operations.md |
| Implementation-path docs | Complete | docs/implementation-path/ |
| Release checklist | Present | docs/16-release-checklist.md |

### Source-of-Truth Priority

Per `docs/README.md` lines 42-49:

1. `docs/00-project-canon.md`
2. `docs/06-constraints-and-invariants.md`
3. `docs/09-implementation-path.md`
4. `docs/10-crate-by-crate-plan.md`
5. Rest of `docs/`

### Bootstrap Reading Order

For new agents or engineers joining after handoff:

1. `docs/00-project-canon.md`
2. `docs/01-quickstart.md`
3. `docs/02-project-overview.md`
4. `docs/03-architecture.md`
5. `docs/04-runtime-flow.md`
6. `docs/05-domain-model.md`
7. `docs/06-constraints-and-invariants.md`
8. `docs/07-policy-and-security-model.md`
9. `docs/08-repository-structure.md`
10. `docs/09-implementation-path.md`
11. `docs/16-release-checklist.md`
12. `docs/15-deployment-and-operations.md`
13. `docs/17-troubleshooting.md`
14. `docs/implementation-path/11-remaining-tasks.md`

### Key Test Files for Reference

| File | What It Covers |
|------|----------------|
| `tests/integration_gateway_flow.rs` | Gateway happy/deny/quarantine/rollback/approval flows |
| `tests/integration_poisoned_context.rs` | Trust, taint, quarantine, fail-closed behaviors |
| `tests/integration_lineage_chain.rs` | Provenance chain, lineage endpoint, terminal events |

---

## 5. Phase F Release Gate

Per `docs/91-phase-success-criteria-and-kpis.md` section 7.4:

FerrumGate is considered "ready to upgrade and integrate" when:
- docs, tests, governance flow, and recovery flow are consistent
- Others can enter the repo and continue without redesigning everything

Current assessment: Phase F evidence and docs are substantially complete for
the supported flow set. The primary P1 backlog item is expanding curated
poisoned-context fixture breadth. All other items are tracked as P1/P2 backlog.

---

## 6. Evidence Sources Summary

| Source | Location | Key Claims |
|--------|----------|------------|
| Phase success criteria | `docs/91-phase-success-criteria-and-kpis.md` | Phase F status, KPIs, release gates |
| Release checklist | `docs/16-release-checklist.md` | All items [x] for current branch |
| Remaining tasks | `docs/implementation-path/11-remaining-tasks.md` | P0/P1/P2 gap tracking |
| Phase F evidence | `docs/implementation-path/11-phase-f-evidence.md` | Internal handoff (Vietnamese) |
| Testing strategy | `docs/11-testing-strategy.md` | Required v1 tests listed |
| Gateway flow tests | `tests/integration_gateway_flow.rs` | 74/74 tests pass |
| Poisoned-context tests | `tests/integration_poisoned_context.rs` | 5/5 tests pass; fixture breadth expansion P1 backlog |
| Lineage chain tests | `tests/integration_lineage_chain.rs` | 5/5 tests pass |

---

*End of Phase F Evidence Pack*
