# 79 — MCP Server D-1.3.3 Preflight Packet

> **Status**: Preflight only. D-1.3.3 is the first side-effecting gate (REST wiring, HTTP calls, state changes). This packet does NOT implement D-1.3.3 — it only captures the preflight decisions needed before implementation may begin.
> **Non-Claim**: This packet does not approve D-1.3.3 implementation, does not enable any REST calls, and does not create any production/G2/operator evidence claim.

---

## 0. Executive Summary

D-1.3.3 is the boundary between pure/draft mapping (D-1.3.2a/b) and side-effecting REST wiring. Before any `.send()` calls, HTTP client usage, or governance endpoint invocation, four preflight items (P1-P4) must be resolved by an oracle/reviewer and recorded here.

This packet provides the preflight checklist. Until P1-P4 are approved, D-1.3.3 implementation is **BLOCKED**.

---

## 1. D1.3.3 Scope (Side-Effecting Gate)

### 1.1 What D1.3.3 Includes

D1.3.3 is the first slice that may include:
- Real `reqwest` HTTP client calls to gateway endpoints
- `ferrum_gateway_client::FerrumGatewayClient` instantiation and usage
- `POST /v1/intents/compile` with real (non-deprecated) DTOs
- `POST /v1/proposals/{proposal_id}/evaluate` call with client-generated proposal_id
- `Option<ProposalId>` threading through the MCP tool flow
- Any state changes (even in-memory)

### 1.2 What D1.3.3 Forbidden Items Remain

| Item | Status | Reason |
| --- | --- | --- |
| Capability minting | Blocked | Later governance stage |
| Rollback preparation | Blocked | Later governance stage |
| Provenance emission | Blocked | Gateway emits at authorize |
| Approval/reject execution | Blocked | Backend endpoint absent |
| Production readiness claim | Blocked | RC-ready only, not production |

---

## 2. Preflight Blockers (P1-P4)

### P1 — Stable Principal Identity Mapping

**Issue**: `DraftIntentCompileRequestParts.principal_id` currently uses `PrincipalId::new()` (auto-generated UUID). Before D1.3.3 REST wiring, we need a stable mapping from MCP `actor_id` to a gateway `PrincipalId`.

**Current State**:
- `principal_id: Ok(PrincipalId::new())` — draft only, not an identity claim
- `actor_id` comes from `ActorIdentity::resolve()` which MCP tool callers provide
- No stable `actor_id → PrincipalId` mapping exists

**Decision Required**:
1. Use UUID v5 (namespace-based) derived from `actor_id` as stable principal for D1.3.3?
2. Or continue using generated principal with explicit non-auth documentation?
3. Or require gateway to accept any principal and defer stable mapping to later?

**Recommendation**: Use UUID v5 derived from `actor_id` as a stable-but-not-authenticated principal identifier for D1.3.3. Mark as "stable for routing only, not an authentication claim."

**Dependencies**: None (can be resolved independently)

---

### P2 — Raw Inputs Untrusted IntentInputRef Policy

**Issue**: `raw_inputs: Err(Todo{...})` is a blocker. The `IntentInputRef` type includes fields for source identity, trust labels, and provenance — none of which MCP tool parameters can honestly provide.

**Current State**:
- `raw_inputs: Err(MappingError::Todo{...})` in `DraftIntentCompileRequestParts`
- No policy for how to handle untrusted MCP-derived inputs

**Decision Required**:
1. Create minimal `IntentInputRef` with `source: "mcp"` and `trust_level: Untrusted`?
2. Or require explicit gateway policy before any raw_inputs conversion?
3. Or remain blocked until a formal untrusted-input policy is written?

**Recommendation**: Implement minimal MCP-derived `IntentInputRef` with explicit `source: "mcp"` and `trust_level: Untrusted` labels for D1.3.3. Do not fabricate trust/provenance. Document the limitation clearly.

**Dependencies**: None (can be resolved independently)

---

### P3 — Real IntentCompileResponse DTO Replacement

**Issue**: `stage2_types::IntentCompileResponse` is deprecated and claims `proposal_id` is returned from compile. Real gateway response is `{ envelope, warnings }`.

**Current State**:
- `IntentCompileResponse { proposal_id }` marked `#[deprecated]`
- Real response: `ferrum_proto::IntentCompileResponse { envelope, warnings }`
- `generate_proposal_id()` helper exists for client-side proposal ID generation

**Decision Required**:
1. Remove deprecated `IntentCompileResponse` from `stage2_types` and re-export from `ferrum_proto`?
2. Or keep deprecated struct for backward compatibility with tests?
3. Or create new `DraftCompileResponse` type?

**Recommendation**: Remove deprecated `IntentCompileResponse` from `stage2_types`. Re-export `ferrum_proto::IntentCompileResponse` in `lib.rs`. Tests should use the real type.

**Dependencies**: D78-2 (already resolved — compile response has no proposal_id)

---

### P4 — Side-Effect Gate Confirmation

**Issue**: D1.3.3 crosses the pure→side-effecting boundary. Need explicit signoff before any HTTP calls are made.

**Current State**:
- D1.3.2a/b are pure/draft (no side effects)
- `rest_mapper.rs` contains read-only tool implementations
- No mutating tool execution enabled

**Decision Required**:
1. Confirm that `reqwest` blocking client may be used for D1.3.3?
2. Confirm that error handling may return `GatewayError` variants to MCP callers?
3. Confirm that no capability/rollback/provenance is introduced in this slice?

**Recommendation**: Confirm P4 by signing off that:
- HTTP client calls are permitted for intent compile/evaluate
- Error responses follow existing `GatewayError` classification
- No governance pipeline stages beyond compile/evaluate are introduced

**Dependencies**: P1, P2, P3 must be resolved first

---

## 3. D1.3.3 Implementation Plan (After Preflight)

If P1-P4 are approved, D1.3.3 implementation may proceed with:

### Allowed:
- Replace `PrincipalId::new()` with UUID v5 derivation from `actor_id`
- Implement minimal `IntentInputRef` with `source: "mcp"`, `trust_level: Untrusted`
- Remove deprecated `IntentCompileResponse`, re-export real type from `ferrum_proto`
- Add `Option<ProposalId>` field to tool call flow
- Implement `POST /v1/intents/compile` using `FerrumGatewayClient`
- Implement `POST /v1/proposals/{proposal_id}/evaluate` using client-generated proposal_id
- Add tests for real compile/evaluate flow (mocked responses)

### Forbidden:
- Calling any endpoint beyond compile/evaluate
- Capability minting, rollback preparation, provenance emission
- Enabling mutating tool execution beyond `NOT_IMPLEMENTED`
- Claiming D1.3.4 or later readiness

---

## 4. Decision Log

| Decision | Status | Alternatives | Current Recommendation |
| --- | --- | --- | --- |
| P1: stable principal mapping | **Pending** | UUID v5 vs generated vs blocked | UUID v5 derived from actor_id (stable for routing, not auth) |
| P2: raw_inputs policy | **Pending** | minimal untrusted ref vs blocked | MCP-derived IntentInputRef with source="mcp", trust_level=Untrusted |
| P3: IntentCompileResponse DTO | **Pending** | remove/deprecate vs re-export | Remove deprecated struct; re-export from ferrum_proto |
| P4: side-effect gate | **Pending** | confirm vs remain pure | Confirm HTTP client usage for compile/evaluate only |

---

## 5. Cross-References

- Doc 78 (D-1.3.2b): D78-11 records D1.3.3 side-effect boundary; D78-12 confirms GATE-4 no-mutating boundary
- Doc 75 (Phase D-1 Stage 2 Plan): Original design for compile/evaluate flow
- Doc 77 (D-1.3.2a helpers): Design constraints and pure helper boundaries

---

## 6. Bottom Line

D-1.3.3 is **BLOCKED** until P1-P4 are approved. This packet does not implement D-1.3.3 — it only records what must be resolved before implementation may begin.

After P1-P4 approval, implementation may proceed but remains bounded to compile/evaluate REST wiring. Governance pipeline stages beyond evaluate (capability mint, rollback prep, provenance emission) remain in later slices.
