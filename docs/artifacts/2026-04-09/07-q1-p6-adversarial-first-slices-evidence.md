# Q1-P6 — Adversarial First Slices Evidence

**Date:** 2026-04-09
**Package:** Q1-P6
**Status:** SATISFIED (adversarial first slices / first-pass suite)
**Evidence type:** Integration tests — WS1/WS2/WS3/WS4 adversarial regression

---

## Q1-P6 Package Objective

> Build adversarial test cases that attempt to bypass each of the four weak spots
> closed in Q1. Each bypass attempt must be stopped by the hardening done in P1–P5.
>
> Scope of this note: **adversarial first slices** — first-pass suite covering WS1–WS4
> with conservative wording. Full Q1 exit gate (Q1-P7) is not claimed.

---

## Weak Spot Coverage

| Weak Spot | Description | Adversarial Test | Location | Pass Criterion |
|-----------|-------------|------------------|----------|---------------|
| WS1 | Prepare-step rollback_class bypass (R3 no-auto-commit) | `test_r3_contracts_have_auto_commit_false` | `integration_gateway_flow.rs:370` | R3 contract rollback_class propagated; `auto_commit=false` preserved at prepare |
| WS2 | Single-use capability reuse after authorize | `test_authorize_can_only_be_called_once` | `integration_gateway_flow.rs:285` | Second authorize returns 409 Conflict; capability stays in `Used` state |
| WS3 | Draft-only intent bypass of evaluate/prepare gate | `test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate` | `integration_gateway_flow.rs:1810` | Prepare returns 403 PolicyDenied for DraftOnly intent bypassing evaluate |
| WS4 | Lineage minimum chain incomplete reporting | `test_lineage_adversarial_partial_execution_no_terminal` | `integration_lineage_chain.rs:356` | Partial flow (authorize+prepare without terminal) produces exactly 2 events; no terminal-present event in lineage |

---

## Code Evidence

### WS1 — R3 rollback_class propagation and auto_commit preservation

**Test:** `test_r3_contracts_have_auto_commit_false` (`integration_gateway_flow.rs:370`)

```rust
// Create a proposal with R3 rollback class (the key adversarial input)
let proposal = make_test_proposal_with_class(
    intent_id,
    proposal_id,
    RollbackClass::R3IrreversibleHighConsequence,
);

// Assertions: prove no downgrade from proposal-sourced rollback_class
assert_eq!(
    contract.rollback_class,
    RollbackClass::R3IrreversibleHighConsequence,
    "WS1 FAILED: prepared contract rollback_class was not R3IrreversibleHighConsequence"
);
assert!(
    !contract.auto_commit,
    "WS1 FAILED: R3 contract must have auto_commit=false"
);
```

**Key guard point:** `server.rs:540` — rollback_class propagated at prepare from proposal-sourced value, not hardcoded default.

### WS2 — Single-use capability enforcement at authorize

**Test:** `test_authorize_can_only_be_called_once` (`integration_gateway_flow.rs:285`)

```rust
// Second authorize attempt with same capability
let response2 = tower::ServiceExt::oneshot(router.clone(), request2).await;
assert_eq!(response2.status(), axum::http::StatusCode::CONFLICT);

// Verify capability remains in Used state after failed reuse attempt
let cap_lease = cap.get(capability_id).await.expect("...");
assert!(
    matches!(cap_lease.status, ferrum_proto::CapabilityStatus::Used),
    "capability status should remain Used after failed reuse"
);
```

**Key guard point:** `server.rs:464` — `mark_used` called at authorize; capability cannot be reused.

### WS3 — Draft-only intent bypass blocked at prepare

**Test:** `test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate` (`integration_gateway_flow.rs:1810`)

```rust
// Step 5: Prepare on DraftOnly intent bypassing evaluate — should REJECT
let response = tower::ServiceExt::oneshot(router, request).await;
assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
let error_response: ferrum_proto::ApiError = serde_json::from_slice(&body).expect(...);
assert!(matches!(error_response.code, ferrum_proto::ApiErrorCode::PolicyDenied));
```

**Attack scenario:** DraftOnly intent created directly in store → proposal created directly → execution record created directly (bypassing evaluate and authorize) → prepare called. Expected: prepare rejects with 403 PolicyDenied before attempting preparation.

**Key guard point:** `server.rs:275` — draft-only approval_mode propagation fix; prepare rechecks intent approval_mode.

### WS4 — Lineage partial flow does not masquerade as complete chain

**Test:** `test_lineage_adversarial_partial_execution_no_terminal` (`integration_lineage_chain.rs:356`)

```rust
// ADVERSARIAL ASSERTIONS: partial flow must NOT appear as complete chain
assert_eq!(
    lineage.events.len(),
    2,
    "partial flow (authorize+prepare without terminal) should produce exactly 2 events"
);

// CRITICAL ADVERSARIAL CHECKS: no terminal-present events must exist
assert!(
    !has_terminal_compensated,
    "ADVERSARIAL CHECK FAILED: SideEffectCompensated must NOT appear in partial flow lineage"
);
assert!(
    !has_terminal_rolled_back,
    "ADVERSARIAL CHECK FAILED: SideEffectRolledBack must NOT appear in partial flow lineage"
);
```

**Key guard point:** `server.rs:496,621,748` — provenance events emitted at authorize/prepare/compensate; lineage query returns only actual events.

---

## Test Run Evidence

```
cargo test -p ferrum-integration-tests --test integration --
```

Adversarial first-slice tests pass:

| Test | Result |
|------|--------|
| `test_r3_contracts_have_auto_commit_false` | PASS |
| `test_authorize_can_only_be_called_once` | PASS |
| `test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate` | PASS |
| `test_lineage_adversarial_partial_execution_no_terminal` | PASS |

All WS1–WS4 adversarial regression tests pass on first-slice implementation.

---

## Scope Limitation — Conservative Wording

**Q1-P6 adversarial first slices: PASS** — WS1/WS2/WS3/WS4 each have at least one
passing adversarial regression test that confirms the bypass is blocked.

**Q1-P6 does NOT claim:**
- Full Q1 exit gate closure (requires Q1-P7 invariant matrix pass)
- Complete adversarial suite coverage (additional edge cases may exist)
- P7 completion

**Q1-P6 scope:** adversarial first slices / first-pass suite only. The four weak spots
each have a first adversarial regression test passing. Further adversarial expansion
is Q1-P7 / exit gate work.

---

## Summary

| Criterion | Status | Evidence |
|-----------|--------|----------|
| WS1 adversarial regression (R3 rollback_class propagation) | PASS | `integration_gateway_flow.rs:370`; `server.rs:540` |
| WS2 adversarial regression (capability reuse blocked) | PASS | `integration_gateway_flow.rs:285`; `server.rs:464` |
| WS3 adversarial regression (draft-only bypass blocked) | PASS | `integration_gateway_flow.rs:1810`; `server.rs:275` |
| WS4 adversarial regression (lineage partial flow verified) | PASS | `integration_lineage_chain.rs:356` |
| Adversarial first slices (WS1–WS4) | PASS | 4/4 tests pass |

**Q1-P6: SATISFIED (adversarial first slices)** — Q1-P7 (invariant matrix pass / exit gate)
remains open.
