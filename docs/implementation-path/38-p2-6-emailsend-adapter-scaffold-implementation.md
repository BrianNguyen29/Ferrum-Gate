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

1. **No provider integration**: The adapter does not include any email provider abstraction or SMTP/API client. This is intentional — provider integration requires separate Phase 2-3 work.

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
