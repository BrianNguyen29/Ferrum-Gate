# 23 — Production readiness assessment

Single-node v1 scope. Assessment against FerrumGate v1 success criteria.

**Release support contract**:
- Supported = single-node governance core with SQLite-backed persistence.
- Partial = adapter crate surfaces (fs/sqlite/maildraft/git/http) and limited `ferrumctl` inspect surface.
- Deferred/post-v1 = real adapter implementations, multi-node/HA/read-replica, PostgreSQL.

## Overall readiness: RC-READY — CONDITIONAL PRODUCTION

**FerrumGate v1 is RC-ready** for single-node SQLite-backed deployment.
All P0/P1/P2 items verified complete. Core governance loop implemented and test-covered.

**Production posture**: Phase 1 only — SQLite write queue. Phase 2 (transaction batching + direct UPDATE) was partially implemented but **deferred/regressed** due to performance regression in benchmarking. Phase 3 (PostgreSQL migration) is the path to full production scale.

**Conditional production constraint**: Full "production-ready" posture is not claimed because operational constraints remain (SQLite single-node write limits, bounded offline/local `ferrumctl backup` workflow with opt-in retention pruning (`--retention-days N`), no built-in incremental backup, no automated scheduling, no encryption; broader observability deferred; PostgreSQL/multi-node deferred; operator sign-off required before production deployment). All 12 invariants are VERIFIED per invariant matrix. The release is **RC-ready** with known accepted risks; operators should evaluate against the production evaluation plan before production deployment.

Remaining gaps are post-v1 backlog documented in `11-remaining-tasks.md` P3.

---

## Dimension 1 — Technical correctness

| Criterion | Status | Evidence |
|---|---|---|
| Workspace compiles | PASS | Fresh P6 validation (2026-04-28): `cargo check --workspace` exit 0 |
| `cargo fmt --all --check` | PASS | Fresh P6 validation: `cargo fmt --all -- --check` exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Fresh P6 validation (2026-04-28) exit 0 |
| Core tests pass | PASS | Fresh feature-completeness validation: `cargo test --workspace` ~797 observed tests pass |
| Integration tests pass | PASS | `ferrum-integration-tests` suite |
| No R3 auto-commit violations | PASS | `test_r3_contracts_have_auto_commit_false` |
| Single-use capability enforced (durable mark-used in authorize path) | PASS | `test_single_use_capability_cannot_be_reused` — see Weak Spot 3 (resolved) |
| Provenance emitted for supported flows | PASS | `test_lineage_endpoint_*` series |
| Rollback/compensate distinct ops | PASS | `test_rollback_and_compensate_are_distinct_operations` |

**Known gaps**: See support contract (19-v1-single-node-support-contract.md) Accepted Risks §4 and invariant matrix (26-EV-v1-single-node-invariant-control-test-evidence-matrix.md). WS1–WS4 are resolved, all 12 invariants are VERIFIED, and output sanitization (Invariant 11) has bounded gateway wiring. Remaining gaps are operational limitations plus PostgreSQL/multi-node/real-adapter backlog.

---

## Dimension 2 — Governance enforcement

| Criterion | Status | Evidence |
|---|---|---|
| Intent required before mutation | PASS | gateway requires intent_id for all flows |
| Capability single-use | PASS | `cap.mark_used` -> AlreadyUsed on reuse |
| Capability scoped narrowly | PASS | scope-bounds mismatch explicitly enforced (P0 resolved) |
| Provenance chain maintained | PASS | lineage endpoint returns events for execution |
| Rollback contract per mutation | PASS (service-level) | R0/R1/R2/R3 contract classes with auto_commit semantics — WS1 resolved (rollback_class loaded at prepare) |
| R3 requires approval | PASS | StaticPdpEngine returns RequireApproval for R3 |
| Draft-only enforcement | PASS (evaluate-level) | draft-only gated at evaluate (before prepare) — WS2 resolved (revalidated at prepare) |

**Known gaps**: See Accepted Risks §4 (19-v1-single-node-support-contract.md) and Weak Spots 1–4 (26-EV-v1-single-node-invariant-control-test-evidence-matrix.md). WS1-WS3 gaps are resolved in code; WS4 provenance completeness is resolved by integration test.

---

## Dimension 3 — Operational completeness

| Criterion | Status | Evidence |
|---|---|---|
| SQLite persistence | PASS | `ferrum-store` with embedded migrations |
| CLI usable for inspection | PARTIAL | health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance |
| Config docs | PASS | `docs/ferrumgate-roadmap-v1/15-deployment-and-operations.md` |
| Approval workflow | PASS | GET /v1/approvals with pagination/filter |
| Provenance query | PASS | GET/POST /v1/provenance/lineage, POST /v1/provenance/query |

**Known gaps**:
- `ferrumctl` limited to read/inspect operations; no mutating commands (post-v1 backlog).

---

## Dimension 4 — Documentation completeness

| Criterion | Status | Evidence |
|---|---|---|
| Project canon | PASS | `docs/ferrumgate-roadmap-v1/00-project-canon.md` |
| Business overview | PASS | `docs/ferrumgate-roadmap-v1/02-project-overview.md` |
| Runtime flow | PASS | `docs/ferrumgate-roadmap-v1/04-runtime-flow.md` |
| Constraints/invariants | PASS | `docs/ferrumgate-roadmap-v1/06-constraints-and-invariants.md` |
| Agent handoff | PASS | `docs/implementation-path/07-agent-handoff-prompt.md` |
| Phase success criteria | PASS | `docs/ferrumgate-roadmap-v1/91-phase-success-criteria-and-kpis.md` |
| Release checklist | PASS | `docs/ferrumgate-roadmap-v1/16-release-checklist.md` |
| Remaining tasks | PASS | `docs/implementation-path/11-remaining-tasks.md` |
| Current state | PASS | `docs/implementation-path/01-current-state.md` |
| Phase checklists | PASS | `docs/implementation-path/09-phase-checklists.md` |
| RC evidence doc | PASS | `docs/implementation-path/25-EV-v1-single-node-rc-evidence.md` exists (this doc) |
| Phase F final docs pack | PASS | implementation-path docs finalized as cohesive pack |

**Known gaps**: See support contract Accepted Risks and invariant matrix (`12 VERIFIED / 0 PARTIAL / 0 INFERRED`) for the current control baseline. Residual production constraints are operational rather than invariant-evidence gaps: SQLite single-node throughput limits, bounded SQLite-only backup/restore with opt-in retention pruning, PostgreSQL/multi-node deferral, and required operator signoff. No outstanding P0/P1/P2 items.

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
3. Supported flows list documented in `25-EV-v1-single-node-rc-evidence.md` Evidence 9.
4. Open gaps list documented in `11-remaining-tasks.md`.

### P2 — v1 polish
(all resolved):
1. `scripts/generate_rc_evidence.py` exists and PASS with all five checks — evidence: fresh P6 run (2026-04-28) → "Overall: ALL PASS".
2. clippy passes: `cargo clippy --workspace --all-targets -- -D warnings` PASS — evidence: fresh P6 validation.
3. `cargo test --workspace` PASS — evidence: fresh feature-completeness validation (~797 observed tests).

---

## Verdict

**FerrumGate v1 is RC-ready** for single-node SQLite-backed deployment, with conditional production posture.

All P0/P1/P2 items verified complete:
- Scope-mismatch deny implemented
- Poisoned-context fixtures curated (6 tests)
- Phase F docs pack finalized
- clippy passes with no warnings
- ~797 observed workspace tests pass (fresh feature-completeness validation 2026-04-28)
- RC evidence script present and passing

The governance loop, persistence layer, and integration test coverage are strong.
Remaining gaps are post-v1 backlog items (multi-node/HA, real adapters, PostgreSQL). U1–U4 are implemented upgrade tracks but are outside the original v1 single-node SQLite support contract.

**Production evaluation required before production deployment.** See `docs/implementation-path/27-production-evaluation-plan.md` for the full evaluation framework covering performance, security, reliability, operations, and release confidence.

**Release paths**: Three mutually exclusive post-P6 decision paths (RC tag, conditional production pilot, Phase 3 PostgreSQL) are documented in `docs/implementation-path/31-release-paths-todo.md` with detailed checklists, gates, evidence references, risks, and rollback/abort criteria.
