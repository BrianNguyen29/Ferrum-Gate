# ADR — I6 Approval Binding Digest Validation

> **Status**: Complete — P1+P2+P3+P5 implemented, production deferred
> **Invariant**: I6 — Approval binding matches action digest
> **Date**: 2026-04-27
> **Deciders**: Oracle recommendation

---

## 1. Context and Evidence

### Current State

- `ApprovalBinding.approved_action_digest` field exists in `crates/ferrum-proto/src/capability.rs`
- `ApprovalRequest.action_digest` field exists in `crates/ferrum-proto/src/capability.rs`
- `CapabilityLease` stores `approval_binding` in `crates/ferrum-cap/src/service.rs`
- `ferrumctl resolve-approval` exists for approval workflow
- Digest comparison does **not** exist during mint/authorize/execute
- Most tests pass `approval_binding=None`

### Gap

`approved_action_digest` is stored but never compared against the actual action/proposal digest at any enforcement point. An attacker or buggy client could mint a capability with one action digest and then execute a different action — the binding would not catch this mismatch.

### Evidence References

- `crates/ferrum-proto/src/capability.rs` — `ApprovalBinding`, `ApprovalRequest`
- `crates/ferrum-cap/src/service.rs` — `CapabilityLease` with `approval_binding`
- `crates/ferrum-store/src/sqlite/approvals.rs` — `action_digest` storage
- `crates/ferrum-gateway/src/server.rs` — `authorize_execution` (proposed enforcement point)

---

## 2. Decision

### Enforcement Location: `authorize_execution`

After capability/intent/proposal load and I5 scope validation, before `mark_capability_used_durable`.

**Rationale**: `authorize_execution` is the single choke point where all execution-relevant preconditions are checked. Placing I6 here avoids duplication (mint already does scope/intent checks) and stays consistent with the existing gateway pipeline order.

### Backwards Compatibility

When `approval_binding=None`, the I6 check is **skipped** silently.

- `approval_binding=None` is valid and means no digest binding required
- No DB schema migration needed
- Future hardening may require `Some` when `approval_mode` requires approval, but that is **out of scope for this ADR**

---

## 3. Canonical Digest Specification

### Function: `ActionProposal::canonical_action_digest`

```rust
/// Computes the canonical SHA-256 hex digest for an ActionProposal.
/// Used for I6 approval binding validation.
pub fn canonical_action_digest(proposal: &ActionProposal) -> String {
    // Fields included (v1):
    // - intent_id
    // - proposal_id
    // - tool_name
    // - server_name
    // - raw_arguments (normalized: sorted by key, recursively)
    // - expected_effect
    // - estimated_risk
    // - requested_rollback_class
    //
    // Fields excluded (v1):
    // - step_index
    // - taint_inputs
    // - metadata
    // - created_at
}
```

### Canonical JSON Construction

1. Build a JSON object with the 8 fields listed above
2. `raw_arguments` value: if is `serde_json::Value::Object`, sort keys recursively and serialize
3. Serialize the entire object with `serde_json::to_string` using canonical ordering
4. Compute `SHA-256` over the UTF-8 bytes of the serialized string
5. Return lowercase hex

### Fields Included (v1)

| Field | Type | Notes |
|---|---|---|
| `intent_id` | `IntentId` | String |
| `proposal_id` | `ProposalId` | String |
| `tool_name` | `ToolName` | String |
| `server_name` | `ServerName` | String |
| `raw_arguments` | `serde_json::Value` | Sorted by key recursively |
| `expected_effect` | `EffectType` | Enum variant name |
| `estimated_risk` | `RiskScore` | Integer or enum |
| `requested_rollback_class` | `RollbackClass` | Enum variant name |

### Fields Excluded (v1)

- `step_index` — execution ordering detail, not part of approval contract
- `taint_inputs` — runtime evaluation, not pre-approval
- `metadata` — auxiliary, not part of approval contract
- `created_at` — timestamp, not stable across re-proposals

---

## 4. Enforcement Checks (when `approval_binding=Some`)

In `authorize_execution`, after I5 scope validation:

1. **Approval exists** — fetch `ApprovalRequest` by `approval_binding.approval_id`
2. **State is `Granted`** — reject if not `Granted`
3. **Not expired** — check `expires_at` vs `Utc::now()`; reject if expired
4. **Binding digest matches approval digest** — `binding.approved_action_digest == approval.action_digest`
5. **Computed digest matches binding** — `canonical_action_digest(proposal) == binding.approved_action_digest`

On any failure: return HTTP 403 with `Forbidden` reason code.

---

## 5. Alternatives Considered

| Option | Rejected because |
|---|---|
| Enforce at `mint_capability` | Duplicates scope/intent checks already at authorize; wrong lifecycle point |
| Enforce at both mint and authorize | Unnecessary duplication |
| Separate endpoint (e.g., `POST /v1/approvals/validate-digest`) | Adds latency without benefit; authorize_execution is the natural choke point |
| Enforce at `execute` | Too late; side effects may have started |
| Enforce in `ferrum-cap` service layer | Gateway pipeline order (I5 → I6 → mark_used) requires gateway-level check |

---

## 6. Non-Goals

- No UI or human-in-the-loop approval flow changes
- No two-phase commit
- No DB schema change (fields already exist)
- No mandatory `approval_binding` for all capabilities (None remains valid)
- Not production-ready — implementation and tests pending

---

## 7. Test Plan

### Unit Tests

1. **Digest determinism**: Same `ActionProposal` → same digest across serialization calls
2. **Sorted `raw_arguments`**: Out-of-order keys produce same digest
3. **Field difference changes hash**: Changing any included field produces different digest
4. **Excluded fields don't affect digest**: Mutating excluded fields (step_index, taint_inputs, metadata, created_at) leaves digest unchanged

### Integration Tests

1. **None binding → success**: `authorize_execution` with `approval_binding=None` passes
2. **Valid binding → success**: `approval_binding=Some` with matching digest passes
3. **Pending approval → 403**: State=`Pending` → rejected
4. **Expired approval → 403**: `expires_at` in past → rejected
5. **Approval not found → 403**: Invalid `approval_id` → rejected
6. **Digest mismatch → 403**: Computed ≠ binding → rejected
7. **Chain broken → 403**: Binding links to wrong approval → rejected
8. **Single-use still works**: Capability is marked used after successful authorized execution

---

## 8. Implementation Phases

| Phase | Deliverable | Status |
|---|---|---|
| **P1** | `canonical_action_digest()` function + unit tests (determinism, key sorting, field inclusion/exclusion) | Complete — implemented in `crates/ferrum-proto/src/execution.rs`; `cargo test -p ferrum-proto -- digest` passed |
| **P2** | Gateway I6 validation in `authorize_execution` (I5 → I6 → mark_used) | **Complete** — implemented in `crates/ferrum-gateway/src/server.rs`; `validate_approval_binding_digest` helper added; wired after I5 scope validation; 8 integration tests pass (see P3) |
| **P3** | Integration tests: None/success/mismatch/pending/expired/not-found/chain-broken/single-use | **Complete** — 8 tests implemented in `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`: None skip, valid binding success, pending denial, digest mismatch denial, expired binding denial, approval not found denial, chain-broken digest mismatch between approval and binding, single-use with valid approval binding |
| **P4** | Optional `ferrumctl` support for digest inspection | Optional — not required for invariant verification; optional future enhancement |
| **P5** | Update I6 status to VERIFIED in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` + update this ADR | **Complete** — I6 marked VERIFIED with 12/0/0; production claim remains deferred |

**Note**: P5 gate passed — I6 is now VERIFIED. Production deployment still deferred per project policy.

---

## 9. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-04-27 | Enforce at `authorize_execution` | Oracle recommendation; consistent with I5 location; avoids duplication |
| 2026-04-27 | `canonical_action_digest` excludes step_index, taint_inputs, metadata, created_at | These fields are runtime/ordering details, not part of approval contract |
| 2026-04-27 | `approval_binding=None` skips check | Backwards compatibility; avoids breaking existing capabilities |
| 2026-04-27 | SHA-256 over deterministic JSON | Consistent, widely available, sufficient for digest |
| 2026-04-27 | 5 enforcement checks (exists, Granted, not expired, binding==approval, computed==binding) | Oracle recommendation; covers all failure modes |
| 2026-04-27 | P2 implemented: `validate_approval_binding_digest` helper in server.rs | Checks approval exists, state Granted, binding/approval not expired, digest chain valid; 6 integration tests pass |
| 2026-04-27 | P3 complete: 8 integration tests pass | chain-broken and single-use tests added; all I6 tests verified |
| 2026-04-27 | P5 complete: I6 marked VERIFIED | 12/0/0 invariant counts; production deployment remains deferred |

---

## 10. Handoff

**Next implementation step**: None — I6 invariant verification complete; P4 ferrumctl support is optional and not required for invariant verification

**Owner**: Fixer (Rust implementation)

**Start condition**: Oracle approved ADR

**Production claim**: **Not ready** — I6 implementation verified; production deployment still deferred per project policy

---

## 11. References

- [45-current-feature-audit.md](./45-current-feature-audit.md) — I6 status context
- [26-EV-v1-single-node-invariant-control-test-evidence-matrix.md](./26-EV-v1-single-node-invariant-control-test-evidence-matrix.md) — Invariant matrix
- `crates/ferrum-proto/src/capability.rs` — `ApprovalBinding`, `ApprovalRequest` structs
- `crates/ferrum-gateway/src/server.rs` — `authorize_execution` (enforcement point)
- `crates/ferrum-cap/src/service.rs` — `CapabilityLease` with `approval_binding`
