# 78 — MCP Server D-1.3.2b Mapping Completion Review Packet

> **Status**: Review packet only. D-1.3.2a pure helpers are implemented and verified, but D-1.3.2b remains **blocked** until the gates in this packet are resolved.
> **Purpose**: Reconcile doc/code drift after D-1.3.2a and decide whether the next slice may safely move from pure helpers to fuller ActionProposal mapping.
> **Constraint**: Do not implement HTTP wiring, governance endpoint calls, mutating execution, approval/reject flows, or provenance emission from this packet alone.
> **Non-Claim**: This packet does not approve D-1.3.3, does not complete D-1, and does not create any production/G2/operator evidence claim.

---

## 0. Executive Summary

D-1.3.2a is now a completed safe slice: `mapping_helpers.rs` contains pure, deterministic helper functions and tests that use real `ferrum-proto` types without HTTP calls or side effects.

D-1.3.2b is **not ready** yet. Explorer review found five contradictions between doc 76, doc 77, and actual code, plus one provenance-timing blocker. The most important correction is that gateway `IntentCompileResponse` does **not** return a `proposal_id`; it returns `{ envelope, warnings }`, and `IntentEnvelope` contains `intent_id`, not `proposal_id`.

This packet turns those findings into a decision checklist. It is intended to be reviewed before any D-1.3.2b implementation task is opened.

---

## 1. Current Confirmed State

### 1.1 Completed Safe Slice

| Item | Status | Evidence |
| --- | --- | --- |
| D-1.3.1 types-only scaffolding | Complete | `crates/ferrum-integrations-mcp/src/stage2_types.rs` |
| D-1.3.2a pure helpers | Complete | `crates/ferrum-integrations-mcp/src/mapping_helpers.rs` |
| Real `ferrum-proto` type usage | Complete | `mapping_helpers.rs` imports `ferrum_proto::{common, ids, intent}` types |
| No HTTP/REST in helpers | Verified | D-1.3.2a scoped verify found no executable HTTP imports/calls |
| Mutating tools default-deny | Preserved | `rest_mapper.rs` and regression tests keep mutating tools returning `NOT_IMPLEMENTED` |

### 1.2 Still Blocked

| Item | Status | Reason |
| --- | --- | --- |
| D-1.3.2b full mapping completion | Blocked | unresolved C-E6, C-RISK, C-RB, C-PRIN, C-RAW, B-MAP-7 |
| D-1.3.3 `POST /v1/intents/compile` wiring | Blocked | D-1.3.2b not approved; side-effect boundary not crossed |
| Policy evaluation / capability mint / rollback prepare | Blocked | later governance pipeline stages; require separate review gates |
| Approval/reject tools | Blocked | backend approval resolve endpoint absent |
| Direct provenance emission | Blocked | strategy unresolved; gateway-internal emission only where available |

---

## 2. B-MAP Status Table

| Blocker | Original Concern | Current Status | Review Decision Needed |
| --- | --- | --- | --- |
| B-MAP-1 | 6 of 12 gateway `IntentCompileRequest` fields lack MCP source | **Partial** | Decide `goal` derivation and `raw_inputs -> IntentInputRef` mapping |
| B-MAP-2 | Parse `requested_resource_scope` from MCP `scope` string | **Resolved in D-1.3.2a** | Confirm parser grammar is acceptable for D-1.3.2b |
| B-MAP-3 | Infer `RiskTier` and `ApprovalMode` | **Implemented, but needs signoff** | Resolve `http_post` risk and whether `DraftOnly` is meaningful for MCP |
| B-MAP-4 | Resolve `server_name` | **Implemented, but needs vocabulary signoff** | Confirm prefix vocabulary and unknown-action behavior |
| B-MAP-5 | Infer `expected_effect` and `estimated_risk` | **Implemented, but needs signoff** | Confirm hardcoded effect strings are acceptable drafts |
| B-MAP-6 | Infer `requested_rollback_class` | **Implemented, but doc conflict remains** | Resolve `http_post` rollback class |
| B-MAP-7 | `ActionProposalSubmitted` provenance timing | **Not resolved** | Confirm whether event timing remains gateway-authorize-time or needs MCP-side emission strategy |

---

## 3. Contradictions to Reconcile

### C-E6 — `IntentCompileResponse` Shape

**Doc 76 problem:** It describes compile output as if `proposal_id` exists in or under the response envelope.

**Code reality:**
- `crates/ferrum-proto/src/intent.rs` defines `IntentCompileResponse { envelope, warnings }`.
- `IntentEnvelope` contains `intent_id`, not `proposal_id`.
- `crates/ferrum-gateway/src/server.rs` compile handler returns the envelope and warnings.
- `ActionProposal` evaluation receives a client-provided `proposal_id` later.

**Decision required:**
- D-1.3.2b must treat `proposal_id` as MCP/client-generated for proposal evaluation, not as compile output.
- Doc 76 should be corrected before D-1.3.2b implementation proceeds.

### C-RISK — `http_post` RiskTier

**Conflict:**
- Doc 76 maps `http_post -> RiskTier::Medium`.
- Doc 77 and `mapping_helpers.rs` map `http_post -> RiskTier::High`.

**Recommended decision:** Keep `http_post -> High` for D-1.3.2b because POST can affect external systems and the MCP governance posture should fail conservative.

### C-RB — `http_post` RollbackClass

**Conflict:**
- Doc 76 maps `http_post -> R2Compensatable`.
- Doc 77 and `mapping_helpers.rs` map `http_post -> R3IrreversibleHighConsequence`.

**Recommended decision:** Keep `http_post -> R3IrreversibleHighConsequence` unless a specific compensating endpoint contract exists. Generic HTTP POST compensation is not reliable enough to assume R2.

### C-PRIN — `principal_id` Blocker Classification

**Conflict:**
- Doc 76 classifies `principal_id` as a true blocker with no default.
- D-1.3.2a currently auto-generates `PrincipalId::new()` in pure mapping.

**Decision required:**
- Either accept generated `PrincipalId` as a draft-only placeholder for D-1.3.2b, or require actor-to-principal mapping before any full mapping implementation.

**Recommended decision:** Accept generated `PrincipalId` only as a draft mapping field, not an identity/authentication claim. If later policy semantics require stable principal identity, route that separately.

### C-RAW — `raw_inputs -> IntentInputRef` Mapping

**Conflict:**
- Doc 76 classifies `raw_inputs` as direct mapping from parameters.
- Actual `IntentInputRef` is structured and includes fields such as source identity, labels, summary, and event linkage.
- D-1.3.2a correctly leaves this unresolved instead of fabricating a trustworthy input ref.

**Decision required:**
- Define a minimal draft `IntentInputRef` policy or keep D-1.3.2b blocked on raw input semantics.

**Recommended decision:** Do not fabricate provenance/trust labels. If a minimal draft is needed, it must be explicitly marked MCP-derived/untrusted and must not imply upstream evidence.

---

## 4. Review Questions Before D-1.3.2b

Answer these before opening an implementation task:

1. **Principal strategy**: Is generated `PrincipalId::new()` acceptable for draft mapping, or must MCP map `actor_id` into a stable principal?
2. **Intent input strategy**: What exact `IntentInputRef` values are allowed for MCP tool parameters?
3. **Scope grammar**: Is the D-1.3.2a `parse_resource_scope()` grammar the canonical MCP scope grammar?
4. **Server vocabulary**: Are the current action prefixes and server names accepted?
5. **Risk inference**: Should `http_post` remain `High`, and should unknown action types fail closed instead of defaulting to `Medium`?
6. **Rollback inference**: Should `http_post` remain `R3IrreversibleHighConsequence` until compensation is explicit?
7. **Approval mode**: Is `DraftOnly` meaningful for MCP, or should medium-risk actions use `Required`?
8. **Expected effect text**: Are hardcoded `expected_effect` strings sufficient for D-1.3.2b drafts?
9. **Compile response flow**: Should D-1.3.2b generate `proposal_id` before evaluate, independent of compile response?
10. **Provenance timing**: Is `ActionProposalSubmitted` emitted only during authorize acceptable, or does D-1 require an earlier event strategy?

---

## 5. Decision Gates

### GATE-1 — Contradiction Resolution

Required before D-1.3.2b:

- [ ] C-E6 corrected: compile response has no `proposal_id`.
- [ ] C-RISK resolved: final `http_post` risk tier selected.
- [ ] C-RB resolved: final `http_post` rollback class selected.
- [ ] C-PRIN resolved: generated principal vs stable mapping decision made.
- [ ] C-RAW resolved: raw input strategy accepted or explicitly deferred.

### GATE-2 — Helper Signoff

Required before D-1.3.2b:

- [ ] D-1.3.2a helper signatures accepted.
- [ ] Parser grammar accepted or change request created.
- [ ] Unknown action behavior accepted or change request created.
- [ ] Regression test preserving mutating default-deny remains in place.

### GATE-3 — Gateway Flow Confirmation

Required before any REST wiring:

- [ ] `POST /v1/intents/compile` response handling uses `{ envelope, warnings }`.
- [ ] `proposal_id` is generated on the client/MCP side before evaluate if needed.
- [ ] `execution_id` is not expected from compile or evaluate; it is created during authorize.
- [ ] D-1.3.3 remains a separate side-effecting step with its own review gate.

### GATE-4 — No-Mutating-Execution Boundary

Required before crossing from mapping to runtime pipeline:

- [ ] D-1.3.2b remains pure or draft-only; no endpoint calls.
- [ ] D-1.3.3 endpoint call is not included in D-1.3.2b.
- [ ] No approval/reject execution is introduced.
- [ ] No capability minting, rollback preparation, or provenance emission is introduced.

---

## 6. Proposed D-1.3.2b Scope If Gates Pass

If GATE-1 through GATE-4 pass, the next implementation slice may be:

**Allowed:**
- Correct MCP-side compile response DTO assumptions.
- Refine pure helper behavior for decisions made above.
- Add tests for corrected `IntentCompileResponse` and client-generated `proposal_id` flow.
- Add draft-only conversion helpers for ActionProposal construction if they remain side-effect-free.

**Forbidden:**
- Calling gateway REST endpoints.
- Adding `.send()` or network-capable governance calls.
- Enabling mutating tools beyond `NOT_IMPLEMENTED`.
- Emitting or fabricating provenance.
- Claiming D-1.3.3 or later readiness.

---

## 7. Recommended Decision Defaults

Unless the reviewer chooses otherwise, use these conservative defaults:

| Topic | Default |
| --- | --- |
| `http_post` risk | `RiskTier::High` |
| `http_post` rollback | `RollbackClass::R3IrreversibleHighConsequence` |
| unknown action type | fail closed with mapping error, not `Medium` |
| principal identity | generated draft ID allowed only with explicit non-auth claim |
| raw inputs | unresolved unless an untrusted MCP-derived `IntentInputRef` policy is written |
| approval mode | prefer `Required` over `DraftOnly` unless gateway semantics confirm `DraftOnly` |
| provenance timing | treat B-MAP-7 as unresolved until gateway owner confirms |

---

## 8. Required Documentation Updates Before Implementation

Before D-1.3.2b code changes:

- [ ] Update doc 76 to correct `IntentCompileResponse` / `proposal_id` claims.
- [ ] Update doc 76 risk/rollback tables to match doc 77 or record an explicit override.
- [ ] Update doc 76 `principal_id` blocker classification based on the decision.
- [ ] Update doc 76 `raw_inputs` classification based on the decision.
- [ ] Record accepted decisions in this doc's decision log.

---

## 9. Decision Log

| Decision | Status | Alternatives | Current Recommendation |
| --- | --- | --- | --- |
| D78-1: Treat D-1.3.2a as complete | Accepted by evidence | Reopen D-1.3.2a | Accept complete; focus D-1.3.2b on contradictions |
| D78-2: Correct compile response shape | Pending | Keep old `proposal_id` assumption | Correct to `{ envelope, warnings }`; generate proposal ID separately |
| D78-3: `http_post` risk | Pending | Medium vs High | High |
| D78-4: `http_post` rollback | Pending | R2 vs R3 | R3 until explicit compensation contract exists |
| D78-5: principal derivation | Pending | generated draft ID vs stable actor mapping | generated draft ID only with explicit non-auth caveat |
| D78-6: raw input mapping | Pending | direct parameter conversion vs untrusted draft ref vs blocked | remain blocked unless untrusted draft policy is written |
| D78-7: provenance timing | Pending | gateway-authorize-time vs MCP-side earlier emission | require gateway owner confirmation |

---

## 10. Bottom Line

D-1.3.2a is complete. D-1.3.2b should **not** start as runtime wiring. The next safe work is documentation reconciliation and a review decision over the five contradictions above.

Only after this packet's gates are checked should implementation proceed, and even then the recommended next slice is still bounded to pure/draft mapping corrections — not REST calls and not mutating execution.
