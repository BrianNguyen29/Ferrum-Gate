# 11 — Remaining tasks

Prioritized checklist of incomplete work, grounded in existing docs.
Do not invent scope; all items cite source docs.
Scope is single-node v1 unless labeled post-v1.

## P0 — Must fix before v1 RC

- [x] cargo clippy `--workspace -- -D warnings` PASS
  - Status: PASS as of 2026-04-02 gate run
  - Historical PASS: `docs/artifacts/2026-03-30/03-cargo-clippy.txt`

- [x] cargo test `--workspace` PASS
  - Status: PASS as of 2026-04-02 gate run
  - Historical PASS: `docs/artifacts/2026-03-30/04-cargo-test.txt`

- [x] scope mismatch deny behavior implemented
  - Src: `docs/16-release-checklist.md` line 20 "scope mismatch deny test -- VERIFIED: empty scope + non-R0 mutation = Deny (`scope.mismatch.empty.scope`), empty scope + R0 = Allow"
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

- [x] clippy cleanup: `cargo clippy --workspace -- -D warnings` PASS (as of 2026-04-02) — P0 resolved; was PASS on 2026-03-30
  - Src: `cargo clippy --workspace -- -D warnings` verified PASS (historical evidence: `docs/artifacts/2026-03-30/03-cargo-clippy.txt`)
  - Note: Clippy gate cleared as of 2026-04-02.

- [x] RC evidence automation script
  - Src: `scripts/generate_rc_evidence.py` exists and runs the RC gate bundle
  - Note: RC evidence generation is automated; 2026-04-02 verdict is ALL GATES PASSED.

## P3 — post-v1 backlog (not in v1 scope)

These are explicitly out of v1 scope. Do not treat as blockers.

**P3.G live evidence — all complete (single-node scope):**
- P3.G1 ✅ DONE — functional readiness proof (end-to-end walkthrough): `docs/implementation-path/34-p3-g1-executed-evidence.md`
- P3.G2 ✅ DONE — smoke stability evidence (automated 12-interval soak): `docs/implementation-path/35-p3-g2-executed-evidence.md` (run_id: `p3-g2-20260403-live`)
- P3.G3 ✅ DONE — backup/restore drill: `docs/implementation-path/31-p3-g3-backup-restore-drill-evidence.md`
- P3.G4 ✅ DONE — observability verification: `docs/implementation-path/32-p3-g4-observability-verification-evidence.md`
- Source of truth for P3 track status: `docs/implementation-path/30-production-roadmap.md` Section — Priority 3

**Remaining post-v1 adapter and integration work:**
- [ ] broader production-verified adapter integrations and hardening (fs, sqlite, git, http)
  - Src: `docs/00-project-canon.md` line 62 "broader production-verified adapter integrations and hardening (fs, sqlite, maildraft, git, http)"
  - Src: `docs/implementation-path/01-current-state.md` lines 26-31
  - Note: fs/sqlite/git/maildraft now have bounded local implementations; broader hardening, remote/external integration depth, and production verification remain post-v1.

- [ ] git: remote workflows and broader ref-mutation coverage (post-v1)
  - Src: `crates/ferrum-adapter-git/README.md` line 22 "All operations are local-only; no remote operations (push/fetch/pull)."
  - Note (2026-04-04): GitPush slice 1 implemented (local-to-remote push against temporary remotes); fetch/pull remain out of scope.

- [ ] maildraft: send/provider integration (post-v1)
  - Src: `crates/ferrum-adapter-maildraft/src/lib.rs` line 6 "send semantics out of scope"
  - Note: maildraft durable persistence and verify semantics implemented in v1; actual mail send to external provider remains post-v1.

- [ ] multi-node / HA / read-replica support
  - Src: `docs/00-project-canon.md` line 56 "full distributed deployment"

- [x] Outcome-aware Governance (U1) — CORE CAPABILITY DONE
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.1
  - Note: evaluate-time slice DONE (allowed-outcome mismatch warns; explicit forbidden-outcome match denies). U1-S2 (verify-time annotate-only assessment) DONE: assessment persisted into execution.metadata, rollback contract metadata, and SideEffectVerified provenance event metadata, with unavailable-context fallback covered. U1-S3a (multi-signal inference with confidence/strength) DONE: rollback_target (HIGH), adapter_key (MED), expected_effect (LOW) inference hierarchy implemented; alignment_strength distinguishes strong/moderate/weak/mismatch/none. U1-S3b (confidence-thresholded verify annotations) DONE: threshold_metadata nested block added with threshold_band, threshold_rule_id, suggested_future_action, annotate_only, and ambiguity_reason fields; schema presence validated; LOW-band ambiguity path and unavailable-context fallback path tested. U1-S4a (higher-fidelity outcome contracts) DONE: additive `OutcomeSelectors` enrich `OutcomeClause`, and verify-time clause annotations now distinguish coarse effect-type matches from selector-enhanced matches/mismatches. U1-S4b (explicit HIGH/MED mismatch fixtures) DONE: HIGH band mismatch via allowed_outcome non-match with HIGH confidence; MED band mismatch via Generic rollback_target + http adapter_key path; selector-enhanced mismatch proven. U1-S5a (soft gate preview) DONE: preview signals `would_block`, `would_require_review`, `reason_codes`, and `derive_basis` emitted at prepare-time and stored in rollback contract metadata. U1-S5b (hard gate) DONE: prepare-time blocks when would_block=true; state-machine halts to Denied; auditability via ErrorRaised event and u1_s5b_hard_gate metadata. U1-S6 (selector-aware effective match) DONE: selector-bearing clauses require effect_type AND selectors to match for effective match. U1-S7a (list-based selector matching) DONE: `adapter_family_in`, `target_family_in`, `request_class_in`, `mutation_family_in` fields with OR semantics. U1-S8a (operator compile-time ergonomics) DONE: compile endpoint accepts optional `allowed_outcomes`/`forbidden_outcomes` via existing OutcomeClause/OutcomeSelectors schema with fail-closed validation. U1-S9a (deterministic policy bundle fingerprinting) DONE: `PolicyBundleId::derive()` uses UUID v5 (name-based with SHA-1) to create deterministic identity from canonical outcome contract serialization; `IntentEnvelope.policy_bundle_fingerprint` stores the derived fingerprint; capability mint propagates the fingerprint into `CapabilityLease.policy_bundle_id` via `CapabilityMintRequest.policy_bundle_id`; provenance events carry the derived bundle identity through `policy_bundle_id` field.
  - Remaining U1 backlog (not core gaps): richer outcome clause expressiveness (nested selectors, temporal constraints); policy bundle migration tooling (CLI authoring workflows remain post-v1).

- [ ] Reversible Execution Planner (U2)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.2

- [ ] Cross-runtime Provenance Fabric (U3)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.3

- [ ] Runtime Integrations — MCP / local / NemoClaw (U4)
  - Src: `docs/91-phase-success-criteria-and-kpis.md` section 8.4

## Execution Sequence

Grounded in `docs/implementation-path/30-production-roadmap.md` post-P3 execution order. Single-node v1 RC-ready; broader production-ready still incomplete.

### Immediate Next Slices (P2 adapter hardening)

| Order | Item | Status | Source |
|-------|------|--------|--------|
| 1 | P2.5 http adapter hardening (Slice 1–10) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.5; `35-p3-g2-executed-evidence.md` |
| 2 | P2.1 fs adapter hardening (Slice 1: fail-closed verify on I/O errors ✅) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.1 |
| 3 | P2.2 sqlite adapter hardening (Slice 1: identifier safety + rollback noop tests ✅) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.2 |
| 4 | P2.3 git adapter hardening (Slice 1: fail-closed verify + noop edge-case tests ✅) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.3 |
| 5 | P2.4 git remote workflows (Slice 1: GitPush against local temporary remotes ✅ 2026-04-04; fetch/pull not yet implemented) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.4 |
| 6 | P2.6 EmailSend governed-path rollout (preflight slice ✅ 2026-04-04; scaffold-only adapter slice ✅ 2026-04-04; provider send integration TBD post-v1) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.6; `36-p2-6-emailsend-governed-path-entry-analysis.md`; `37-p2-6-emailsend-adapter-contract-draft.md`; `38-p2-6-emailsend-adapter-scaffold-implementation.md` |
| 7 | P2.7 maildraft broader verify semantics hardening (Slice 1: explicit EmailDraftExists verify_checks handling ✅ 2026-04-04) | 🔄 IN PROGRESS | `30-production-roadmap.md` P2.7 |

### Longer-Term / Planned Tracks

| Order | Item | Status | Source |
|-------|------|--------|--------|
| 8 | P4.1 `ferrumctl` advanced operator flows | ⬜ TODO | `30-production-roadmap.md` P4.1 |
| 9 | P4.2 Policy bundle lifecycle tooling | ⬜ TODO | `30-production-roadmap.md` P4.2 |
| 10 | P5.4–P5.5 Sync-1 preflight checks + decision table | ⬜ TODO | `30-production-roadmap.md` P5.4–P5.5 |
| 11 | P5.7 HA / multi-leader replication | ⬜ PLANNED | `30-production-roadmap.md` P5.7 |
| 12 | U1.1–U1.2 Outcome-aware Governance (remaining backlog) | ⬜ PLANNED | `30-production-roadmap.md` U1.1–U1.2; Outcome-aware Governance backlog note above |
| 13 | U2 Reversible Execution Planner | ⬜ PLANNED | `30-production-roadmap.md` U2 |
| 14 | U3 Cross-runtime Provenance Fabric | ⬜ PLANNED | `30-production-roadmap.md` U3 |
| 15 | U4 Runtime Integrations (MCP / local / NemoClaw) | ⬜ PLANNED | `30-production-roadmap.md` U4 |

**Note:** Execution order follows roadmap priority sequence per `docs/implementation-path/24-p1-p2-p3-execution-plan.md` lines 266–297.

---

## Documented drift / cleanup notes (as of 2026-04-03)

- scope mismatch deny: IMPLEMENTED in `crates/ferrum-pdp/src/engine.rs` lines 31-46
- all Phase A/B/E items treated as complete per `docs/91-phase-success-criteria-and-kpis.md`
- Phase C firewall logic confirmed present; curated regression fixtures DONE (6 tests)
- `scripts/generate_rc_evidence.py` EXISTS and PASS — verdict is ALL GATES PASSED (2026-04-02)
- clippy: PASS (2026-04-02) — was PASS on 2026-03-30 per artifacts
- cargo test --workspace: PASS (2026-04-02) — was PASS on 2026-03-30 per artifacts
- issue #97 (2026-04-03): HTTP adapter verify semantics clarified — mutations use execute-time metadata only, fail-closed on non-2xx without explicit check; gateway-level failure-mode integration test added (`test_http_post_500_verify_false_commit_rejected_from_failed_state`); broader adapter hardening remains post-v1
