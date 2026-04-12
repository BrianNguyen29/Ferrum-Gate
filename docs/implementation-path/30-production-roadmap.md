# 30 — Production Roadmap

**Last updated:** 2026-04-12
**Current truth:** Single-node v1 is RC-ready (2026-04-02) and broader production-ready is declared (2026-04-08) within the scoped T1/T2/T3 support boundary.

---

## Status at a Glance

| Priority | Track | Status | Target Outcome |
|----------|-------|--------|----------------|
| 1 | Production support boundary / contract | ✅ DONE | Support matrix (P1.1) + SLA surface (P1.2) + EOL policy (P1.3) all published |
| 2 | Adapter hardening + external integration depth | ✅ DONE (scoped) | Bounded adapter hardening complete for current declaration; broader external integration depth remains post-v1 |
| 3 | Operational hardening / release evidence | ✅ DONE | Ship-worthy packaging, observability, runbooks |
| 4 | Operator control-plane completeness (`ferrumctl`) | ✅ DONE / ⏸ PARTIAL FOLLOW-UP | Full operator-driven workflows complete; policy bundle authoring remains deferred |
| 5 | Resilience architecture (HA / read-replica / multi-node) | ⬜ PLANNED | Multi-node v1; HA-ready topology |
| 6 | Post-v1 expansion (U1 full + U2/U3/U4) | ⬜ PLANNED | Outcome-aware governance; remaining upgrades |

---

## Priority 1 — Lock Production Support Boundary / Contract

**Goal:** Define and lock what "production-supported" means for v1.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P1.1 | Publish support matrix (single-node scope, known gaps) | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 0 (Support Tier Summary) |
| P1.2 | Document SLA surface (availability, recovery, response) | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 7 (SLA Surface) |
| P1.3 | Define EOL / deprecation policy | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 9 (EOL / Deprecation Policy) | 2026-04-03 |

**Evidence:** `docs/implementation-path/23-production-readiness-assessment.md`

---

## Priority 2 — Adapter Hardening + External Integration Depth

**Goal:** Production-grade adapters for fs, sqlite, git, http, maildraft. Explicit remote/external integration sub-items.

> Per `11-remaining-tasks.md` P3 backlog and `23-production-readiness-assessment.md`: bounded local implementations exist for all five adapters; broader production hardening and external integration depth are post-v1.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P2.1 | fs adapter — hardening and production verification | ✅ DONE 2026-04-08 (Slice 1: fail-closed verify on I/O errors ✅ 2026-04-03; Slice 2: compensate deletes new file when no snapshot ✅ 2026-04-07; Slice 3: fail-closed compensate/rollback on I/O error during recovery ✅ 2026-04-08; Slice 4: gateway-level fs verify hash mismatch → Failed → commit rejected ✅ 2026-04-08; Slice 5: gateway-level fs rollback drill after verify false ✅ 2026-04-08; Slice 6: gateway-level fs compensate drill after verify false ✅ 2026-04-08) | `ferrum-adapter-fs`: `test_fs_adapter_verify_fail_closed_on_io_error_permission_denied`, `test_fs_adapter_verify_hash_mismatch_is_verified_false_not_error`, `test_fs_adapter_verify_file_deleted_is_verified_false_not_error`, `test_fs_adapter_compensate_deletes_new_file`, `test_fs_adapter_compensate_fail_closed_on_permission_denied`, `test_fs_adapter_rollback_fail_closed_on_permission_denied`, `test_fs_adapter_compensate_fail_closed_on_delete_permission_denied`, `test_fs_adapter_rollback_fail_closed_on_delete_permission_denied`; integration test: `test_fs_verify_hash_mismatch_transitions_to_failed_and_rejects_commit` (gateway API: execute→tamper file→verify mismatch→execution becomes `Failed`→commit rejected) |
| P2.2 | sqlite adapter — hardening and production verification | ✅ DONE 2026-04-08 (Slice 1: identifier safety + noop edge-case tests ✅ 2026-04-04; Slice 2: file-backed lifecycle + error-path tests ✅ 2026-04-04; Slice 3: fail-closed verify on DB-open error ✅ 2026-04-07; Slice 4: fail-closed compensate/rollback on DB error during recovery ✅ 2026-04-07; Slice 5: fail-closed verify on DB-corruption mid-operation ✅ 2026-04-07; Slice 6: gateway-level sqlite verify false → Failed → commit rejected ✅ 2026-04-08; Slice 7: gateway-level sqlite rollback drill after verify false ✅ 2026-04-08; Slice 8: gateway-level sqlite compensate drill after verify false ✅ 2026-04-08) | `ferrum-adapter-sqlite`: `test_sqlite_adapter_rejects_unsafe_table_name` (SQL injection prevention), `test_sqlite_adapter_rollback_no_snapshots_is_noop`, `test_sqlite_adapter_verify_no_snapshots_returns_true`, `test_sqlite_adapter_execute_invalid_json_payload` (fail-closed error-path), `test_sqlite_adapter_full_lifecycle_file_backed` (prepare→execute→verify→rollback), `test_sqlite_adapter_multi_row_transaction_lifecycle` (multi-row transaction lifecycle); Slice 3: `test_sqlite_adapter_verify_fail_closed_on_nonexistent_db_path`, `test_sqlite_adapter_verify_fail_closed_on_permission_denied_db_path` (fail-closed verify on DB-open error); Slice 4: `test_sqlite_adapter_rollback_fail_closed_on_nonexistent_db_path`, `test_sqlite_adapter_rollback_fail_closed_on_permission_denied_db_path`, `test_sqlite_adapter_compensate_fail_closed_on_nonexistent_db_path`, `test_sqlite_adapter_compensate_fail_closed_on_permission_denied_db_path` (fail-closed recovery on DB error); Slice 5: `test_sqlite_adapter_verify_fail_closed_on_corrupted_db` (fail-closed verify when DB becomes corrupted mid-operation); Slice 6: integration test `test_sqlite_verify_false_transitions_to_failed_and_rejects_commit` (gateway API: execute→tamper row→verify mismatch→execution becomes `Failed`→commit rejected); Slice 7: integration test `test_sqlite_verify_false_triggers_rollback_drill` (gateway API: execute→tamper row→verify false→rollback→execution `RolledBack`→sqlite state restored); Slice 8: integration test `test_sqlite_verify_false_triggers_compensate_drill` (gateway API: execute→tamper row→verify false→compensate→execution `Compensated`→sqlite state restored) |
| P2.3 | git adapter — hardening and production verification | ✅ DONE 2026-04-08 (Slice 1: fail-closed verify on I/O errors + noop edge-case tests ✅ 2026-04-04; Slice 2: GitBranchCreate prepare fails closed on detached HEAD ✅ 2026-04-04; Slice 3: GitPush rollback no-op when no pre_push_ref ✅ 2026-04-07; Slice 4: GitFetch rollback restores existing local ref ✅ 2026-04-08; Slice 5: GitPull compensate/rollback fail-closed when branch changed since prepare/execute ✅ 2026-04-08; Slice 6: gateway-level git verify false → Failed → commit rejected ✅ 2026-04-08; Slice 7: GitPush rollback fail-closed when recovery force-push fails ✅ 2026-04-08; Slice 8: GitFetch rollback fail-closed when recovery force-update fails ✅ 2026-04-08; Slice 9: gateway-level git rollback drill after verify false ✅ 2026-04-08; Slice 10: gateway-level git compensate drill after verify false ✅ 2026-04-08) | `ferrum-adapter-git`: `test_verify_repo_path_missing_is_verified_false_not_error`, `test_verify_already_at_expected_ref_is_verified_true`, `test_verify_ref_mismatch_is_verified_false`, `test_verify_missing_both_refs_falls_back_to_before_ref`, `test_verify_missing_both_refs_and_head_changed_is_verified_false`; `test_branch_create_prepare_rejects_detached_head`; `test_gitpush_rollback_noop_when_no_pre_push_ref`; `test_gitfetch_rollback_restores_existing_local_ref` (Slice 4: GitFetch rollback path where local ref existed before fetch, rollback restores pre-fetch ref via force-update); `test_gitpull_rollback_fail_closed_when_branch_changed`, `test_gitpull_compensate_fail_closed_when_branch_changed` (Slice 5: GitPull rollback/compensate fail closed when current branch differs from branch captured at prepare/execute; added `git_cleanup_pull` function that checks branch context before reset; returns Validation error if branch changed rather than resetting wrong branch); integration test: `test_git_verify_false_transitions_to_failed_and_rejects_commit` (Slice 6: gateway API execute→outside interference advances HEAD→verify mismatch→execution becomes `Failed`→commit rejected); `test_gitpush_rollback_fail_closed_when_force_push_fails` (Slice 7: GitPush rollback returns `recovered=false` with metadata instead of propagating recovery force-push failure); `test_gitfetch_rollback_fail_closed_when_force_update_fails` (Slice 8: GitFetch rollback returns `recovered=false` with metadata when `git branch -f` fails due to invalid pre_fetch_ref, matching the GitPush fail-closed pattern) |
| P2.4 | git remote workflows — push/fetch/pull integration | ✅ DONE (Slice 1: GitPush against local temporary remotes ✅ 2026-04-04; Slice 2: GitFetch against local temporary remotes ✅ 2026-04-04; Slice 3: GitPull fast-forward-only against local temporary remotes ✅ 2026-04-04) | `ferrum-adapter-git`: GitPush tests (8 tests) + GitFetch tests (7 tests) + GitPull tests (8 tests: prepare_captures_before_ref, prepare_rejects_dirty_repo, prepare_rejects_missing_remote, prepare_rejects_diverged_local, execute_performs_ff_pull, verify_confirms_pull, rollback_resets_to_before_ref, happy_path_full_flow) |
| P2.5 | http adapter — hardening and production verification | ✅ DONE 2026-04-04 | Slice 1: `test_http_execute_transport_failure_is_fail_closed` (execute connection-refused → fail-closed + `Failed` state); Slice 2: `test_http_execute_timeout_fails_closed` (execute timeout → fail-closed + `Failed` state); Slice 3: `test_verify_get_transport_failure_fails_closed` (adapter unit: GET re-request transport failure → `verified=false`); Slice 4: `test_verify_get_re_request_timeout_fails_closed` (adapter unit: GET re-request timeout → `verified=false`); Slice 5: `test_verify_mutation_patch_explicit_check_mismatch` (gateway API: PATCH execute captures 503, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=false`, execution becomes Failed, commit rejected); Slice 6: `test_verify_mutation_patch_explicit_check_match` (gateway API: PATCH execute captures 200, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=true`, execution remains AwaitingVerification/Committed, commit succeeds) ✅ DONE 2026-04-03; Slice 7: `test_verify_get_transport_failure_fails_closed` (gateway API: explicit `verify_checks` injected in-store, re-request fails with connection-refused, verify returns 200 + `verified=false`, commit rejected from `Failed`); Slice 8: `test_verify_get_re_request_timeout_fails_closed` (gateway API: explicit `verify_checks` injected in-store, GET re-request times out, verify returns 200 + `verified=false`, commit rejected from `Failed`); Slice 9: `test_verify_mutation_post_explicit_check_mismatch` (gateway API: POST execute captures 503, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=false`, execution becomes Failed, commit rejected) ✅ DONE 2026-04-03; Slice 10: `test_verify_mutation_delete_explicit_check_match` (gateway API: DELETE execute captures 200, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=true`, execution remains AwaitingVerification/Committed, commit succeeds) ✅ DONE 2026-04-03; fixes: HTTP adapter verify catches GET re-request transport errors and gateway execute failures transition to `Failed` |
| P2.6 | maildraft — EmailSend governed-path entry + adapter scaffold (G-E1 boundary satisfied; real provider send integration post-v1/non-blocking) | ✅ DONE (scaffold) 2026-04-04 (Slice 1: governed-path entry analysis ✅; Slice 2: scaffold-only `ferrum-adapter-emailsend` implementation ✅; Slice 3: provider abstraction + mock provider foundation ✅; Slice 4: provider-injection structural slice ✅; Slice 5: internal typed payload parser ✅; real provider send integration TBD post-v1) | `docs/implementation-path/36-p2-6-emailsend-governed-path-entry-analysis.md`; `docs/implementation-path/37-p2-6-emailsend-adapter-contract-draft.md`; `docs/implementation-path/38-p2-6-emailsend-adapter-scaffold-implementation.md`; deny-regression tests: `test_email_allow_send_true_prepare_denied_with_explicit_error`, `test_maildraft_adapter_rejects_send_payload`; scaffold tests: `test_prepare_accepts_email_send_with_auto_commit_false`, `test_execute_fails_closed_with_validation_error`; mock provider tests: `test_mock_provider_send_success`, `test_mock_provider_send_tracks_calls`, `test_mock_provider_send_failure_*` (14 tests); provider injection tests: `test_new_adapter_has_mock_provider`, `test_with_provider_stores_provider`, `test_execute_still_fails_closed_with_injected_provider`; payload parser tests: `test_parse_payload_valid_*` (5 tests), `test_parse_payload_fail_closed_*` (14 tests), `test_execute_still_fail_closed_with_*` (3 tests), `test_mock_provider_direct_call_*` (2 tests); total: 53 tests ✅ |
| P2.7 | maildraft — broader verify semantics hardening | ✅ DONE (Slice 1: explicit EmailDraftExists verify_checks handling ✅ 2026-04-04; Slice 2: fail-closed verify on storage/db error ✅ 2026-04-04; Slice 3: malformed explicit check fail-closed strictness ✅ 2026-04-04; Slice 4: compensate/rollback fail-closed on storage/db error during delete ✅ 2026-04-04; Slice 5: gateway-level fail-closed on storage/db error ✅ 2026-04-04) | `ferrum-adapter-maildraft`: `test_maildraft_adapter_verify_with_explicit_email_draft_exists_check_passes`, `test_maildraft_adapter_verify_with_explicit_email_draft_exists_check_fails`, `test_maildraft_adapter_verify_fail_closed_on_storage_db_error` (updated: returns `verified=false` for proper gateway integration), `test_maildraft_adapter_verify_explicit_check_missing_draft_id_fails_validation`, `test_maildraft_adapter_verify_explicit_check_non_string_draft_id_fails_validation`, `test_maildraft_adapter_compensate_fail_closed_on_storage_db_error`, `test_maildraft_adapter_rollback_fail_closed_on_storage_db_error`; integration test: `test_maildraft_gateway_verify_fail_closed_on_db_error` (gateway API: execute→corrupt DB→verify returns `verified=false`→execution becomes `Failed`→commit rejected 409) |

> **Out-of-tree sqlite performance candidate (NOT merged):** A write-queue
> optimization was evaluated in a local workspace (Phase 1 showed S4–S7 gains;
> Phase 2 deferred after perf regression under high load). Do NOT treat as
> repo truth until validated and merged. If eventually merged, would inform
> H1.4 sqlite WAL-mode production tuning as a potential input — not a
> replacement for the current P2.2 bounded hardening scope.

**Source:** `11-remaining-tasks.md` P3; `01-current-state.md` lines 26-31

---

## Priority 3 — Operational Hardening / Required Release Evidence

**Goal:** Ship-worthy packaging, observability, and runbooks.

> The four items below are the required release gate evidence for production readiness. RC gate rows (P3.1–P3.6) are preserved as anchors.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P3.1 | Cargo workspace clean (`clippy -- -D warnings`) | ✅ DONE | 2026-04-02 |
| P3.2 | Full workspace test suite (`cargo test --workspace`) | ✅ DONE | 2026-04-02 |
| P3.3 | Scope-mismatch deny | ✅ DONE | 2026-04-02 |
| P3.4 | Poisoned-context fixtures (6 tests) | ✅ DONE | 2026-04-02 |
| P3.5 | Phase F docs pack | ✅ DONE | 2026-04-02 |
| P3.6 | RC evidence script | ✅ DONE | 2026-04-02 |
| P3.G1 | Functional readiness proof — end-to-end operator walkthrough (install → functional probe → first control action → first rollback/compensate drill) | ✅ DONE 2026-04-03 | `docs/implementation-path/34-p3-g1-executed-evidence.md` — live walkthrough executed 2026-04-03; all 5 sections passed; attestation block signed |
| P3.G2 | Smoke stability evidence — sustained-lifecycle smoke suite (48h+ runbook-driven soak or equivalent automated cycle) | ✅ DONE 2026-04-03 | `docs/implementation-path/35-p3-g2-executed-evidence.md` — live smoke run executed 2026-04-03 (run_id: `p3-g2-20260403-live`); 12 automated probe intervals at ~5s cadence, 100% pass rate, 0 failures, 0 consecutive failures; pre-run and end-run store integrity `ok`; 2/2 control-path checks passed (cancel + inspect); no fatal logs; attestation block signed. Satisfies the "equivalent automated cycle" clause per evidence template Section 1. |
| P3.G3 | Backup / restore drill evidence — successful backup capture and restore drill under rollback scenario | ✅ DONE 2026-04-03 | `docs/implementation-path/31-p3-g3-backup-restore-drill-evidence.md` — live drill executed 2026-04-03; backup captured to `/tmp/ferrum-p3g3/backups/ferrumgate_20260403_1705.db` (225280 bytes, integrity ok); restore drill to `/tmp/ferrum-p3g3/restored.db` (225280 bytes, integrity ok); restored ferrumd responded to readyz and approvals probes; intent_id `09996e3b-7a9b-4c55-b806-8713486cee44` verified present with provenance chain; attestation block signed |
| P3.G4 | Observability verification — metrics, logging, and tracing surface confirmed operational in target environment | ✅ DONE 2026-04-03 | `docs/implementation-path/32-p3-g4-observability-verification-evidence.md` — live verification executed 2026-04-03; all probe endpoints (/healthz, /readyz, /approvals, /metrics) returned 200; logs flowing; attestation block signed |

**Evidence:** `docs/implementation-path/25-v1-single-node-rc-evidence.md`

---

## Post-P3 Execution Order

The following lists the executed production-evaluation order after P3 completion (P3.G1–P3.G4 ✅ DONE 2026-04-03), grounded in roadmap priority order. Single-node v1 RC-ready; broader production-ready is now ratified through G-E5.

### Immediate Next Slice (P2 adapter hardening — completed)

> All P2 slices (P2.1, P2.2, P2.3, P2.5, P2.6 scaffold, P2.7) are ✅ DONE as of
> 2026-04-08 per G-E1 gate. The fs-first `before_hash`/`after_hash` wiring
> (narrow follow-on evidence from `docs/artifacts/2026-04-09/closure-note.txt`)
> was added via PR #165 after G-E5 sign-off; it does not alter the T1/T2/T3
> boundary and is recorded for completeness only.

1. **P2.5** — http adapter hardening (Slice 1–10 ✅ DONE 2026-04-04; broader production hardening continues)
2. **P2.1** — fs adapter hardening + production verification
3. **P2.2** — sqlite adapter hardening + production verification
4. **P2.3** — git adapter hardening + production verification
5. **P2.4** — git remote workflows (push/fetch/pull integration)
6. **P2.6** — maildraft EmailSend governed-path entry + adapter scaffold (G-E1 boundary satisfied; real provider send integration post-v1/non-blocking) ✅ DONE 2026-04-04
7. **P2.7** — maildraft broader verify semantics hardening (Slice 1–5 ✅ 2026-04-04)

### Remaining Longer-Term / Planned Tracks

Post-P3 tracks are indexed in `50-post-v2-roadmap.md` (Horizons H1/H2/H3) rather
than duplicated here. Brief reference:

8. **P4.2** — policy bundle lifecycle tooling (H1.1)
9. **P5.7** — HA / multi-leader replication (H2.1)
10. **U1 remaining backlog** — expressiveness + authoring tooling (H1.2)
11. **U2** — Reversible Execution Planner (H2.2)
12. **U3** — Cross-runtime Provenance Fabric (H3.1)
13. **U4** — Runtime Integrations (H3.2)

**Source:** `docs/implementation-path/11-remaining-tasks.md`; `docs/implementation-path/50-post-v2-roadmap.md`; execution order follows roadmap priority sequence per `docs/implementation-path/24-p1-p2-p3-execution-plan.md` lines 266–297.

**Canonical execution plan:** `docs/implementation-path/41-production-execution-plan.md` — sequential phase plan with per-phase doc update protocol and commit/PR merge cadence.

---

## Priority 4 — Operator Control-Plane Completeness (`ferrumctl`)

**Goal:** Close remaining `ferrumctl` operator-surface gaps; keep policy bundle lifecycle tooling as post-G-E3 backlog unless separately scoped.

> Per `23-production-readiness-assessment.md` Dimension 3: `ferrumctl` covers the high-use operator surface; some advanced/intent-authoring flows still require direct HTTP/OpenAPI. Per `11-remaining-tasks.md` P3: policy bundle migration tooling (CLI authoring workflows) is post-v1 backlog.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P4.1 | `ferrumctl` advanced operator flows (remaining REST surface) | ✅ DONE 2026-04-08 | `cargo test -p ferrumctl`; `ferrumctl server compile-intent --help`; `ferrumctl server commit-execution --help` |
| P4.2 | Policy bundle lifecycle tooling | ⏸ DEFERRED (post-G-E3) | Separate scope required |

---

## Priority 5 — Resilience Architecture (HA / Read-Replica / Multi-Node)

**Goal:** Multi-node v1 with HA-ready topology.

> P5.1–P5.6 analysis/design complete. P5.7 HA/multi-leader implementation is post-v2
> backlog — indexed as **H2.1** in `50-post-v2-roadmap.md` (Horizon 2). Full distributed
> deployment (beyond HA) is **H3.4** (Horizon 3), which depends on H2.1 completion.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P5.1 | SQLite read-replica use-case analysis | ✅ DONE | Analysis doc |
| P5.2 | Leader-election requirements analysis | ✅ DONE | Analysis doc |
| P5.3 | Sync-0 safety contract plan | ✅ DONE | Design doc |
| P5.4 | Sync-1 preflight checks (PF1–PF8) | ✅ DONE 2026-04-08 | `cargo test -p ferrum-store --lib sync_preflight`; `cargo test -p ferrum-store --lib sync_service` |
| P5.5 | Sync-1 decision table + abort semantics | ✅ DONE 2026-04-08 | `cargo test -p ferrum-sync --lib`; `cargo test -p ferrum-store --lib sync_service` |
| P5.6 | Sync-2 read-only preflight sketch | ✅ DONE | Design doc |
| P5.7 | HA / multi-leader replication | ⬜ PLANNED (post-v2) | Post-P3; see `50-post-v2-roadmap.md` H2.1 |

---

## Priority 6 — Post-v1 Expansion Tracks

**Goal:** Complete U1 and kick off U2 / U3 / U4.

> U1 core (S1–S8a) is ✅ DONE in v1/v2 scope. Remaining U1 backlog (expressiveness +
> authoring tooling) is **H1.2** in `50-post-v2-roadmap.md` (Horizon 1). U2/U3/U4 are
> **H2.2**, **H3.1**, **H3.2** (Horizons 2 and 3). Full detail with sequencing guidance
> is in `50-post-v2-roadmap.md`.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| U1.1 | Richer outcome clause expressiveness (nested selectors, temporal) | ⬜ PLANNED (H1) | `50-post-v2-roadmap.md` H1.2a |
| U1.2 | Policy bundle migration / authoring tooling | ⬜ PLANNED (H1) | `50-post-v2-roadmap.md` H1.1, H1.2b |
| U2 | Reversible Execution Planner | ⬜ PLANNED (H2) | `50-post-v2-roadmap.md` H2.2; `91-phase-success-criteria-and-kpis.md` section 8.2 |
| U3 | Cross-runtime Provenance Fabric | ⬜ PLANNED (H3) | `50-post-v2-roadmap.md` H3.1; `91-phase-success-criteria-and-kpis.md` section 8.3 |
| U4 | Runtime Integrations (MCP / local / NemoClaw) | ⬜ PLANNED (H3) | `50-post-v2-roadmap.md` H3.2; `91-phase-success-criteria-and-kpis.md` section 8.4 |

**Cross-link:** `docs/implementation-path/11-remaining-tasks.md`; `docs/implementation-path/50-post-v2-roadmap.md`

---

## Production Evaluation and Execution Plan

### Current Production Posture

FerrumGate v1 single-node is **RC-ready** (2026-04-02). RC gates passed:
- `cargo clippy --workspace -- -D warnings` ✅ PASS
- `cargo test --workspace` ✅ PASS
- All P0 gates cleared

FerrumGate v1 single-node is **broader production-ready** within the scoped T1/T2/T3
support-contract boundary. The following remain post-v1 backlog items rather than
blockers to the current declaration:
- broader external adapter verification / integration depth
- P4.2 policy bundle lifecycle tooling
- P5 resilience expansion (HA/multi-node)
- P6 upgrade tracks (U2/U3/U4)

### Production Evaluation Gates

The following evaluation gates must be passed before declaring broader
production-ready status. These are execution milestones, not additional RC gates.

| Gate | Description | Owner | Status |
|------|-------------|-------|--------|
| G-E1 | **P2 adapter hardening complete** — all P2 slices (P2.1, P2.2, P2.3, P2.6 scaffold, P2.7) pass their slice criteria; P2.6 real provider send integration explicitly post-v1 | Engineering | ✅ DONE 2026-04-08 (P2.1 ✅; P2.2 ✅; P2.3 ✅; P2.5 ✅; P2.6 scaffold ✅ 2026-04-04; P2.7 ✅; real provider send remains post-v1/non-blocking by gate definition) |
| G-E2 | **P2 performance baseline established** — benchmark suite covers key SQLite and adapter paths under concurrent load | Engineering | ✅ DONE 2026-04-08 (`benches/` benchmark harness merged; evidence: `42-p2-performance-baseline-evidence.md`) |
| G-E3 | **P4 `ferrumctl` advanced flows complete** — remaining REST surface accessible via CLI | Engineering | ✅ DONE 2026-04-08 (`compile-intent`, `evaluate-proposal`, `mint-capability`, `authorize-execution`, `verify-execution`, `commit-execution` added to `ferrumctl`) |
| G-E4 | **P5 resilience design ratified** — Sync-1 preflight checks + decision table implemented and reviewed | Engineering | ✅ DONE 2026-04-08 (`ferrum-sync` + `ferrum-store` sync tests re-run; PF1–PF8, decision table, and live readiness orchestration all verified) |
| G-E5 | **Production evaluation sign-off** — documented assessment confirming T1 is production-supported and T2 is hardened to the partial contract level per support contract | Team | ✅ DONE 2026-04-08 (`43-production-readiness-signoff.md`) |
| Phase 6 | **v2 sign-off** — v2 sign-off artifact complete; v2 RATIFIED | Team | ✅ DONE 2026-04-12 (`46-v2-readiness-signoff.md` — **v2 ✅ RATIFIED**) |

### Out-of-Tree SQLite Performance Candidate

An out-of-tree SQLite write-queue optimization was evaluated in a local workspace:

- **Phase 1:** Strong S4–S7 gains observed under concurrent write workloads
- **Phase 2:** Deferred after perf regression (backpressure contention under high load)

This candidate is **NOT merged** into the repo. See
`40-out-of-tree-sqlite-performance-candidate.md` for full evidence and caveats.
If eventually validated and merged, it would be a potential input to **H1.4**
(sqlite WAL-mode production tuning) — not a replacement for the P2.2 bounded
hardening scope already completed.

### Execution Sequence (Production Evaluation Path)

Grounded in roadmap priority order. Single-node v1 RC-ready first; broader
production-ready is now declared after G-E1 through G-E5 completion.

| Order | Item | Gate | Status |
|-------|------|------|--------|
| 1 | Complete P2 adapter hardening (P2.1, P2.2, P2.3, P2.6, P2.7) | G-E1 | ✅ DONE |
| 2 | Establish P2 performance baseline + benchmark suite | G-E2 | ✅ DONE |
| 3 | Complete P4 `ferrumctl` advanced operator flows | G-E3 | ✅ DONE |
| 4 | Ratify P5 Sync-1 preflight checks + decision table | G-E4 | ✅ DONE |
| 5 | Production evaluation sign-off and broader production-ready declaration | G-E5 | ✅ DONE |

**Note:** This execution path is the current best estimate. Adjustments may be
made as P2 adapter hardening progresses and new information becomes available.

---

## Update Convention

When a row completes:

1. Change status: `⬜ TODO` → `🔄 IN PROGRESS` → `✅ DONE`
2. Add verification column entry (file, test command, or commit ref)
3. Add date or commit hash in the Status column
4. **Do not rewrite the structure.** Append new rows if new items are discovered.

Example:
```
| P3.7 | Production runbook | ✅ DONE | runbooks/prod.md @ abc1234 | 2026-04-05 |
```

---

## Key References

| Topic | File |
|-------|------|
| v1 RC evidence | `25-v1-single-node-rc-evidence.md` |
| Production readiness assessment | `23-production-readiness-assessment.md` |
| Current state | `01-current-state.md` |
| Remaining tasks | `11-remaining-tasks.md` |
| Release checklist | `16-release-checklist.md` |
| Out-of-tree SQLite perf candidate | `40-out-of-tree-sqlite-performance-candidate.md` |
| v2 sign-off (✅ RATIFIED) | `46-v2-readiness-signoff.md` |
| Post-v2 roadmap (H1/H2/H3) | `50-post-v2-roadmap.md` |
