# Q1-P4 Combined Closure Note — P4a (mark_used at authorize) + P4b (rollback_class propagation)

**Date:** 2026-04-09
**Package:** Q1-P4
**Status:** P4a PASS · P4b PASS · Combined Q1-P4 objective met
**supersedes:** `04-q1-p4b-prepare-rollback-class-evidence.md` (P4b detail retained in artifact log)

---

## Q1-P4 Package Objective

> Close two closely coupled defects that block Gate C:
> 1. P4a: `cap mark_used` single-use enforcement at the authorize path
> 2. P4b: `rollback_class` propagation at the prepare step

Both fixes are required before the full lineage chain test (step 1.6) can run.

---

## P4a — `mark_used` at authorize_execution

### Problem

`authorize_execution` in `server.rs:448` accepted the capability but did not immediately call `mark_used`. Single-use enforcement was deferred to execute, meaning a capability could be authorized multiple times concurrently before the mark was recorded.

### Fix

`server.rs:461–466` now calls `mark_used` immediately inside `authorize_execution`:

```rust
// server.rs:461–466
state
    .runtime
    .cap
    .mark_used(request.capability_id)
    .await
    .map_err(ApiProblem::from_capability)?;
```

The `AlreadyUsed` error is returned immediately on the second authorize attempt — no deferred enforcement.

### Verification

- `server.rs:448` — `authorize_execution` handler entry point
- `server.rs:464` — `mark_used` call site (synchronous, first-match semantics)
- `integration_gateway_flow.rs:159` — `test_single_use_capability_cannot_be_reused_via_gateway`: exercises two sequential `authorize_execution` calls through the gateway HTTP endpoint; second call returns `CapError::AlreadyUsed`

**P4a: PASS**

---

## P4b — `rollback_class` propagation at prepare

### Problem

`prepare_execution` in `server.rs:524` was hardcoding `RollbackClass::R0NativeReversible`, ignoring the `requested_rollback_class` stored in the proposal. This caused R3 proposals to incorrectly receive `auto_commit=true`.

### Fix

`server.rs:524–544` now fetches the proposal record and propagates the real `rollback_class`:

```rust
// server.rs:526–540
let proposal = state
    .runtime
    .store
    .proposals()
    .get(execution.proposal_id)
    .await
    ...
    .ok_or_else(|| ApiProblem::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound, "proposal not found"))?;
let rollback_class = proposal.requested_rollback_class.clone();

let request = state.runtime.rollback.default_prepare_request(
    execution.intent_id,
    execution.proposal_id,
    execution_id,
    rollback_class.clone(), // ← real value from proposal
);
```

### Verification

- Build: `cargo check --workspace` → `Finished dev profile` (PASS)
- Integration: `cargo test --package ferrum-integration-tests --test integration_gateway_flow` → 21 passed, 0 failed (PASS)
- R3 `auto_commit=false` confirmed respected at prepare step

**P4b: PASS**

Full evidence documented in `docs/artifacts/2026-04-09/04-q1-p4b-prepare-rollback-class-evidence.md`.

---

## Combined Q1-P4 Status

| Sub-task | Status | Key Evidence |
|---|---|---|
| P4a — mark_used at authorize | PASS | `server.rs:464`; `integration_gateway_flow.rs:159` |
| P4b — rollback_class propagation | PASS | `server.rs:540`; `integration_gateway_flow.rs` 21 tests pass; `04-q1-p4b-prepare-rollback-class-evidence.md` |

### Conservative scope note

- **Q1-P4 package objective is met** for P4a and P4b.
- Gate C (line 42 of `01-quarterly-plan.md`) requires BOTH P4a and P4b before lineage test. Both sub-tasks now pass, so Gate C criterion is satisfied on the Q1-P4 package dimension.
- This note does **not** overclaim Gate A full closure (Q1-P3/PDP slice satisfied; Q1-P4a mark_used evidenced; full closure requires remaining integration coverage per manifest.txt lines 50-52) or full Q1 exit gate closure (steps 1.6–1.8 remain open).

**Q1-P4: CLOSED** — combined closure 2026-04-09.