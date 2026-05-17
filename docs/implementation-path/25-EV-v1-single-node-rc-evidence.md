# 25 — v1 Single-Node RC Evidence

> **Evidence document**: This file records verification evidence; it is not a feature/status authority on its own. When it conflicts with current support-contract or release-path docs, defer to those current docs.

Single-node v1 scope. This document is the canonical evidence record for the
FerrumGate v1 release candidate. It is the output of the Phase F evidence gate
from `91-phase-success-criteria-and-kpis.md` section 7.5.

---

## Evidence 1 — Workspace compiles

```
cargo check --workspace     # PASS
cargo fmt --all -- --check  # PASS
cargo clippy --workspace --all-targets -- -D warnings  # PASS
cargo test --workspace       # PASS
```

Source: Fresh P6 validation (2026-04-28): `cargo check --workspace` exit 0,
`cargo fmt --all -- --check` exit 0,
`cargo clippy --workspace --all-targets -- -D warnings` exit 0,
`cargo test --workspace` exit 0 with test counts below.

---

## Evidence 2 — Integration test suite

```
ferrum-integration-tests:
  test_single_use_capability_cannot_be_reused          # PASS
  test_r3_contracts_have_auto_commit_false             # PASS
  test_rollback_and_compensate_are_distinct_operations  # PASS
  compensate_execution_flow                              # PASS
  test_high_taint_triggers_quarantine                   # PASS
  test_scope_mismatch_deny_on_empty_scope_with_mutation # PASS
  test_r0_allowed_with_empty_scope                      # PASS
  test_pending_approvals_pagination                    # PASS
  test_pending_approvals_filtered_by_proposal_id       # PASS

  # Poisoned context regression fixtures (6 tests):
  test_poisoned_context_taint_at_boundary_69_no_quarantine   # PASS
  test_poisoned_context_r0_bypasses_taint_check              # PASS
  test_poisoned_context_taint_at_maximum_100                # PASS
  test_poisoned_context_r3_requires_approval                 # PASS
  test_poisoned_context_moderate_taint_50_no_quarantine      # PASS
  test_poisoned_context_trust_attributes_no_bypass            # PASS

integration_lineage_chain:
  # Core provenance chain tests (3):
  test_lineage_chain_minimum_provenance_events                # PASS
  test_lineage_adversarial_partial_execution_no_terminal     # PASS
  test_lineage_chain_full_provenance_events                   # PASS
  # Real adapter lineage tests (5):
  test_lineage_chain_fs_adapter_compensate                   # PASS
  test_lineage_chain_fs_adapter_full_committed                # PASS
  test_lineage_chain_sqlite_adapter_compensate               # PASS
  test_lineage_chain_maildraft_adapter_compensate             # PASS
  test_lineage_chain_git_adapter_compensate                   # PASS
  # Lineage query endpoint tests (5):
  test_lineage_query_returns_404_for_nonexistent_event       # PASS
  test_lineage_query_accepts_default_direction                # PASS
  test_lineage_query_rejects_invalid_event_id_format         # PASS
  test_lineage_query_accepts_all_directions                  # PASS
  test_lineage_query_handles_max_hops_clamping                # PASS

Source: `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`,
`crates/ferrum-integration-tests/src/integration_lineage_chain.rs`.

**P6 Fresh test counts** (2026-05-06 `cargo test --workspace`):
- adapter-fs: 146 | adapter-git: 86 | adapter-http: 103 | adapter-sqlite: 16 | adapter-maildraft: 16
- ferrum-cap: 4 | ferrum-firewall: 21 | ferrum-gateway: 41
- integration tests: 87 total = contracts(2) + fs-roundtrip(7) + gateway-flow(65) + lineage-chain(13)
- ferrumctl: 35 | ferrumd: 6 | ferrum-store: 60 | ferrum-sync: 65 | invalid_transitions: 22
- Observed total: ~797 workspace tests including integration tests and doc-tests

---

## Evidence 3 — Gateway flow coverage

The gateway orchestrates the complete flow for SQLite-backed single-node:

```
evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate
```
(commit/rollback are internal orchestration semantics; compensate is the sole exposed v1 recovery endpoint)

Supported negative paths:
- **deny**: StaticPdpEngine returns Deny for high-risk proposals
- **quarantine**: taint score >= 70 triggers Quarantine for non-R0 mutations
- **RequireApproval**: R3 (IrreversibleHighConsequence) requires approval
- **draft-only gated at evaluate**: draft-only intents gated at evaluate step (before prepare)
- **compensate**: compensate execution from prepared state (exposed v1 route)

Source: `crates/ferrum-gateway/src/`, `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`.

---

## Evidence 4 — Scope mismatch denial is IMPLEMENTED

The PDP now performs explicit scope-bounds checking. When `intent.resource_scope.is_empty()` 
AND the proposal requests a non-R0 rollback class (mutation), the PDP returns `Decision::Deny` 
with rule `"scope.mismatch.empty.scope"`. R0 actions with empty scope are still allowed 
(`Allow`) since R0 is native reversible and does not require scope.

Tests:
- `test_scope_mismatch_deny_on_empty_scope_with_mutation` — verifies Deny on empty scope + non-R0
- `test_r0_allowed_with_empty_scope` — verifies Allow on empty scope + R0

Source: `crates/ferrum-pdp/src/engine.rs` (StaticPdpEngine::evaluate), 
`crates/ferrum-integration-tests/src/integration_gateway_flow.rs`.

---

## Evidence 5 — Provenance emitted for supported flows

Lineage endpoint is implemented:
- `GET /v1/provenance/lineage/{execution_id}` — fetch lineage by execution
- `POST /v1/provenance/lineage` — query by event_id with direction (ancestors/descendants/both) and max_hops

Empty lineage returns 200 (fail-soft). Unknown event returns 404. Invalid UUID returns 400.

Source: `tests/integration_lineage_chain.rs`, `crates/ferrum-gateway/src/` routes.

---

## Evidence 6 — Persistence boundary

SQLite store persists all core objects:
- intents, proposals, capabilities, executions
- rollback contracts (with R0/R1/R2/R3 class and auto_commit flag)
- provenance events
- approvals (with pagination and proposal_id filter)

Source: `crates/ferrum-store/src/`, embedded migrations.

---

## Evidence 7 — Contracts, schemas, OpenAPI in sync

```
bash scripts/validate_repo_layout.sh       # "Repository layout looks OK"
python3 scripts/check_contract_consistency.py  # VALIDATION PASSED
```

Source: Fresh P6 validation (2026-04-28): both scripts exit 0.

---

## Evidence 8 — RC automation script

`scripts/generate_rc_evidence.py` exists and PASS with all five checks.
Uses `cargo clippy --workspace --all-targets -- -D warnings`.
Source: Fresh P6 run: `python3 scripts/generate_rc_evidence.py` → "Overall: ALL PASS".

---

## Evidence 9 — Supported flows list (Phase F gate 7.5)

The following flows are confirmed supported in single-node v1:

1. **Evaluate proposal**: POST /v1/proposals/{proposal_id}/evaluate
2. **Mint capability**: POST /v1/capabilities/mint
3. **Authorize execution**: POST /v1/executions/authorize
4. **Prepare execution**: POST /v1/executions/{execution_id}/prepare
5. **Execute** (via gateway internal call)
6. **Verify execution** (via gateway internal call)
7. ~~**Commit execution**: POST /v1/executions/{execution_id}/commit~~ — not exposed in v1 router
8. **Compensate execution**: POST /v1/executions/{execution_id}/compensate
9. ~~**Rollback execution**: POST /v1/executions/{execution_id}/rollback~~ — not exposed in v1 router
10. **Inspect execution**: GET /v1/executions/{execution_id}
11. **Inspect lineage**: GET /v1/provenance/lineage/{execution_id}
12. **Query lineage**: POST /v1/provenance/lineage
13. **List pending approvals**: GET /v1/approvals
14. **Get pending approval**: GET /v1/approvals/{approval_id}

All of the above are backed by integration tests or unit tests and persist
via SQLite. They cover the complete happy path and major negative paths.

**Implemented outside v1 support baseline** (U1–U4 upgrade tracks — post-v1 scope, not covered by v1 single-node support contract):
- Outcome-aware governance (U1) — POST /v1/executions/{id}/evaluate-outcome
- Reversible execution planner (U2) — PlannableAdapter trait + PlannableFsAdapter
- Cross-runtime provenance fabric (U3) — ExternalEventSource trait + POST /v1/provenance/ingest
- MCP/local/NemoClaw runtime integrations (U4) — RuntimeBridge trait + GET /v1/bridges + GET /v1/bridges/{id}/tools

**NOT YET SUPPORTED in v1** (post-v1 backlog):
- Broader adapter surface completion (fs: permissions, symlinks; git: remote ops; http: broader replay; maildraft: full implementation)
- Multi-node / HA / read-replica

---

## Evidence 10 — Open gaps list (Phase F gate 7.5)

| Gap | Priority | Description |
|---|---|---|
| (none) | P0 | All P0 items resolved |
| (none) | P1 | All P1 items resolved |
| (none) | P2 | All P2 items resolved |
| Real adapter implementations | P3 | filesystem/SQLite/maildraft — post-v1 backlog |
| Multi-node/HA | P3 | read-replica support — post-v1 backlog |

Full details in `docs/implementation-path/11-remaining-tasks.md`.

---

## Verdict

FerrumGate v1 single-node is **RC-ready** for SQLite-backed deployment.

All P0/P1/P2 items verified complete as of 2026-04-28 (fresh P6 validation):
- P0: scope-mismatch deny implemented in PDP
- P1: poisoned-context fixtures (6 tests), Phase F docs pack finalized, supported flows documented
- P2: clippy passes (`--all-targets`), ~797 observed workspace tests pass, RC evidence script present and passing

**P3–P5 evidence summary** (bounded, post-v1 backlog context):
- **P3 (readiness/observability)**: `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep` all return 200 with expected payloads.
- **P4 (ADR/DSN)**: ADR-50 (store DSN guardrails) verified; `ferrumd` guards unsupported DSN types.
- **P5 (backup)**: `ferrumctl backup create/verify/restore` implemented; verify runs `PRAGMA integrity_check`; restore preserves pre-restore copy and restores rows correctly.

Remaining gaps are post-v1 backlog (real adapters, multi-node/HA).

All evidence items above are grounded in actual repo content and test files.
No speculative claims have been made about multi-node, HA, or future upgrade tracks.

---

## Addendum — v0.1.0-rc.2 Evidence (2026-05-17)

This addendum records the delta between rc.1 and rc.2 without rewriting the canonical rc.1 evidence above.

### rc.2 Engineering Delta

- **MCP D1 governance beta preview** — `crates/ferrum-integrations-mcp` local coverage hardened (239 tests); D1 local drill runner present
- **Auth gate** — bearer-token auth enforced in production config mode (`auth_mode = "bearer"`)
- **Rate limiting** — configurable per-endpoint rate limiting integrated with gateway
- **Local lifecycle/load smoke** — `bash scripts/run_pre_target_gate.sh --full` passes; `bins/ferrum-stress` available for bounded local load
- **D78-8 mapping** — delivery-to-milestone traceability updated
- **Architecture/status docs** — `67-production-readiness-roadmap.md` and `122-completion-roadmap-and-hardening-tracker.md` added
- **T3 scaffolds** — Phase 3 PostgreSQL/MCP bridge scaffolds present (no functional Phase 3 claim)
- **Clippy cleanup** — resolved G1 clippy warnings with behavior-neutral cleanup in `ferrum-gateway/src/server.rs` and `ferrum-integrations-mcp/src/lib.rs`

### rc.2 Fresh G1 Pass (2026-05-17)

| Check | Status |
|-------|--------|
| `cargo check --workspace` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS (~797 tests) |
| `python3 scripts/generate_rc_evidence.py` | PASS ("Overall: ALL PASS") |
| `bash scripts/validate_repo_layout.sh` | PASS ("Repository layout looks OK") |
| `python3 scripts/check_contract_consistency.py` | PASS ("VALIDATION PASSED") |
| `bash scripts/run_pre_target_gate.sh --full` | PASS ("ALL LOCAL CHECKS PASSED") |
| `git diff --check` | PASS |

### Non-Production Declaration (rc.2)

- **NOT production-ready** — conditional single-node SQLite pilot only
- **Block A (real domain)**: BLOCKED — no real owned domain or DNS available yet
- **SendGrid API key rotation**: pending / operator-blocked
- **Live target-host MCP smoke/load**: still open; local-only validation to date
- Production deployment requires explicit operator signoff per `31-release-paths-todo.md` Path 2 gates
