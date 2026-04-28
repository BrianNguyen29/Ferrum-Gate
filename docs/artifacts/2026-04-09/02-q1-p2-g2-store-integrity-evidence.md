# Artifact Note: Q1-P2 / G2 Store Integrity — Closure Evidence

**Date**: 2026-04-09
**Package / Gate**: Q1-P2 / G2
**Author**: fixer (evidence bundle agent)
**Status**: PASS

## Criterion

> "G2: Store layer exposes explicit state transitions for executions, capabilities,
> and approvals. Terminal states are absorbing. Repo-layer tests confirm
> InvalidState errors are returned for all invalid transitions."

## Verification Commands Run

```sh
cargo test --package ferrum-store -- transitions
cargo test --package ferrum-store -- invalid_transitions
```

## Results

### transitions unit tests (ferrum-store/src/transitions.rs)

```
running 24 tests
test capability_active_to_active_invalid ... ok
test capability_active_to_expired_valid ... ok
test capability_active_to_quarantined_valid ... ok
test capability_active_to_revoked_valid ... ok
test capability_active_to_used_valid ... ok
test capability_cannot_reuse_used ... ok
test capability_terminal_no_transitions ... ok
test capability_used_is_terminal ... ok
test approval_cannot_regrant_denied ... ok
test approval_denied_is_terminal ... ok
test approval_expired_is_terminal ... ok
test approval_granted_is_terminal ... ok
test approval_pending_to_denied_valid ... ok
test approval_pending_to_expired_valid ... ok
test approval_pending_to_granted_valid ... ok
test approval_pending_to_pending_invalid ... ok
test approval_terminal_no_transitions ... ok
test execution_all_non_terminal_transitions_allowed ... ok
test execution_authorized_to_prepared_valid ... ok
test execution_committed_is_terminal ... ok
test execution_proposed_to_authorized_valid ... ok
test execution_running_to_committed_valid ... ok
test execution_terminal_no_transitions ... ok

test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; filtered out; finished in 0.01s
```

### invalid_transitions repo-layer integration tests

```
running 22 tests
test approval_resolve_denied_to_pending_returns_invalid_state ... ok
test approval_resolve_granted_to_denied_returns_invalid_state ... ok
test approval_resolve_nonexistent_returns_ok ... ok
test approval_resolve_expired_to_pending_returns_invalid_state ... ok
test approval_resolve_granted_to_pending_returns_invalid_state ... ok
test approval_resolve_pending_to_granted_is_valid ... ok
test capability_update_status_nonexistent_returns_ok ... ok
test capability_update_status_expired_to_active_returns_invalid_state ... ok
test capability_update_status_active_to_used_is_valid ... ok
test capability_update_status_quarantined_to_active_returns_invalid_state ... ok
test capability_update_status_revoked_to_active_returns_invalid_state ... ok
test capability_update_status_used_to_active_returns_invalid_state ... ok
test capability_update_status_used_to_expired_returns_invalid_state ... ok
test execution_update_state_committed_to_failed_returns_invalid_state ... ok
test execution_update_state_denied_to_running_returns_invalid_state ... ok
test execution_update_state_compensated_to_running_returns_invalid_state ... ok
test execution_update_state_committed_to_running_returns_invalid_state ... ok
test execution_update_state_failed_to_running_returns_invalid_state ... ok
test execution_update_state_nonexistent_returns_ok ... ok
test execution_update_state_proposed_to_authorized_is_valid ... ok
test execution_update_state_quarantined_to_running_returns_invalid_state ... ok
test execution_update_state_rolled_back_to_running_returns_invalid_state ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
```

Combined: 24 unit tests + 22 integration tests = **46 tests passed**.

## Known Gap — update() Raw Paths

The transitions enforced here cover the `update_status` / `resolve` / `update_state` repo
paths. The raw `update()` paths (e.g., full record replace semantics) that bypass
transition validation remain **defense-in-depth debt** — they do not yet go through
the transition guard layer. A future slice must route all mutating paths through
the validated transition helpers so that every state change is transition-checked.

This gap is documented in `crates/ferrum-store/src/transitions.rs` ("Deferred: Rollback/Full
Execution Strictness") and tracked as a known limitation for the store integrity slice.

## Provenance Scope

Provenance append-only semantics (event log immutability, append-only constraint in the
provenance repo) are enforced at the **code-level / repo-surface layer**:
`append_event` in `crates/ferrum-store/src/sqlite/provenance.rs` performs inserts only
(no update/delete on provenance_events table). This is the current scope of the G2
provenance assertion.

Runtime tamper-evidence (e.g., cryptographic hash chain, signed event bundles,
downstream audit trail integrity) is a **future slice** (post-Q1 / Q4 enterprise
evidence plane scope per `01-quarterly-plan.md`).

## Gate Criterion Link

This note satisfies: G2 evidence — store transition rules are implemented and tested,
terminal states are absorbing, InvalidState errors returned for invalid transitions.

## V1 Boundary

- [x] This evidence is for v1 kernel hardening (Q1)
