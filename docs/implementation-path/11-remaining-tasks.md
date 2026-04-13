# 11 — Remaining tasks

Prioritized checklist of incomplete work, grounded in existing docs.
Do not invent scope; all items cite source docs.
Scope is single-node v1/v2 unless labeled post-v1 or post-v2.

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

> **Post-signoff (2026-04-09) — narrow fs-first evidence slice**: PR #165 added
> two integration tests confirming `before_hash`/`after_hash` wiring on the fs
> adapter rollback path (`tests/integration_gateway_flow.rs`:
> `test_new_file_before_hash_none_after_prepare_after_hash_some_after_execute`,
> `test_existing_file_before_hash_some_after_prepare_before_hash_ne_after_hash_after_execute`).
> This is a narrow follow-on evidence slice that does not alter the T1/T2/T3
> declaration from `43-production-readiness-signoff.md` and does not create a new
> blocker or roadmap track. See `docs/artifacts/2026-04-09/closure-note.txt`.

## P2 — v1 polish (not blockers for RC but needed before v1 stable)

- [x] clippy cleanup: `cargo clippy --workspace -- -D warnings` PASS (as of 2026-04-02) — P0 resolved; was PASS on 2026-03-30
  - Src: `cargo clippy --workspace -- -D warnings` verified PASS (historical evidence: `docs/artifacts/2026-03-30/03-cargo-clippy.txt`)
  - Note: Clippy gate cleared as of 2026-04-02.

- [x] RC evidence automation script
  - Src: `scripts/generate_rc_evidence.py` exists and runs the RC gate bundle
  - Note: RC evidence generation is automated; 2026-04-02 verdict is ALL GATES PASSED.

## Stage A — v2 Ratification

**Complete.** v2 RATIFIED 2026-04-12 per `46-v2-readiness-signoff.md`. No blockers.
v2 scope (per `44-v2-production-execution-plan.md` lines 27–41) explicitly excludes
all P3 items: multi-node/HA, U2/U3/U4, broader external adapter integrations
(remote git, external http, real mail send), and policy authoring tooling.

## P3 — post-v1 backlog (not in v1 scope)

These are explicitly out of v1 scope. Do not treat as blockers.
Structured backlog is in **`50-post-v2-roadmap.md`** with Horizons H1/H2/H3.

**P3.G live evidence — all complete (single-node scope):**
- P3.G1 ✅ DONE — functional readiness proof (end-to-end walkthrough): `docs/implementation-path/34-p3-g1-executed-evidence.md`
- P3.G2 ✅ DONE — smoke stability evidence (automated 12-interval soak): `docs/implementation-path/35-p3-g2-executed-evidence.md` (run_id: `p3-g2-20260403-live`)
- P3.G3 ✅ DONE — backup/restore drill: `docs/implementation-path/31-p3-g3-backup-restore-drill-evidence.md`
- P3.G4 ✅ DONE — observability verification: `docs/implementation-path/32-p3-g4-observability-verification-evidence.md`
- Source of truth for P3 track status: `docs/implementation-path/30-production-roadmap.md` Section — Priority 3

**Remaining post-v1 adapter and integration work:**

See **`50-post-v2-roadmap.md`** for structured backlog with Horizons H1/H2/H3.
Brief classification of items from this backlog:

- **H1 (near-term post-v2):** Policy bundle tooling (P4.2), U1 remaining expressiveness
  backlog, deeper git/fs/sqlite/http adapter hardening — build on v1/v2 single-node base
- **H2 (next capability):** HA/multi-leader replication (P5.7), U2 Reversible Execution Planner
- **H3 (long-term/deferred):** U3 Cross-runtime Provenance Fabric, U4 Runtime Integrations,
  mail provider send integration, full distributed multi-node deployment

Longer-term / planned items (from Execution Sequence below) are indexed in the roadmap
doc. Priority order from this file remains the source for sequencing guidance.

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

Grounded in `docs/implementation-path/30-production-roadmap.md` post-P3 execution order. Single-node v1 RC-ready first; broader production-ready is now declared through G-E5.

### Immediate Next Slices (P2 adapter hardening)

| Order | Item | Status | Source |
|-------|------|--------|--------|
| 1 | P2.5 http adapter hardening (Slice 1–10: all slices ✅ DONE 2026-04-04; adapter suite passes; gateway integration tests for slices 1,2,5,6,7,8,9,10 re-run successfully) | ✅ DONE | `30-production-roadmap.md` P2.5 |
| 2 | P2.1 fs adapter hardening (Slice 1: fail-closed verify on I/O errors ✅; Slice 2: compensate deletes new file when no snapshot ✅ 2026-04-07; Slice 3: fail-closed compensate/rollback on I/O error during recovery ✅ 2026-04-08; Slice 4: gateway-level fs verify hash mismatch → Failed → commit rejected ✅ 2026-04-08; Slice 5: gateway-level fs rollback drill after verify false ✅ 2026-04-08; Slice 6: gateway-level fs compensate drill after verify false ✅ 2026-04-08) | ✅ DONE | `30-production-roadmap.md` P2.1 |
| 3 | P2.2 sqlite adapter hardening (Slice 1: identifier safety + rollback noop tests ✅ 2026-04-04; Slice 2: file-backed lifecycle + error-path tests ✅ 2026-04-04; Slice 3: fail-closed verify on DB-open error ✅ 2026-04-07; Slice 4: fail-closed compensate/rollback on DB error during recovery ✅ 2026-04-07; Slice 5: fail-closed verify on DB-corruption mid-operation ✅ 2026-04-07; Slice 6: gateway-level sqlite verify false → Failed → commit rejected ✅ 2026-04-08; Slice 7: gateway-level sqlite rollback drill after verify false ✅ 2026-04-08; Slice 8: gateway-level sqlite compensate drill after verify false ✅ 2026-04-08) | ✅ DONE | `30-production-roadmap.md` P2.2 |
| 4 | P2.3 git adapter hardening (Slice 1: fail-closed verify + noop edge-case tests ✅; Slice 2: GitBranchCreate prepare fails closed on detached HEAD ✅ 2026-04-04; Slice 3: GitPush rollback no-op when no pre_push_ref ✅ 2026-04-07; Slice 4: GitFetch rollback restores existing local ref ✅ 2026-04-08; Slice 5: GitPull compensate/rollback fail-closed when branch changed since prepare/execute ✅ 2026-04-08; Slice 6: gateway-level git verify false → Failed → commit rejected ✅ 2026-04-08; Slice 7: GitPush rollback fail-closed when recovery force-push fails ✅ 2026-04-08; Slice 8: GitFetch rollback fail-closed when recovery force-update fails ✅ 2026-04-08; Slice 9: gateway-level git rollback drill after verify false ✅ 2026-04-08; Slice 10: gateway-level git compensate drill after verify false ✅ 2026-04-08) | ✅ DONE | `30-production-roadmap.md` P2.3 |
| 5 | P2.4 git remote workflows (Slice 1: GitPush ✅ 2026-04-04; Slice 2: GitFetch ✅ 2026-04-04; Slice 3: GitPull fast-forward-only ✅ 2026-04-04) | ✅ DONE | `30-production-roadmap.md` P2.4 |
| 6 | P2.6 EmailSend governed-path rollout (scaffold ✅ 2026-04-04; governed-path entry analysis ✅; adapter contract draft ✅; scaffold-only adapter ✅; mock-provider foundation ✅; provider-injection structural ✅; internal typed payload parser ✅; real provider send integration TBD post-v1/non-blocking — G-E1 boundary satisfied by scaffold completion) | ✅ DONE | `30-production-roadmap.md` P2.6; `36-p2-6-emailsend-governed-path-entry-analysis.md`; `37-p2-6-emailsend-adapter-contract-draft.md`; `38-p2-6-emailsend-adapter-scaffold-implementation.md` |
| 7 | P2.7 maildraft broader verify semantics hardening (Slice 1: explicit EmailDraftExists verify_checks handling ✅ 2026-04-04; Slice 2: fail-closed verify on storage/db error ✅ 2026-04-04; Slice 3: malformed explicit check fail-closed strictness ✅ 2026-04-04; Slice 4: compensate/rollback fail-closed on storage/db error during delete ✅ 2026-04-04; Slice 5: gateway-level fail-closed on storage/db error ✅ 2026-04-04) | ✅ DONE | `30-production-roadmap.md` P2.7 |

### Post-v1 Backlog Index

Post-v1 backlog is now structured in **`50-post-v2-roadmap.md`** (Horizons H1/H2/H3).
Longer-term planned tracks from this file are indexed below for reference only —
full detail is in the roadmap doc.

| Item | Status | Horizon | Roadmap ref |
|------|--------|---------|-------------|
| P4.1 `ferrumctl` advanced operator flows | ✅ DONE | — | `30-production-roadmap.md` P4.1 |
| P4.2 Policy bundle lifecycle tooling | ✅ DONE — H1.1a–H1.1d all delivered | H1 | `50-post-v2-roadmap.md` H1.1 |
| P5.4–P5.5 Sync-1 preflight + decision table | ✅ DONE | — | `30-production-roadmap.md` P5.4–P5.5 |
| P5.7 HA / multi-leader replication | ⬜ PLANNED | H2 | `50-post-v2-roadmap.md` H2.1 |
| U1 remaining backlog (expressiveness + authoring CLI) | ⬜ PLANNED | H1 | `50-post-v2-roadmap.md` H1.2 |
| U2 Reversible Execution Planner | ⬜ PLANNED | H2 | `50-post-v2-roadmap.md` H2.2 |
| U3 Cross-runtime Provenance Fabric | ⬜ PLANNED | H3 | `50-post-v2-roadmap.md` H3.1 |
| U4 Runtime Integrations (MCP / local / NemoClaw) | ⬜ PLANNED | H3 | `50-post-v2-roadmap.md` H3.2 |

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
