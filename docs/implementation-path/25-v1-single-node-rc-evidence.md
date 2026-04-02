# 25 — v1 Single-Node RC Evidence

Single-node v1 scope. This document is the canonical evidence record for the
FerrumGate v1 release candidate. It is the output of the Phase F evidence gate
from `91-phase-success-criteria-and-kpis.md` section 7.5.

---

## Evidence 1 — Workspace compiles

```
cargo check --workspace     # PASS
cargo fmt --all --check      # PASS
cargo clippy --workspace -- -D warnings  # PASS
cargo test --workspace       # PASS
```

Source: `docs/artifacts/2026-03-30/01-cargo-check.txt`,
`docs/artifacts/2026-03-30/02-cargo-fmt.txt`,
`docs/artifacts/2026-03-30/03-cargo-clippy.txt`,
`docs/artifacts/2026-03-30/04-cargo-test.txt`.

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
  test_lineage_endpoint_returns_empty_for_unknown_execution  # PASS
  test_lineage_endpoint_rejects_invalid_uuid                # PASS
  test_lineage_endpoint_returns_correct_content_type         # PASS
  test_lineage_query_returns_404_for_nonexistent_event       # PASS
  test_lineage_query_accepts_default_direction               # PASS
  test_lineage_query_rejects_invalid_event_id_format         # PASS
  test_lineage_query_accepts_all_directions                  # PASS
  test_lineage_query_handles_max_hops_clamping               # PASS
```

Source: `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`,
`tests/integration_lineage_chain.rs`.

---

## Evidence 3 — Gateway flow coverage

The gateway orchestrates the complete flow for SQLite-backed single-node:

```
evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate
```
(compensate is the primary recovery endpoint; commit and rollback routes are also exposed)

**Regression coverage for prepare rollback class**: `integration_gateway_flow.rs:527-792`
verifies that the prepare path loads the persisted rollback class and `auto_commit=false`
from the store (not a hardcoded R0). This regression test confirms R3 intents
retain their elevated rollback class through the prepare step post-approval.

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
python3 scripts/check_contract_consistency.py  # VALIDATION PASSED
```

Source: `docs/artifacts/2026-03-30/05-contract-consistency.txt`,
`.github/workflows/ci.yml` step "Check contract consistency".

---

## Evidence 8 — RC automation script

`scripts/generate_rc_evidence.py` exists and PASS with all five checks.
Source: `docs/artifacts/2026-03-30/07-rc-evidence-script.txt`.

---

## Evidence 9 — Supported flows list (Phase F gate 7.5)

The following flows are confirmed supported in single-node v1:

1. **Evaluate proposal**: POST /v1/proposals/{server_name}/evaluate
2. **Mint capability**: POST /v1/capabilities/mint
3. **Authorize execution**: POST /v1/executions/authorize
4. **Prepare execution**: POST /v1/executions/{execution_id}/prepare
5. **Execute** (via gateway internal call)
6. **Verify execution** (via gateway internal call)
7. **Commit execution**: POST /v1/executions/{execution_id}/commit
8. **Compensate execution**: POST /v1/executions/{execution_id}/compensate
9. **Rollback execution**: POST /v1/executions/{execution_id}/rollback
10. **Inspect execution**: GET /v1/executions/{execution_id}
11. **Inspect lineage**: GET /v1/provenance/lineage/{execution_id}
12. **Query lineage**: POST /v1/provenance/lineage
13. **List pending approvals**: GET /v1/approvals
14. **Get pending approval**: GET /v1/approvals/{approval_id}

All of the above are backed by integration tests or unit tests and persist
via SQLite. They cover the complete happy path and major negative paths.

**Materially mature — supported for current scope** (U1):
- U1-S2 (verify-time annotate-only assessment): DONE — assessment persisted in execution.metadata, rollback contract metadata, and SideEffectVerified provenance event metadata
- U1-S3a (multi-signal inference): DONE — rollback_target (HIGH), adapter_key (MED), expected_effect (LOW) inference hierarchy
- U1-S3b (confidence-thresholded verify annotations): DONE — threshold_metadata with high/medium/low bands; annotate-only semantics preserved
- U1-S4a (higher-fidelity outcome contracts): DONE — additive OutcomeSelectors enrich OutcomeClause; selector-enhanced match/mismatch distinction
- U1-S4b (explicit HIGH/MED mismatch fixtures): DONE — HIGH band mismatch via allowed_outcome non-match; MED band mismatch via adapter_key inference; selector-enhanced mismatch proven
- U1-S5a (soft gate preview): DONE — prepare-time `would_block`/`would_require_review`/`reason_codes`/`derive_basis` signals emitted; auto-commit unchanged per R3
- U1-S5b (hard gate): DONE — prepare-time blocks when would_block=true; state-machine halts to Denied; auditability via ErrorRaised event and u1_s5b_hard_gate metadata
- U1-S6 (selector-aware effective match): DONE — selector-bearing clauses require effect_type AND selectors to match for effective match
- U1-S7a (list-based selector matching): DONE — `adapter_family_in`, `target_family_in`, `request_class_in`, `mutation_family_in` fields with OR semantics
- U1-S8a (operator compile-time ergonomics): DONE — compile endpoint accepts optional `allowed_outcomes`/`forbidden_outcomes` via existing OutcomeClause/OutcomeSelectors schema; fail-closed validation; backward-compatible omission behavior

Remaining U1 backlog (not core capability gaps):
- Richer outcome clause expressiveness (nested selectors, temporal constraints)
- Policy bundle versioning and migration tooling

**NOT YET SUPPORTED in v1** (post-v1 backlog):
- Real filesystem/SQLite/maildraft adapter implementations (bounded local implementations exist; broader hardening deferred)
- Multi-node / HA / read-replica
- U2 (Reversible Execution Planner)
- U3 (Cross-runtime Provenance Fabric)
- U4 (MCP/local/NemoClaw runtime integrations)

---

## Evidence 10 — Open gaps list (Phase F gate 7.5)

| Gap | Priority | Description |
|---|---|---|
| (none) | P0 | All P0 items resolved |
| (none) | P1 | All P1 items resolved |
| (none) | P2 | All P2 items resolved |
| Real adapter implementations | P3 | filesystem/SQLite/maildraft — post-v1 backlog |
| Multi-node/HA | P3 | read-replica support — post-v1 backlog |
| Upgrade track U1 — core capability | P3 | Materially mature — remaining: richer expressiveness / operator ergonomics (not core gaps) |
| Upgrade tracks U2-U4 | P3 | post-v1 backlog |

Full details in `docs/implementation-path/11-remaining-tasks.md`.

---

## Verdict

FerrumGate v1 single-node is **RC-ready** for SQLite-backed deployment.

All P0/P1/P2 items verified complete as of 2026-03-30 (see `docs/artifacts/2026-03-30/`):
- P0: scope-mismatch deny implemented in PDP
- P1: poisoned-context fixtures (6 tests), Phase F docs pack finalized, supported flows documented
- P2: clippy passes, 128 tests pass, RC evidence script present and passing

Remaining gaps are post-v1 backlog (real adapters, multi-node/HA, U2-U4 upgrade tracks). U1 core capability is materially mature; remaining U1 work is richer expressiveness and operator ergonomics (not core gaps).

All evidence items above are grounded in actual repo content and test files.
No speculative claims have been made about multi-node, HA, or future upgrade
tracks.
