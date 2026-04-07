# 23 — Production readiness assessment

Single-node v1 scope. Assessment against FerrumGate v1 success criteria.

**Release support contract**:
- Supported = single-node governance core with SQLite-backed persistence.
- Partial = adapter crate surfaces (fs/sqlite/git/http) plus maildraft (SQLite-backed persistence and verify semantics implemented; send/provider integration deferred/post-v1), and a bounded `ferrumctl` operator surface.
- Deferred/post-v1 = broader adapter hardening, multi-node/HA/read-replica, remaining U1 expressiveness/operator tooling work, and U2-U4 upgrade tracks.

## Overall readiness: RC-READY — full evidence in `25-v1-single-node-rc-evidence.md`

See [25-v1-single-node-rc-evidence.md](./25-v1-single-node-rc-evidence.md) for the
canonical evidence record (Phase F gate 7.5 output), including all gate results,
test verdicts, and supported-flows attestation.

FerrumGate v1 single-node passed all gates as of 2026-04-02:
1. `cargo clippy --workspace -- -D warnings` PASS
2. `cargo test --workspace` PASS

Core governance loop is implemented. Scope-mismatch deny is implemented.
All P0 gates cleared as of 2026-04-02. RC sign-off can proceed.

---

## Dimension 1 — Technical correctness

| Criterion | Status | Evidence |
|---|---|---|
| Workspace compiles | PASS | `cargo check --workspace` (2026-04-02) |
| `cargo fmt --all --check` | PASS | `cargo fmt --all -- --check` (2026-04-02) |
| `cargo clippy --workspace -- -D warnings` | PASS | All gates PASS as of 2026-04-02 |
| Core tests pass | PASS | `cargo test --workspace` (2026-04-02) |
| Integration tests pass | PASS | `ferrum-integration-tests` suite |
| No R3 auto-commit violations | PASS | `test_r3_contracts_have_auto_commit_false` |
| Single-use capability enforced | PASS | `test_single_use_capability_cannot_be_reused` |
| Provenance emitted for supported flows | PASS | `test_lineage_endpoint_*` series |
| Rollback/compensate distinct ops | PASS | `test_rollback_and_compensate_are_distinct_operations` |

**Known gaps**: none. All P0 gates cleared as of 2026-04-02. Residual accepted risks and partial controls are documented in the support contract Accepted Risks section and invariant matrix.

---

## Dimension 2 — Governance enforcement

| Criterion | Status | Evidence |
|---|---|---|
| Intent required before mutation | PASS | gateway requires intent_id for all flows |
| Capability single-use | PASS | `cap.mark_used` -> AlreadyUsed on reuse |
| Capability scoped narrowly | PASS | scope-bounds mismatch explicitly enforced (P0 resolved) |
| Provenance chain maintained | PASS | lineage endpoint returns events for execution |
| Rollback contract per mutation | PASS | R0/R1/R2/R3 contract classes with auto_commit semantics |
| R3 requires approval | PASS | StaticPdpEngine returns RequireApproval for R3 |
| Draft-only enforcement | PASS | draft-only gated at evaluate (before prepare) |

**Known gaps**: none. All P0 gates cleared. Residual accepted risks and partial controls are documented in the support contract Accepted Risks section and invariant matrix.

---

## Dimension 3 — Operational completeness

| Criterion | Status | Evidence |
|---|---|---|
| SQLite persistence | PASS | `ferrum-store` with embedded migrations |
| CLI usable for inspection and targeted operator control | PARTIAL | See `docs/19-v1-single-node-support-contract.md` Section 1.3 for the current `ferrumctl` surface |
| Config docs | PASS | `docs/15-deployment-and-operations.md` |
| Approval workflow | PASS | GET /v1/approvals with pagination/filter |
| Provenance query | PASS | GET/POST /v1/provenance/lineage, POST /v1/provenance/query |

**Known gaps**:
- `ferrumctl` covers the high-use operator surface but not every REST endpoint; some advanced or intent-authoring flows still require direct HTTP/OpenAPI usage. The primary operator-facing flows (inspect, watch, resolve, cancel, pause, resume, prepare, execute, compensate, rollback) are all CLI-covered.

---

## Dimension 4 — Documentation completeness

| Criterion | Status | Evidence |
|---|---|---|
| Project canon | PASS | `docs/00-project-canon.md` |
| Business overview | PASS | `docs/02-project-overview.md` |
| Runtime flow | PASS | `docs/04-runtime-flow.md` |
| Constraints/invariants | PASS | `docs/06-constraints-and-invariants.md` |
| Agent handoff | PASS | `docs/implementation-path/07-agent-handoff-prompt.md` |
| Phase success criteria | PASS | `docs/91-phase-success-criteria-and-kpis.md` |
| Release checklist | PASS | `docs/16-release-checklist.md` |
| Remaining tasks | PASS | `docs/implementation-path/11-remaining-tasks.md` |
| Current state | PASS | `docs/implementation-path/01-current-state.md` |
| Phase checklists | PASS | `docs/implementation-path/09-phase-checklists.md` |
| RC evidence doc | PASS | `docs/implementation-path/25-v1-single-node-rc-evidence.md` exists (this doc) |
| Phase F final docs pack | PASS | implementation-path docs finalized as cohesive pack |

**Known gaps**: none. Residual accepted risks and partial controls are documented in the support contract and invariant matrix.

---

## Dimension 5 — Testing coverage

| Test | Status | File |
|---|---|---|
| Capability single-use | PASS | `integration_gateway_flow.rs` |
| R3 no auto-commit | PASS | `integration_gateway_flow.rs` |
| Rollback/compensate distinct | PASS | `integration_gateway_flow.rs` |
| Compensate end-to-end | PASS | `integration_gateway_flow.rs` |
| Taint-based quarantine | PASS | `integration_gateway_flow.rs` |
| Scope mismatch deny | PASS | `integration_gateway_flow.rs` (P0 resolved) |
| Pending approvals pagination | PASS | `integration_gateway_flow.rs` |
| Pending approvals filter | PASS | `integration_gateway_flow.rs` |
| Lineage endpoint shape | PASS | `integration_lineage_chain.rs` |
| Lineage query validation | PASS | `integration_lineage_chain.rs` |
| Poisoned context fixtures | PASS | 6 curated fixture tests (P1 resolved) |

---

## Open gaps summary

### RC blockers — none (all P0 gates cleared 2026-04-02)
- scope-mismatch deny implemented (`crates/ferrum-pdp/src/engine.rs:31-46`)
- issue #97 merged 2026-04-03: HTTP adapter verify semantics clarified; broader adapter hardening remains post-v1

### RC evidence — complete (all P1 items resolved 2026-04-02)
- Curated poisoned-context regression fixtures (6 tests)
- Phase F docs pack finalized
- Supported flows: `25-v1-single-node-rc-evidence.md` Evidence 9
- Open gaps: `11-remaining-tasks.md`

### Broader production-ready — in progress via roadmap gates
FerrumGate v1 single-node is **RC-ready** (2026-04-02). Broader production-ready
requires completing G-E1 through G-E5 per `30-production-roadmap.md` Priority 5
(Section "Production Evaluation and Execution Plan"). Key open items:
- **G-E1**: P2 adapter hardening (fs, sqlite, git, http, maildraft) — 🔄 IN PROGRESS
- **G-E2**: P2 performance baseline + benchmark suite — ⬜ TODO
- **G-E3**: `ferrumctl` advanced operator flows — ⬜ PLANNED
- **G-E4**: P5 Sync-1 preflight checks + decision table — ⬜ PLANNED
- **G-E5**: Production evaluation sign-off — ⬜ PLANNED

> **Out-of-tree SQLite candidate (NOT merged):** A write-queue optimization was
> evaluated in a local workspace (Phase 1 ✅, Phase 2 deferred after regression).
> See `40-out-of-tree-sqlite-performance-candidate.md`. This is **not repo truth**
> and is tracked as a potential future input to P2.2 Slice 3.

---

## Verdict

**FerrumGate v1 is RC-ready** as of 2026-04-02.

All RC gates pass:
1. `cargo clippy --workspace -- -D warnings` PASS
2. `cargo test --workspace` PASS

Core governance loop is implemented. Scope-mismatch deny is done. P1 evidence items are complete.
All P0 blockers resolved as of 2026-04-02.
P3.G1-G4 live evidence executed and attested (2026-04-03): see `30-production-roadmap.md` Priority 3 (lines 57–77).

**Broader production-ready** requires completing the evaluation gates G-E1 through G-E5
defined in `30-production-roadmap.md` (Priority 5, "Production Evaluation and Execution Plan").
Remaining gaps (multi-node/HA, broader adapter hardening, U2-U4 upgrade tracks) are post-v1 backlog.

Full evidence record: [25-v1-single-node-rc-evidence.md](./25-v1-single-node-rc-evidence.md).

Issue #97 (2026-04-03) improved HTTP adapter verify semantics and gateway integration
coverage but does not expand the supported scope beyond single-node RC-ready.
