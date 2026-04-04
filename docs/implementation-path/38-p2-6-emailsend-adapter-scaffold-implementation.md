# Slice 38 — P2.6 EmailSend Adapter Scaffold (Initial Implementation Slice)

**Date:** 2026-04-04
**Type:** Implementation slice (scaffold only; no actual send)
**Status:** IMPLEMENTED (scaffold only — no provider integration)

---

## Overview

This slice implements the **initial scaffold** for a dedicated `EmailSend` adapter, per the governed-path entry requirements identified in Slice 36 and the adapter contract draft in Slice 37.

**Scope of this slice:**
- Dedicated `ferrum-adapter-emailsend` crate (scaffold only)
- Prepare-time validation: `EmailSend` action type enforcement and `auto_commit=false` R3 enforcement
- Fail-closed `execute()`: returns clear validation error (no actual send)
- No-op `verify()`, `compensate()`, `rollback()` semantics (consistent with R3)
- Unit tests for all scaffold behaviors

**Out of scope for this slice (and for v1):**
- Provider integration
- Actual email send behavior
- Weakening the current gateway deny boundary (`allow_send=true`)

---

## Slice 38 Addendum — Mock Provider Foundation (2026-04-04)

This section records the **mock-provider foundation slice** added to the scaffold adapter. The adapter execute remains fail-closed; no real send behavior is enabled.

**What was added:**
- `EmailProvider` trait: provider-agnostic `send()`, `can_revoke()`, `revoke()` interface
- `ProviderSendResult` and `ProviderError` types (error categories: Transient, Permanent, Auth, Network)
- `MockEmailProvider` struct with configurable success/failure and call tracking
- 14 unit tests for mock provider (success, failure modes, call tracking, error display)
- Updated scaffold doc-tests and module docs

**What was NOT added (preserved from scaffold):**
- Adapter execute remains fail-closed (still returns "not implemented" error)
- No real provider integration (mock only; SMTP/API client TBD)
- Gateway deny boundary unchanged

**Test coverage added (14 new tests):**
- `test_mock_provider_send_success`
- `test_mock_provider_send_tracks_calls`
- `test_mock_provider_send_failure_transient`
- `test_mock_provider_send_failure_permanent`
- `test_mock_provider_send_failure_auth`
- `test_mock_provider_send_failure_network`
- `test_mock_provider_can_revoke_returns_false_by_default`
- `test_mock_provider_can_revoke_returns_true_when_enabled`
- `test_mock_provider_revoke_returns_error_by_default`
- `test_mock_provider_revoke_succeeds_when_enabled`
- `test_mock_provider_reset_clears_state`
- `test_mock_provider_unique_message_ids_per_send`
- `test_mock_provider_error_display`
- `test_mock_provider_clone_is_independent`

**Verification:** `cargo test -p ferrum-adapter-emailsend` (23 tests pass); `cargo check -p ferrum-gateway` (clean)

---

## Slice 39 — P2.6 Provider-Injection Structural Slice (2026-04-04)

This section records the **provider-injection structural slice** for the EmailSend adapter. The adapter now owns injected provider state via constructors, following the `MaildraftAdapter::with_store` pattern. Execute remains fail-closed; no real send is wired.

**What was added:**
- `provider: Arc<dyn EmailProvider>` field added to `EmailSendAdapter` struct
- `new()` now creates adapter with `MockEmailProvider::new()` as default provider
- `with_key(key)` now creates adapter with `MockEmailProvider::new()` as default provider
- `with_provider(key, provider)` constructor for dependency injection of any `EmailProvider`
- `provider()` accessor for test inspection of stored provider
- Manual `Debug` and `Clone` implementations (required because `dyn EmailProvider` doesn't derive Debug)
- 5 new unit tests proving provider storage and execute-fail-closed invariant

**What was NOT added (preserved invariant):**
- Adapter execute remains fail-closed (execute does NOT call provider.send())
- No real provider integration (provider stored but not invoked in execute)
- Gateway deny boundary unchanged (`allow_send=true` still denied at prepare-time)
- No send/revoke semantics wired to execute

**New test coverage (5 tests):**
- `test_new_adapter_has_mock_provider` — verifies default adapter has accessible mock provider
- `test_with_provider_stores_provider` — verifies injected provider is stored via Arc::ptr_eq
- `test_with_provider_can_use_failure_configured_provider` — verifies failing provider is usable via accessor
- `test_execute_still_fails_closed_with_injected_provider` — proves execute returns "not implemented" error even when provider is injected
- `test_with_provider_revoke_supported` — verifies revoke-supported provider is stored and accessible

**Structural invariant (preserved):**
```rust
// Provider injection ≠ send wiring. Execute remains fail-closed:
async fn execute(...) -> Result<ExecuteReceipt, AdapterError> {
    Err(AdapterError::Validation(
        "EmailSend adapter: execute not implemented (scaffold only). \
         Real send requires provider integration and R3 safety analysis."
            .to_string(),
    ))
}
```

**Test output:** `cargo test -p ferrum-adapter-emailsend` — 28 tests pass (23 prior + 5 new)

**Verification:** `cargo test -p ferrum-adapter-emailsend` (28 tests pass); `cargo check -p ferrum-gateway` (clean)

---

## Current Boundary (Preserved)

### Gateway Prepare-Time Deny

**Source:** `crates/ferrum-gateway/src/server.rs:1592-1612`

This boundary **remains intact**. The gateway continues to deny `allow_send=true` EmailDraft bindings at prepare-time.

### Adapter Execute-Time Fail-Closed

**Source:** `crates/ferrum-adapter-emailsend/src/lib.rs`

```rust
async fn execute(
    &self,
    _contract: &RollbackContract,
    _payload: &serde_json::Value,
) -> Result<ExecuteReceipt, AdapterError> {
    Err(AdapterError::Validation(
        "EmailSend adapter: execute not implemented (scaffold only). \
         Real send requires provider integration and R3 safety analysis."
            .to_string(),
    ))
}
```

---

## What Was Implemented

### New Crate: `ferrum-adapter-emailsend`

| File | Purpose |
|------|---------|
| `Cargo.toml` | Crate manifest with workspace dependencies |
| `src/lib.rs` | Scaffold adapter implementation with unit tests |

### Adapter Contract

| Method | Behavior |
|--------|----------|
| `key()` | Returns `"emailsend"` |
| `prepare()` | Validates `ActionType::EmailSend` and `auto_commit=false` |
| `execute()` | **Fail-closed**: returns validation error |
| `verify()` | No-op: returns `verified=true` |
| `compensate()` | No-op: returns `recovered=false` with R3 reason |
| `rollback()` | Same as compensate |

### Test Coverage

9 unit tests covering:
- `prepare()` accepts EmailSend with `auto_commit=false`
- `prepare()` rejects `auto_commit=true` (R3 enforcement)
- `prepare()` rejects non-EmailSend action types
- `execute()` fails closed with clear validation error
- `verify()` returns true (no-op)
- `compensate()` returns `recovered=false` with R3 reason
- `rollback()` delegates to compensate
- `key()` returns correct adapter key
- Custom key support

---

## Implementation Notes

1. **No real provider integration**: The adapter now includes a provider abstraction and mock provider for unit testing, but it still has no SMTP/API client or real send wiring. Real provider integration remains separate Phase 2-3 work.

2. **Fail-closed execute**: The execute method returns a clear validation error rather than silently succeeding. This preserves the security boundary.

3. **R3 consistency**: `compensate()` and `rollback()` return `recovered=false` with a documented reason, consistent with the R3 (irreversible) classification.

4. **Gateway boundary preserved**: The gateway's deny for `allow_send=true` at server.rs:1592-1612 remains unchanged. This adapter scaffold does not weaken that boundary.

---

## Relationship to Other Slices

| Slice | Topic | Relationship |
|-------|-------|--------------|
| 36 | EmailSend governed-path entry analysis | Identified entry requirements; this slice fulfills the initial scaffold requirement |
| 37 | EmailSend adapter contract draft | Contract draft; this slice implements the scaffold portion only |
| 16-A | HTTP Mutation Recovery and EmailSend Boundary Ratification | Ratified the current deny boundary; this slice preserves it |

---

## Non-Goals (Explicit)

- EmailSend **is NOT** being fully implemented in this slice
- No `EmailSend` recovery or "unsend" path is being claimed
- Provider integration is **not** included
- The gateway deny boundary for `allow_send=true` is **not** weakened

---

## References

| File | Role | Relevant Lines |
|------|------|----------------|
| `crates/ferrum-adapter-emailsend/src/lib.rs` | Scaffold adapter | All |
| `crates/ferrum-gateway/src/server.rs:1592` | Gateway prepare-time deny | Lines 1592-1612 |
| `crates/ferrum-adapter-maildraft/src/lib.rs:269` | Maildraft send rejection | Lines 269-277 |
| `docs/implementation-path/36-p2-6-emailsend-governed-path-entry-analysis.md` | Entry requirements | All |
| `docs/implementation-path/37-p2-6-emailsend-adapter-contract-draft.md` | Adapter contract draft | All |

---

## Next Steps (Post-Scaffold)

1. Phase 2: Send-semantics safety analysis with documented evidence
2. Phase 3: R3 binding extension proposal for `allow_send=true` as governed binding type
3. Provider-level send/revoke semantics definition
4. Actual EmailSend adapter implementation with provider integration

**Owner:** Orchestrator (post-review, verification, commit, PR, merge)
