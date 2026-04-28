# Q1-P4b — prepare_execution rollback_class Propagation Evidence

**Date:** 2026-04-09
**Task:** Q1-P4b — propagate real `rollback_class` into `prepare_execution`
**Status:** PASS

## Problem

`prepare_execution` in `server.rs:524–529` was hardcoding `RollbackClass::R0NativeReversible`:

```rust
let request = state.runtime.rollback.default_prepare_request(
    execution.intent_id,
    execution.proposal_id,
    execution_id,
    RollbackClass::R0NativeReversible, // ← hardcoded, wrong
);
```

This caused `auto_commit` to be set to `true` (because `R0` is not irreversible) even when the proposal requested a higher-risk rollback class (R1–R3). The proposal's `requested_rollback_class` was stored in the DB but ignored at prepare time.

## Fix

`prepare_execution` now fetches the proposal record using `execution.proposal_id` and propagates the real `rollback_class`:

```rust
// server.rs:524–537
let proposal = state
    .runtime
    .store
    .proposals()
    .get(execution.proposal_id)
    .await
    .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
    .ok_or_else(|| {
        ApiProblem::new(
            StatusCode::NOT_FOUND,
            ApiErrorCode::NotFound,
            "proposal not found",
        )
    })?;
let rollback_class = proposal.requested_rollback_class.clone();

let request = state.runtime.rollback.default_prepare_request(
    execution.intent_id,
    execution.proposal_id,
    execution_id,
    rollback_class.clone(), // ← real value from proposal
);
```

## Verification

### Build check
```
cargo check --workspace → Finished `dev` profile (PASS)
```

### Integration tests
```
cargo test --package ferrum-integration-tests --test integration_gateway_flow
→ 21 passed; 0 failed (PASS)
```

Tests exercising prepare with various rollback classes (R0, R2, R3) all pass.

### Gate B criterion (line 50 of 01-quarterly-plan.md)
> "prepare-step rollback_class test passes; R3 `auto_commit=false` respected at prepare"

The fix ensures `requested_rollback_class` is used to build the `RollbackPrepareRequest`, so:
- R3 → `auto_commit=false` (correct, per `test_r3_contracts_have_auto_commit_false`)
- R0/R1/R2 → `auto_commit=true` (correct)

**Q1-P4b: PASS** — evidence recorded 2026-04-09.
