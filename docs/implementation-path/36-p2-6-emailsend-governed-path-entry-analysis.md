# Slice 36 — P2.6 EmailSend Governed-Path Entry Analysis

**Date:** 2026-04-04
**Type:** Preflight slice (docs + tests only; no implementation)
**Status:** IN PROGRESS (preflight entry slice)

---

## Overview

This slice performs the preflight analysis required before EmailSend can be considered for governed-path support in Ferrum-Gate. It does **not** implement EmailSend — it establishes the analytical groundwork and regression tests that must be in place before any future EmailSend implementation begins.

EmailSend is **explicitly out of scope** for v1. The current boundary is:
- Gateway: `allow_send=true` EmailDraft bindings are **denied at prepare-time** (fail-closed)
- Adapter: maildraft adapter **rejects send payloads at execute-time** (fail-closed)

This slice is the **entry gate** — governed-path entry analysis, adapter contract draft, and deny-regression tests — before P2.6 can advance to actual EmailSend implementation.

---

## Decision Context

Per `docs/implementation-path/16a-slice-16-a-boundary-ratification.md` lines 88-96, the following entry criteria were identified for future EmailSend governed-path evaluation:

1. **Send-semantics safety analysis**: Real send operations must have documented compensation
2. **Provider-level undo semantics**: Must document what "undo" means for the target email provider
3. **R3 binding extension proposal**: A separate Slice proposal for `allow_send=true` as a governed binding type
4. **Adapter implementation with send semantics**: A dedicated adapter (not maildraft) must be designed
5. **Evidence pack**: Proof that send-path governance does not introduce new R3 escalation risks

This slice addresses criteria 3 and 4 partially (contract draft) while leaving 1, 2, and 5 for future slices.

---

## Current Boundary (Ratified)

### Gateway Prepare-Time Deny

**Source:** `crates/ferrum-gateway/src/server.rs:1592-1612`

```rust
// Fail-closed: explicitly deny EmailDraft bindings with allow_send=true.
let has_send_email = capability.resource_bindings.iter().any(|b| {
    matches!(
        b,
        ResourceBinding::EmailDraft {
            allow_send: true,
            ..
        }
    )
});

if has_send_email {
    return Err(ApiProblem::new(
        StatusCode::FORBIDDEN,
        ApiErrorCode::PolicyDenied,
        "EmailDraft with allow_send=true is not supported in v1: \
         real send recovery is out of scope; use draft-only (allow_send=false) instead",
    ));
}
```

### Adapter Execute-Time Rejection

**Source:** `crates/ferrum-adapter-maildraft/src/lib.rs:269-277`

```rust
// Fail-closed: reject send payloads (send semantics out of scope)
if let Some(send) = payload.get("send").and_then(|v| v.as_bool()) {
    if send {
        return Err(AdapterError::Validation(
            "maildraft adapter: send semantics out of scope, rejecting send payload"
                .to_string(),
        ));
    }
}
```

### Two-Layer Defense

| Layer | Check | Behavior |
|-------|-------|----------|
| Gateway | Prepare-time inspects `allow_send=true` on EmailDraft bindings | Returns `403 PolicyDenied` |
| Adapter (maildraft) | Execute-time inspects `send=true` in payload | Returns `AdapterError::Validation` |

Both layers are **fail-closed**: they reject rather than silently succeed.

---

## Governed-Path Entry Requirements

### Phase 1: Preflight (COMPLETED)

- [x] Governed-path entry analysis doc (this file)
- [x] Dedicated EmailSend adapter contract draft (`docs/implementation-path/37-p2-6-emailsend-adapter-contract-draft.md`)
- [x] Deny-regression tests locking current boundary
- [x] Roadmap update: P2.6 marked as "preflight slice IN PROGRESS"
- [x] **Scaffold implementation**: `ferrum-adapter-emailsend` crate with prepare-time validation and fail-closed execute (`docs/implementation-path/38-p2-6-emailsend-adapter-scaffold-implementation.md`)

### Phase 2: Send-Semantics Safety Analysis (Future)

Required before any EmailSend implementation:

- [ ] Document email send compensation model (recall, cancel, acknowledge vs. guarantee)
- [ ] Analyze provider-level undo semantics for target email providers (SMTP, SendGrid, SES, etc.)
- [ ] Classify send operation as compensatable vs. irreversible
- [ ] Assess blast-radius if send occurs without proper authorization

### Phase 3: Adapter Contract + R3 Binding Proposal (Future)

- [ ] Dedicated `EmailSend` adapter contract (separate from maildraft)
- [ ] R3 binding extension proposal for `allow_send=true` as governed binding type
- [ ] New `ActionType::EmailSend` variant (if separating from EmailDraft)
- [ ] Provider-level send/revoke semantics definition

### Phase 4: Implementation + Evidence (Future)

- [ ] EmailSend adapter implementation with explicit send/revoke semantics
- [ ] Integration tests for send-path governance
- [ ] Evidence that send-path governance does not introduce R3 escalation risks

---

## Relationship to Other Slices

| Slice | Topic | Relationship |
|-------|-------|--------------|
| 16-A | HTTP Mutation Recovery and EmailSend Boundary Ratification | Ratified the current deny boundary; this slice is the entry analysis for future governed-path work |
| 36 (this) | EmailSend governed-path entry analysis | Preflight analysis and contract draft |
| 37 | EmailSend adapter contract draft | Adapter contract sketch (separate from maildraft) |

---

## Non-Goals (Explicit)

- EmailSend is **NOT** being implemented in this slice
- No `EmailSend` recovery or "unsend" path is being claimed
- `allow_send=true` bindings must not be routed to any adapter without explicit Slice proposal
- The maildraft adapter **does not** and **will not** support send semantics — a separate adapter is required

---

## References

| File | Role | Relevant Lines |
|------|------|----------------|
| `crates/ferrum-gateway/src/server.rs:1592` | Gateway prepare-time deny | Lines 1592-1612 |
| `crates/ferrum-adapter-maildraft/src/lib.rs:269` | Maildraft send rejection | Lines 269-277 |
| `docs/13-adapter-contracts.md` | Adapter contract docs | Lines 22-26 |
| `docs/06-constraints-and-invariants.md` | Rollback invariants | Line 25 |
| `docs/implementation-path/16a-slice-16-a-boundary-ratification.md` | EmailSend boundary ratification | Lines 57-96 |
| `docs/implementation-path/37-p2-6-emailsend-adapter-contract-draft.md` | EmailSend adapter contract draft | N/A (this slice) |

---

## Next Action

After this preflight slice completes, the following must be done before EmailSend implementation can begin:

1. Complete Phase 2 (send-semantics safety analysis) with documented evidence
2. Draft Phase 3 R3 binding extension proposal
3. Obtain explicit approval to advance from preflight to implementation

**Owner:** Orchestrator (post-review, verification, commit, PR, merge)
