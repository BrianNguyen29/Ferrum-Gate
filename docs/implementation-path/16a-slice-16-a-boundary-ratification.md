# Slice 16-A — HTTP Mutation Recovery and EmailSend Boundary Ratification

## Decision/boundary doc. ASCII only. Docs-only change.

This slice ratifies the current safety boundaries for two items previously
flagged as "evaluate before implementing" in Plan 16:

1. HTTP mutation recovery
2. EmailSend governed-path support

**This is a ratification doc, not an implementation plan.** No code changes.
No rollback-equivalent claims are being made for remote HTTP mutations.

---

## Decision 1: HTTP Mutation Recovery — R3/Manual Boundary Ratified

### Current Behavior (Ratified)

HTTP adapter rollback is a **conservative no-op** by design.

- **Source**: `crates/ferrum-adapter-http/src/lib.rs:1079-1105`
  - `rollback()` returns `RecoveryReceipt { recovered: true, metadata: { rollback: "no-op", reason: "..." } }`
  - The adapter does not attempt to reverse, undo, or compensate any remote HTTP mutation
- **Source**: `crates/ferrum-gateway/src/server.rs:2660-2676` (`infer_rollback_class`)
  - POST/PUT/PATCH/DELETE HTTP endpoints are inferred as `RollbackClass::R3IrreversibleHighConsequence`
  - R3 requires explicit human approval; no auto-commit
- **Source**: `docs/13-adapter-contracts.md:42`
  - "rollback / compensate la conservative no-op; destructive remote mutation van la explicit R3 boundary"

### What This Means

| HTTP Method | Rollback Behavior | Reason |
|-------------|-------------------|--------|
| GET | No-op (no side effects) | Safe by nature |
| POST/PUT/PATCH/DELETE | No-op (manual R3 required) | Remote destructive side effects cannot be auto-reversed |

### Non-Goals (Explicit)

- HTTP mutation recovery is **NOT** being implemented in this slice
- The rollback adapter is **NOT** claiming rollback-equivalence for remote mutations
- No future slice should use this doc as precedent for claiming automatic HTTP mutation undo
- If mutating HTTP methods need recovery in a future slice, a **new boundary analysis** is required before any implementation begins

### Entry Criteria for Future HTTP Mutation Recovery Implementation

All of the following must be true before any implementation of HTTP mutation recovery:

1. **Idempotency analysis**: The remote endpoint must provably support safe idempotent replay or explicit undo semantics
2. **Side-effect classification**: Mutation must be classified as compensatable vs. irreversible before adapter work begins
3. **Digest-boundary verification**: The `approved_request_digest` must be preserved and verifiable against replay
4. **R3 boundary extension proposal**: A separate Slice proposal documenting the extended boundary, reviewed and approved
5. **Test coverage**: Integration tests covering the recovery path with mocked remote endpoints

---

## Decision 2: EmailSend — Deny Boundary Ratified

### Current Behavior (Ratified)

`EmailDraft` bindings with `allow_send=true` are **explicitly denied** at gateway prepare-time (fail-closed).

- **Source**: `crates/ferrum-gateway/src/server.rs:1149-1169`
  - `has_send_email` check at prepare-time returns `403 Forbidden` with clear error message
  - Error: "EmailDraft with allow_send=true is not supported in v1: real send recovery is out of scope"
- **Source**: `crates/ferrum-adapter-maildraft/src/lib.rs:161-168`
  - Adapter-level defense: `execute()` rejects any payload with `send=true`
  - Error: "maildraft adapter: send semantics out of scope, rejecting send payload"
- **Source**: `docs/13-adapter-contracts.md:22`
  - "EmailSend van ngoai scope recovery / unsend trong v1"
- **Source**: `docs/06-constraints-and-invariants.md:25`
  - "EmailSend luon la R3 trong v1" (always R3 in v1)

### What This Means

| Configuration | Behavior | Notes |
|---------------|----------|-------|
| `allow_send=false` (draft-only) | Supported | Create/delete drafts, compensatable via draft deletion |
| `allow_send=true` | Denied at prepare-time | Fail-closed; no silent fallthrough to noop |

### Non-Goals (Explicit)

- EmailSend is **NOT** being elevated to a governed capability in this slice
- No `EmailSend` recovery or "unsend" path is being claimed
- `allow_send=true` bindings must not be routed to any adapter without explicit Slice proposal
- Future EmailSend support requires a separate governed-path evaluation with evidence of send-semantics safety

### Entry Criteria for Future EmailSend Governed-Path Evaluation

All of the following must be addressed before EmailSend is considered for governed-path support:

1. **Send-semantics safety analysis**: Real send operations must have documented compensation (e.g., email recall not available for most providers)
2. **Provider-level undo semantics**: Must document what "undo" means for the target email provider (if anything)
3. **R3 binding extension proposal**: A separate Slice proposal for `allow_send=true` as a governed binding type
4. **Adapter implementation with send semantics**: A dedicated adapter (not maildraft) must be designed with explicit send/revoke semantics
5. **Evidence pack**: Proof that send-path governance does not introduce new R3 escalation risks

---

## Relationship to Plan 16

Plan 16 (`docs/implementation-path/16-recovery-hardening-follow-up-execution-plan.md`)
exists as a **backlog tracking doc** for two evaluation items:

- HTTP mutation recovery boundary evaluation
- EmailSend governed-path evaluation

This Slice 16-A **resolves** both items by ratifying the current conservative
boundaries rather than implementing new capabilities. Plan 16 now references
this slice as the ratification outcome.

---

## Updated Status Table

| Item | Plan 16 Status | Slice 16-A Decision | Next Action |
|------|----------------|---------------------|-------------|
| HTTP mutation recovery | "evaluate before implementing" | Ratified: R3/manual no-op | New Slice required for any future recovery implementation |
| EmailSend governed-path | "evaluate before implementing" | Ratified: deny at prepare-time | New Slice required for any future governed-path support |

---

## References

| File | Role | Relevant Lines |
|------|------|----------------|
| `crates/ferrum-adapter-http/src/lib.rs:1079` | HTTP rollback no-op | Lines 1079-1105 |
| `crates/ferrum-gateway/src/server.rs:1149` | EmailSend deny at prepare | Lines 1149-1169 |
| `crates/ferrum-gateway/src/server.rs:2660` | R3 inference for mutating HTTP | Lines 2660-2676 |
| `crates/ferrum-adapter-maildraft/src/lib.rs:161` | Send payload rejection | Lines 161-168 |
| `crates/ferrum-proto/src/` | Protocol type definitions | Email types |
| `docs/13-adapter-contracts.md` | Adapter contract docs | Lines 22, 42 |
| `docs/06-constraints-and-invariants.md` | Rollback invariants | Line 25 |
| `docs/17-troubleshooting.md` | Operational guidance | Line 79 |
| `docs/18-phase-f-evidence-pack.md` | Open gaps tracking | Lines 145-153 |
