# 11 — Remaining tasks

Prioritized checklist of incomplete work, grounded in existing docs.
Do not invent scope; all items cite source docs.
Scope is single-node v1 unless labeled post-v1.

## P0 — Must fix before v1 RC

- [x] scope mismatch deny behavior implemented
  - Src: `docs/16-release-checklist.md` line 16 "scope mismatch deny test (explicit scope-mismatch deny behavior not implemented yet; see test_scope_mismatch_behavior_not_yet_implemented)"
  - Status: DONE — `StaticPdpEngine::evaluate()` now checks `intent.resource_scope.is_empty()` AND non-R0 mutation and returns `Decision::Deny` with rule `"scope.mismatch.empty.scope"`. Tests: `test_scope_mismatch_deny_on_empty_scope_with_mutation`, `test_r0_allowed_with_empty_scope`.

## P1 — v1 RC evidence gaps

- [x] poisoned context regression fixtures (curated test set, >= 80% catch rate target)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` F.3 "Poisoned-context test suite pass rate: >= 80% target on curated fixtures"
  - Src: `docs/91-phase-success-criteria-and-kpis.md` 7.5 evidence "poisoned-context tests"
  - Status: DONE — 6 curated fixture tests added: `test_poisoned_context_taint_at_boundary_69_no_quarantine`, `test_poisoned_context_r0_bypasses_taint_check`, `test_poisoned_context_taint_at_maximum_100`, `test_poisoned_context_r3_requires_approval`, `test_poisoned_context_moderate_taint_50_no_quarantine`, `test_poisoned_context_trust_attributes_no_bypass`.

- [x] final docs pack for Phase F (complete, consistent, internally non-contradictory)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` 7.5 evidence "final docs pack"
  - Status: DONE — implementation-path docs finalized as cohesive pack. See `01-current-state.md`, `11-remaining-tasks.md`, `23-production-readiness-assessment.md`, `25-v1-single-node-rc-evidence.md`.

- [x] supported flows list (Phase F evidence)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` 7.5 evidence "supported flows list"
  - Status: DONE — documented in `25-v1-single-node-rc-evidence.md` Evidence 9. The gateway orchestrates: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (compensate is the primary recovery endpoint; commit and rollback routes are also exposed). Denial paths: deny, quarantine, compensate, await-approval, scope-mismatch (now explicit), draft-only gated at evaluate (before prepare).

- [x] open gaps list (Phase F evidence)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` 7.5 evidence "open gaps list"
  - Status: DONE — this file serves as the gaps list. Remaining gaps are P3 post-v1 backlog items.

## P2 — v1 polish (not blockers for RC but needed before v1 stable)

- [x] clippy cleanup: `cargo clippy --workspace -- -D warnings` PASS — evidence: `docs/artifacts/2026-03-30/03-cargo-clippy.txt`
  - Src: `cargo clippy --workspace -- -D warnings` verified PASS
  - Note: Not a v1 RC blocker; verified clean as of 2026-03-29.

- [x] RC evidence automation script
  - Src: `scripts/generate_rc_evidence.py` exists and PASS with all five checks
  - Note: RC evidence generation now automated.

## P3 — post-v1 backlog (not in v1 scope)

These are explicitly out of v1 scope. Do not treat as blockers.

- [ ] ledger hash chain
  - Src: `docs/implementation-path/08-next-issue-backlog.md` P2

- [ ] git adapter (real implementation)
  - Src: `docs/implementation-path/08-next-issue-backlog.md` P2
  - Src: `docs/implementation-path/04-crate-by-crate-tasks.md` "adapters: ... -> git/http"
  - Note: Skeleton exists at `crates/ferrum-adapter-git/`; real implementation is post-v1.

- [ ] http adapter (real implementation)
  - Src: `docs/implementation-path/08-next-issue-backlog.md` P2
  - Src: `docs/implementation-path/04-crate-by-crate-tasks.md` "adapters: ... -> git/http"

- [ ] ferrumctl expanded commands (beyond health/inspect)
  - Src: `docs/implementation-path/08-next-issue-backlog.md` P2
  - Src: `docs/implementation-path/15-ferrumctl-more-useful-execution-plan.md`

- [ ] multi-node / HA / read-replica support
  - Src: `docs/00-project-canon.md` line 56 "full distributed deployment"

- [ ] Outcome-aware Governance (U1)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.1

- [ ] Reversible Execution Planner (U2)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.2

- [ ] Cross-runtime Provenance Fabric (U3)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.3

- [ ] Runtime Integrations — MCP / local / NemoClaw (U4)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.4

## Documented drift / cleanup notes (all resolved as of 2026-03-29)

- scope mismatch deny: IMPLEMENTED in `crates/ferrum-pdp/src/engine.rs` lines 31-46
- all Phase A/B/E items treated as complete per `docs/91-phase-success-criteria-and-kpis.md`
- Phase C firewall logic confirmed present; curated regression fixtures DONE (6 tests)
- `scripts/generate_rc_evidence.py` EXISTS and PASS
- clippy: PASS with no warnings
- test count: 128 tests across workspace
