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

- [ ] broader production-verified adapter integrations and hardening (fs, sqlite, git, http)
  - Src: `docs/00-project-canon.md` line 62 "broader production-verified adapter integrations and hardening (fs, sqlite, maildraft, git, http)"
  - Src: `docs/implementation-path/01-current-state.md` lines 26-31
  - Note: fs/sqlite/git/maildraft now have bounded local implementations; broader hardening, remote/external integration depth, and production verification remain post-v1.

- [ ] git: remote workflows and broader ref-mutation coverage (post-v1)
  - Src: `crates/ferrum-adapter-git/README.md` line 22 "All operations are local-only; no remote operations (push/fetch/pull)."
  - Note: local HEAD restore and branch-create rollback exist; remote Git workflows remain out of scope.

- [ ] maildraft: send/provider integration (post-v1)
  - Src: `crates/ferrum-adapter-maildraft/src/lib.rs` line 6 "send semantics out of scope"
  - Note: maildraft durable persistence and verify semantics implemented in v1; actual mail send to external provider remains post-v1.

- [ ] multi-node / HA / read-replica support
  - Src: `docs/00-project-canon.md` line 56 "full distributed deployment"

- [x] Outcome-aware Governance (U1) — CORE CAPABILITY DONE
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.1
  - Note: evaluate-time slice DONE (allowed-outcome mismatch warns; explicit forbidden-outcome match denies). U1-S2 (verify-time annotate-only assessment) DONE: assessment persisted into execution.metadata, rollback contract metadata, and SideEffectVerified provenance event metadata, with unavailable-context fallback covered. U1-S3a (multi-signal inference with confidence/strength) DONE: rollback_target (HIGH), adapter_key (MED), expected_effect (LOW) inference hierarchy implemented; alignment_strength distinguishes strong/moderate/weak/mismatch/none. U1-S3b (confidence-thresholded verify annotations) DONE: threshold_metadata nested block added with threshold_band, threshold_rule_id, suggested_future_action, annotate_only, and ambiguity_reason fields; schema presence validated; LOW-band ambiguity path and unavailable-context fallback path tested. U1-S4a (higher-fidelity outcome contracts) DONE: additive `OutcomeSelectors` enrich `OutcomeClause`, and verify-time clause annotations now distinguish coarse effect-type matches from selector-enhanced matches/mismatches. U1-S4b (explicit HIGH/MED mismatch fixtures) DONE: HIGH band mismatch via allowed_outcome non-match with HIGH confidence; MED band mismatch via Generic rollback_target + http adapter_key path; selector-enhanced mismatch proven. U1-S5a (soft gate preview) DONE: preview signals `would_block`, `would_require_review`, `reason_codes`, and `derive_basis` emitted at prepare-time and stored in rollback contract metadata. U1-S5b (hard gate) DONE: prepare-time blocks when would_block=true; state-machine halts to Denied; auditability via ErrorRaised event and u1_s5b_hard_gate metadata. U1-S6 (selector-aware effective match) DONE: selector-bearing clauses require effect_type AND selectors to match for effective match. U1-S7a (list-based selector matching) DONE: `adapter_family_in`, `target_family_in`, `request_class_in`, `mutation_family_in` fields with OR semantics.
  - Remaining U1 backlog (not core gaps): richer outcome clause expressiveness (nested selectors, temporal constraints); operator ergonomics for outcome contract authoring; policy bundle versioning and migration tooling.

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
