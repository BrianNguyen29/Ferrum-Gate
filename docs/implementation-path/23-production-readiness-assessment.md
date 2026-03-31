# 23 — Production readiness assessment

Single-node v1 scope. Assessment against FerrumGate v1 success criteria.

**Release support contract**:
- Supported = single-node governance core with SQLite-backed persistence.
- Partial = adapter crate surfaces (fs/sqlite/maildraft/git/http) and a bounded `ferrumctl` operator surface.
- Deferred/post-v1 = real adapter implementations, multi-node/HA/read-replica, U1-U4 upgrade tracks.

## Overall readiness: RC-READY

FerrumGate v1 is RC-ready for single-node SQLite-backed deployment.
All P0/P1/P2 items verified complete. Core governance loop implemented and test-covered.
Remaining gaps are post-v1 backlog documented in `11-remaining-tasks.md` P3.

---

## Dimension 1 — Technical correctness

| Criterion | Status | Evidence |
|---|---|---|
| Workspace compiles | PASS | `docs/artifacts/2026-03-30/01-cargo-check.txt` |
| `cargo fmt --all --check` | PASS | `docs/artifacts/2026-03-30/02-cargo-fmt.txt` |
| `cargo clippy --workspace -- -D warnings` | PASS | `docs/artifacts/2026-03-30/03-cargo-clippy.txt` |
| Core tests pass | PASS | `docs/artifacts/2026-03-30/04-cargo-test.txt` |
| Integration tests pass | PASS | `ferrum-integration-tests` suite |
| No R3 auto-commit violations | PASS | `test_r3_contracts_have_auto_commit_false` |
| Single-use capability enforced | PASS | `test_single_use_capability_cannot_be_reused` |
| Provenance emitted for supported flows | PASS | `test_lineage_endpoint_*` series |
| Rollback/compensate distinct ops | PASS | `test_rollback_and_compensate_are_distinct_operations` |

**Known gaps**: none in core correctness. See support contract (19-v1-single-node-support-contract.md) Accepted Risks section and invariant matrix (26-v1-single-node-invariant-control-test-evidence-matrix.md) for residual accepted risks and partial controls.

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

**Known gaps**: none in core correctness. See support contract Accepted Risks and invariant matrix for residual accepted risks.

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
- `ferrumctl` does not expose the full REST surface; some flows still require direct HTTP/OpenAPI usage.

---

## Dimension 4 — Documentation completeness

| Criterion | Status | Evidence |
|---|---|---|
| Project canon | PASS | `docs/00-project-canon.md` |
| Business overview | PASS | `docs/01-business-overview.md` |
| Runtime flow | PASS | `docs/02-runtime-flow.md` |
| Constraints/invariants | PASS | `docs/06-constraints-and-invariants.md` |
| Agent handoff | PASS | `docs/12-agent-handoff.md` |
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

### P0 — v1 RC blocker
(none) — scope-mismatch deny implemented in `crates/ferrum-pdp/src/engine.rs` lines 31-46.

### P1 — v1 RC evidence
(none) — all P1 items resolved:
1. Curated poisoned-context regression fixtures (6 tests).
2. Phase F docs pack finalized as cohesive, non-contradictory set.
3. Supported flows list documented in `25-v1-single-node-rc-evidence.md` Evidence 9.
4. Open gaps list documented in `11-remaining-tasks.md`.

### P2 — v1 polish
(all resolved):
1. `scripts/generate_rc_evidence.py` exists and PASS with all five checks — evidence: `docs/artifacts/2026-03-30/07-rc-evidence-script.txt`.
2. clippy passes: `cargo clippy --workspace -- -D warnings` PASS — evidence: `docs/artifacts/2026-03-30/03-cargo-clippy.txt`.
3. `cargo test --workspace` PASS — evidence: `docs/artifacts/2026-03-30/04-cargo-test.txt`.

---

## Verdict

**FerrumGate v1 is RC-ready** for single-node SQLite-backed deployment.

All P0/P1/P2 items verified complete:
- Scope-mismatch deny implemented
- Poisoned-context fixtures curated (6 tests)
- Phase F docs pack finalized
- clippy passes with no warnings
- 128 tests pass across workspace
- RC evidence script present and passing

The governance loop, persistence layer, and integration test coverage are strong.
Remaining gaps are post-v1 backlog items (multi-node/HA, real adapters, U1-U4 upgrade tracks).
