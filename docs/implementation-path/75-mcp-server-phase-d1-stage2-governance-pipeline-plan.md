# 75 — MCP Server Phase D-1 Stage 2 Governance Pipeline Plan

> **Status**: Planning documentation only. Stage 2 implementation is deferred — requires design review and explicit approval gate before any implementation begins.
> **Stage 1 Status**: D-1.1 (Auth) + D-1.2 (Tool Registry) complete. Stage 1 adds actor identity resolution and mutating tool default-deny. No execution.
> **Purpose**: Detailed implementation plan for Stage 2 (D-1.3–D-1.7) — policy evaluation, capability issuance, rollback preparation, provenance, and tool execution integration.
> **Scope**: Stage 2 implements the governance pipeline integration. D-1.8+ (Sanitization, Rate Limiting) remains GATED pending Stage 2 completion.
> **Constraint**: Do not claim MCP server readiness. Do not claim production/G2/operator approval. Do not implement D-1.3+ without explicit design review gate approval.
> **Handoff from**: [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) (Stage 1 complete; Stage 2 plan is next step)

---

## Explicit Non-Claims

- **No MCP server readiness claim.** Stage 2 is planning documentation; implementation is gated.
- **No production-ready claim.** Stage 2 implementation does not make MCP server production-ready.
- **No G2 complete claim.** G2.1–G2.8 remain pending (operator-owned).
- **MCP server is post-v1 scope.** Phase D-1 is for v1.4 MCP Governance Beta.
- **Stage 2 not approved.** Implementation requires explicit design review gate and approval.
- **No mutating tool execution until backend endpoints exist.** Approval tools remain blocked per §Blockers.
- **Provenance gap remains.** If gateway does not emit ActionProposalSubmitted/ToolCallPrepared/Executed via REST path, this must be called out and resolved before Stage 2 implementation proceeds.
- **D-0/D-1 Stage 1 smoke is local evidence only.** Not production, operator, or target-host evidence.

---

## 0. Stage 1 Recap and Stage 2 Scope

### 0.1 Stage 1 Complete (D-1.1 + D-1.2)

| Item | Status | Evidence |
|------|--------|----------|
| D-1.1 ActorIdentity with env var precedence | ✅ Done | `ActorIdentity::resolve()` |
| D-1.2 MUTATING_TOOLS constant (7 tools) | ✅ Done | `lib.rs::MUTATING_TOOLS` |
| D-1.2 Default-deny mutating tools | ✅ Done | `McpToolError::not_implemented()` returns -32001 |
| D-1.6 error_codes consolidation | ✅ Done | Single `error_codes` module in `lib.rs` |

### 0.2 Stage 2 Scope (D-1.3 – D-1.7)

Stage 2 implements the governance pipeline for mutating tools:

| Phase | Items | Description |
|-------|-------|-------------|
| D-1.3 | Policy Evaluation Integration | Map MCP call to ActionProposal; evaluate against policy bundle |
| D-1.4 | Capability Issuance | Mint single-use capability with TTL ≤ 300s |
| D-1.5 | Rollback Preparation | Prepare rollback contract before execution |
| D-1.6 | Provenance Emission | Emit lineage events (gateway-internal preferred; gap must be resolved) |
| D-1.7 | Tool Execution | Implement submit/evaluate/prepare/execute/compensate tools |

### 0.3 Why Stage 2 is Gated

Stage 2 was gated in doc 74 because:
1. **Endpoint mismatches in doc 74**: Explorer findings reveal actual endpoints differ from doc 74's design
2. **Sequential ID flow not designed**: How intent_id → proposal_id → capability_id → execution_id flows needs explicit design
3. **Approval endpoint gap**: No backend approval resolve endpoint exists — `ferrum_gate_approve_intent` and `ferrum_gate_reject_intent` cannot be implemented
4. **Provenance gap**: No direct provenance emission endpoint; gateway-internal emission must be verified
5. **evaluate/submit semantics**: Doc 74 had evaluate and submit as separate tools but their interaction needs clarification

This plan resolves those gaps before implementation.

---

## 1. Endpoint and DTO Map (Corrected from Explorer Findings)

### 1.1 Corrected Endpoint Map

> **⚠️ Doc 74 endpoint paths were incorrect.** This section supersedes doc 74 §3.2. All paths below are from explorer findings.

| Governance Step | MCP Tool(s) | Actual Gateway REST Endpoint | HTTP Method | DTOs |
|----------------|-------------|----------------------------|-------------|------|
| **Compile Intent** | `ferrum_gate_submit_intent` | `/v1/intents/compile` | POST | `IntentCompileRequest` → `IntentCompileResponse` |
| **Policy Eval** | `ferrum_gate_evaluate_intent` | `/v1/proposals/{proposal_id}/evaluate` | POST | (uses compiled proposal) → `EvaluateProposalResponse` |
| **Capability Mint** | (internal, after eval) | `/v1/capabilities/mint` | POST | `CapabilityMintRequest` → `CapabilityMintResponse` |
| **Authorize** | (internal) | `/v1/executions/authorize` | POST | `AuthorizeExecutionRequest` → `AuthorizeExecutionResponse` |
| **Prepare** | `ferrum_gate_prepare_execution` | `/v1/executions/{execution_id}/prepare` | POST | → `PrepareExecutionResponse` |
| **Execute** | `ferrum_gate_execute_prepared` | `/v1/executions/{execution_id}/execute` | POST | `ExecuteExecutionRequest` → `ExecuteExecutionResponse` |
| **Verify** | (internal) | `/v1/executions/{execution_id}/verify` | POST | → `VerifyExecutionResponse` |
| **Compensate** | `ferrum_gate_compensate` | `/v1/executions/{execution_id}/compensate` | POST | → `CompensateExecutionResponse` |
| **Cancel** | (not exposed) | `/v1/executions/{execution_id}/cancel` | POST | (internal only) |
| **Evaluate Outcome** | (not exposed) | `/v1/executions/{execution_id}/evaluate-outcome` | POST | → `EvaluateOutcomeResponse` |

### 1.2 DTO Summary

| DTO | Direction | Description |
|-----|----------|-------------|
| `IntentCompileRequest` | MCP → Gateway | Compile MCP tool call into ActionProposal |
| `IntentCompileResponse` | Gateway → MCP | Contains `proposal_id` for policy eval |
| `ActionProposal` | Internal | FerrumGate internal representation of the proposed action |
| `EvaluateProposalResponse` | Gateway → MCP | Contains policy decision (Allow/Deny) |
| `CapabilityMintRequest` | MCP → Gateway | Request capability for execution |
| `CapabilityMintResponse` | Gateway → MCP | Contains `capability_id`, TTL, scope |
| `AuthorizeExecutionRequest` | MCP → Gateway | Request authorization for execution |
| `AuthorizeExecutionResponse` | Gateway → MCP | Contains authorization result |
| `PrepareExecutionResponse` | Gateway → MCP | Contains `execution_id` for subsequent calls |
| `ExecuteExecutionRequest` | MCP → Gateway | Execute with capability proof |
| `ExecuteExecutionResponse` | Gateway → MCP | Contains execution result or pending status |
| `VerifyExecutionResponse` | Gateway → MCP | Contains verification result |
| `CompensateExecutionResponse` | Gateway → MCP | Contains compensation result |
| `EvaluateOutcomeResponse` | Gateway → MCP | Contains outcome report |

### 1.3 Gap: Approval Endpoints Do Not Exist

> **🔴 Blocker**: No backend endpoint exists for `ferrum_gate_approve_intent` or `ferrum_gate_reject_intent`.

Explorer findings show no approval resolve endpoint. These tools **must remain blocked** until:
1. Backend implements approval resolve endpoint, OR
2. Explicit design decision that approvals go through a different flow

**Impact**: `ferrum_gate_approve_intent` and `ferrum_gate_reject_intent` remain in `MUTATING_TOOLS` but return `NOT_IMPLEMENTED` even after Stage 2 complete.

### 1.4 Gap: No Direct Provenance Emission Endpoint

> **⚠️ Open Question**: Explorer found no direct provenance emission endpoint (`POST /v1/provenance/emit` or similar).

Doc 74 §3.6 assumes provenance emission via REST. If gateway uses internal-only emission, MCP server cannot emit provenance events directly. This must be resolved before Stage 2 proceeds.

**Options**:
1. **Gateway-internal emission (preferred)**: Gateway emits `ActionProposalSubmitted`, `ToolCallPrepared`, `ToolCallExecuted` internally when corresponding REST calls are made. MCP server does NOT emit provenance — it only triggers the REST calls that cause internal emission.
2. **Gap acknowledged**: If gateway does NOT emit these events internally, provenance gap must be documented and resolved before Stage 2.

**Decision required**: Does gateway internally emit `ActionProposalSubmitted` when `POST /v1/intents/compile` is called? Does it emit `ToolCallPrepared` when `POST /v1/executions/{id}/prepare` is called?

---

## 2. Sequential ID Flow

### 2.1 ID Flow Diagram

```
MCP tools/call: ferrum_gate_submit_intent
        │
        ▼
POST /v1/intents/compile
        │
        ▼ returns { proposal_id: "uuid-1" }
        │
        ▼
POST /v1/proposals/{proposal_id}/evaluate
        │
        ▼ returns { decision: "allow", execution_id: "uuid-2" }  [NOTE: eval may return execution_id directly]
        │
        ▼ [if allowed]
POST /v1/capabilities/mint
        │
        ▼ returns { capability_id: "uuid-3", ttl: 300 }
        │
        ▼
POST /v1/executions/authorize
        │
        ▼ returns { authorized: true }
        │
        ▼
POST /v1/executions/{execution_id}/prepare
        │
        ▼ returns { prepared: true, rollback_contract_id: "uuid-4" }
        │
        ▼
POST /v1/executions/{execution_id}/execute
        │
        ▼ returns { status: "completed" | "pending" }
        │
        ▼ [if pending]
GET /v1/executions/{execution_id}
        │
        ▼ [poll until completed]
        │
        ▼
POST /v1/executions/{execution_id}/verify
        │
        ▼ returns { verified: true }
```

### 2.2 ID Persistence Points

| ID | Purpose | Persisted By | When Created |
|----|---------|--------------|--------------|
| `proposal_id` | References compiled intent | Gateway | POST /v1/intents/compile |
| `execution_id` | References the execution | Gateway | Policy eval returns it OR prepare returns it |
| `capability_id` | Single-use execution token | Gateway | POST /v1/capabilities/mint |
| `rollback_contract_id` | References rollback state | Gateway | POST /v1/executions/{id}/prepare |

### 2.3 Pending/Polling Behavior

For long-running executions, the MCP server uses a **pending/polling model**:

1. **Immediate return** (if sync complete):
   ```json
   {
     "jsonrpc": "2.0",
     "result": {
       "content": [{ "type": "text", "text": "{\"status\": \"completed\", \"result\": {...}}" }]
     },
     "id": 1
   }
   ```

2. **Immediate return** (if async/pending):
   ```json
   {
     "jsonrpc": "2.0",
     "result": {
       "content": [{ "type": "text", "text": "{\"status\": \"pending\", \"execution_id\": \"uuid-2\", \"poll_url\": \"/v1/executions/uuid-2\"}" }]
     },
     "id": 1
   }
   ```

3. **Polling** via `GET /v1/executions/{execution_id}` returns status until `status: "completed"` or `status: "failed"`

### 2.4 Open Question: evaluate vs submit Semantics

Doc 74 had both `ferrum_gate_submit_intent` and `ferrum_gate_evaluate_intent` as separate tools. Explorer findings suggest:

- `POST /v1/intents/compile` compiles intent → returns `proposal_id`
- `POST /v1/proposals/{proposal_id}/evaluate` evaluates proposal → may return `execution_id` directly

**Clarification needed**:
- Is `submit_intent` meant to compile AND submit (auto-evaluate)?
- Is `evaluate_intent` meant to evaluate an already-submitted intent?
- What is the relationship between intent submission and policy evaluation?

**This ambiguity must be resolved before Stage 2 implementation.**

---

## 3. Capability Lifecycle

### 3.1 Capability Issuance Flow

```
1. MCP submits intent via POST /v1/intents/compile
2. Gateway compiles → returns proposal_id
3. MCP evaluates via POST /v1/proposals/{proposal_id}/evaluate
4. Gateway evaluates → if allowed, returns execution_id
5. MCP requests capability via POST /v1/capabilities/mint
6. Gateway mint → returns capability_id, ttl_seconds, scope
7. MCP includes capability_id in subsequent execution calls
```

### 3.2 Capability Requirements

| Requirement | Value | Enforcement |
|-------------|-------|-------------|
| TTL max | 300 seconds | Gateway enforces |
| Single-use | Yes | Gateway tracks usage |
| Scope match | Must match tool call scope | Gateway validates |
| Expiry | After ttl | Gateway invalidates |

### 3.3 Capability in Execution Calls

Subsequent calls (`/prepare`, `/execute`) must include capability proof. The exact mechanism (header, request body, etc.) must be verified with actual gateway API.

---

## 4. Rollback Contract Persistence

### 4.1 Rollback Preparation Flow

```
1. MCP calls POST /v1/executions/{execution_id}/prepare
2. Gateway prepares rollback contract
3. Gateway stores contract (internal state)
4. Returns { rollback_contract_id: "uuid-4", prepared: true }
5. MCP proceeds to execute
```

### 4.2 Rollback Contract Contents

The rollback contract (managed by gateway via `ferrum-rollback`) contains:
- Compensation actions in reverse order of execution
- Parameters needed for each compensation action
- Timeout for compensation execution
- Rollback contract ID for later reference

### 4.3 Rollback Execution

If execution fails or compensation is requested:
```
POST /v1/executions/{execution_id}/compensate
→ Returns CompensateExecutionResponse
```

### 4.4 Open Question: Rollback Contract ID Exposure

**Unknown**: Should MCP server store the `rollback_contract_id` locally for debugging, or is it purely a gateway-internal reference?

---

## 5. Provenance Strategy

### 5.1 Preferred: Gateway-Internal Emission

Per explorer findings, gateway may emit provenance events internally when REST endpoints are called. If this is the case, MCP server **does not need to emit provenance directly**.

The REST call sequence implies the following events are emitted internally:

| REST Call | Implied Provenance Event |
|-----------|--------------------------|
| POST /v1/intents/compile | ActionProposalSubmitted |
| POST /v1/proposals/{id}/evaluate | PolicyEvaluated |
| POST /v1/capabilities/mint | CapabilityMinted |
| POST /v1/executions/{id}/prepare | ToolCallPrepared |
| POST /v1/executions/{id}/execute | ToolCallExecuted |
| POST /v1/executions/{id}/verify | SideEffectPrepared, SideEffectVerified |
| POST /v1/executions/{id}/compensate | SideEffectCompensated |

### 5.2 Provenance Gap: Must Be Resolved Before Stage 2

> **🔴 Blocker**: If gateway does NOT emit provenance events internally, MCP server cannot fulfill the provenance-first lineage requirement.

**Required resolution**:
1. Verify with gateway team: Does calling `POST /v1/intents/compile` emit `ActionProposalSubmitted` internally?
2. Verify: Does calling `POST /v1/executions/{id}/prepare` emit `ToolCallPrepared` internally?
3. If NO to either: Provenance gap must be addressed before Stage 2 proceeds.

### 5.3 MCP Server Provenance Role (if gateway-internal)

If gateway handles emission internally, MCP server role is:
- Trigger the correct REST calls in correct order
- Trust that gateway emits provenance events
- Log/monitor for any provenance-related errors from gateway responses

---

## 6. Error Mapping

### 6.1 REST Error to MCP Error Code Mapping

| Gateway HTTP Status | Gateway Error Type | MCP JSON-RPC Code | MCP Error Name |
|--------------------|--------------------|--------------------|----------------|
| 400 | Bad Request | -32602 | INVALID_PARAMS |
| 401 | Unauthorized | -32002 | AUTH_FAILED |
| 403 | Forbidden | -32002 | AUTH_FAILED |
| 404 | Not Found | -32601 | METHOD_NOT_FOUND |
| 409 | Conflict | -32000 | SERVER_ERROR |
| 422 | Unprocessable | -32602 | INVALID_PARAMS |
| 429 | Rate Limited | -32005 | RATE_LIMITED |
| 500 | Internal Error | -32004 | GATEWAY_SERVER_ERROR |
| 503 | Unavailable | -32003 | GATEWAY_UNREACHABLE |
| timeout | — | -32003 | GATEWAY_UNREACHABLE |

### 6.2 Gateway Error Response Format

Expected gateway error format:
```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable message",
    "details": { ... }
  }
}
```

### 6.3 MCP-Specific Error Codes

| Code | Name | When Used |
|------|------|----------|
| -32001 | NOT_IMPLEMENTED | Mutating tool called but governance not ready |
| -32002 | AUTH_FAILED | 401/403 from gateway |
| -32003 | GATEWAY_UNREACHABLE | Connection refused, timeout |
| -32004 | GATEWAY_SERVER_ERROR | 4xx/5xx from gateway |
| -32005 | RATE_LIMITED | 429 from gateway |
| -32601 | METHOD_NOT_FOUND | Unknown tool (not in MUTATING_TOOLS or READ_ONLY_TOOLS) |
| -32602 | INVALID_PARAMS | Malformed request to gateway |

---

## 7. Test Strategy

### 7.1 Unit Tests

| Phase | Test | Mock |
|-------|------|------|
| D-1.3 | Policy eval request serialization | Mock `IntentCompileRequest` → JSON |
| D-1.3 | ActionProposal mapping from MCP call | Verify correct fields mapped |
| D-1.4 | CapabilityMintRequest creation | Mock gateway response |
| D-1.5 | PrepareExecution call | Mock gateway response |
| D-1.7 | Full submit flow (compile → eval → cap → prepare → execute) | Mock sequence |

### 7.2 Integration Tests (Mock Gateway)

| Test | Description |
|------|-------------|
| `test_submit_intent_full_flow` | MCP call → compile → eval → cap → prepare → execute |
| `test_evaluate_intent_flow` | MCP call → eval → returns decision |
| `test_compensate_flow` | MCP call → compensate |
| `test_polling_on_pending_execution` | Execute returns pending → poll → completed |
| `test_auth_failure_propagates` | Gateway 401 → MCP -32002 |
| `test_mutating_tool_blocked_until_governance` | Call mutating tool without full flow → -32001 |

### 7.3 Integration Tests (Real Gateway, Local Dev Only)

> **⚠️ Local dev evidence only — NOT production/target-host evidence.**

| Test | Description |
|------|-------------|
| `test_submit_intent_against_real_gateway` | Full flow against real gateway (dev config, auth disabled) |
| `test_compensate_against_real_gateway` | Compensate against real gateway |

### 7.4 Open Question: Test Infrastructure

**Unknown**: Does integration test infrastructure exist for MCP → gateway integration testing? May need to create `ferrum-mcp-integration-tests` crate or add to existing `ferrum-integration-tests`.

---

## 8. Blockers and Open Questions

### 8.1 🔴 Blockers (Must Resolve Before Stage 2 Implementation)

| # | Blocker | Impact | Owner | Resolution Path |
|---|---------|--------|-------|-----------------|
| **B1** | **No approval resolve endpoint** | `ferrum_gate_approve_intent` and `ferrum_gate_reject_intent` cannot be implemented | Gateway team | Design approval flow; implement endpoint; OR explicitly defer approval tools |
| **B2** | **Provenance gap not verified** | Cannot confirm ActionProposalSubmitted/ToolCallPrepared/Executed are emitted by gateway | Gateway team | Verify gateway emits these events internally; if not, add endpoint or fix |
| **B3** | **evaluate/submit semantics ambiguous** | Unclear how evaluate_intent and submit_intent interact | Design review | Clarify: Is submit auto-eval? Is evaluate for re-evaluation? |
| **B4** | **Doc 74 endpoint paths were wrong** | Design used incorrect paths | Engineering | This doc (75) corrects paths; design review must approve corrected paths |

### 8.2 ⚠️ Open Questions (Should Resolve Before Stage 2 Implementation)

| # | Question | Impact | Status |
|---|----------|--------|--------|
| **Q1** | What is the exact `AuthorizeExecutionRequest` payload? | Must match gateway API | Unknown |
| **Q2** | How is capability proof included in execution calls (header, body)? | Affects MCP request construction | Unknown |
| **Q3** | Does policy eval return `execution_id` directly, or does `prepare` create it? | Affects sequential ID flow | Unknown |
| **Q4** | What is the `VerifyExecutionResponse` content? | Must understand verification result | Unknown |
| **Q5** | Should MCP server store `rollback_contract_id` locally? | Debugging/tracing concern | Undecided |
| **Q6** | Does `evaluate-outcome` need MCP exposure? | May not need MCP tool | Undecided |
| **Q7** | Test infrastructure: does `ferrum-integration-tests` support MCP → gateway? | Affects integration test approach | Unknown |

---

## 9. Ordered Implementation Todo-List

### Stage 2 Phases (D-1.3 – D-1.7)

| Phase | # | Item | Status | Dependencies | Notes |
|-------|---|------|--------|--------------|-------|
| **D-1.3: Policy Eval** | D1.3.1 | Define `IntentCompileRequest` struct | Future | — | Map MCP tool call to compile request |
| | D1.3.2 | Define `ActionProposal` mapping | Future | D1.3.1 | Map MCP call to ActionProposal |
| | D1.3.3 | Implement compile call: POST /v1/intents/compile | Future | D1.3.2 | Returns proposal_id |
| | D1.3.4 | Implement eval call: POST /v1/proposals/{id}/evaluate | Future | D1.3.3 | Returns policy decision |
| | D1.3.5 | Add D-1.3 unit tests | Future | D1.3.4 | Test request/response mapping |
| **D-1.4: Capability** | D1.4.1 | Define `CapabilityMintRequest/Response` structs | Future | — | |
| | D1.4.2 | Implement mint call: POST /v1/capabilities/mint | Future | D1.3.4 | After policy eval passes |
| | D1.4.3 | Verify TTL ≤ 300s enforcement | Future | D1.4.2 | Gateway enforces |
| | D1.4.4 | Add D-1.4 unit tests | Future | D1.4.3 | |
| **D-1.5: Rollback** | D1.5.1 | Define rollback-related DTOs | Future | — | |
| | D1.5.2 | Implement prepare call: POST /v1/executions/{id}/prepare | Future | D1.4.2 | Returns execution_id, rollback_contract_id |
| | D1.5.3 | Add D-1.5 unit tests | Future | D1.5.2 | |
| **D-1.6: Provenance** | D1.6.1 | Verify gateway-internal emission | Future | B2 resolved | Confirm ActionProposalSubmitted emitted on compile |
| | D1.6.2 | Verify ToolCallPrepared emitted on prepare | Future | B2 resolved | Confirm gateway emits event |
| | D1.6.3 | Add provenance logging/monitoring | Future | D1.6.1, D1.6.2 | If gateway does not emit, this becomes a blocker |
| | D1.6.4 | Add D-1.6 tests | Future | D1.6.3 | |
| **D-1.7: Execution** | D1.7.1 | Define ExecuteExecutionRequest/Response | Future | — | |
| | D1.7.2 | Implement execute call: POST /v1/executions/{id}/execute | Future | D1.5.2 | |
| | D1.7.3 | Implement verify call: POST /v1/executions/{id}/verify | Future | D1.7.2 | |
| | D1.7.4 | Implement compensate call: POST /v1/executions/{id}/compensate | Future | D1.7.3 | |
| | D1.7.5 | Implement polling logic for pending executions | Future | D1.7.2 | |
| | D1.7.6 | Wire ferrum_gate_submit_intent tool | Future | D1.3.4, D1.4.2, D1.5.2, D1.7.2, D1.7.3 | Full compile→eval→cap→prepare→execute→verify |
| | D1.7.7 | Wire ferrum_gate_evaluate_intent tool | Future | D1.3.4, B3 resolved | Depends on Q3 clarification |
| | D1.7.8 | Wire ferrum_gate_prepare_execution tool | Future | D1.5.2 | |
| | D1.7.9 | Wire ferrum_gate_execute_prepared tool | Future | D1.7.2 | |
| | D1.7.10 | Wire ferrum_gate_compensate tool | Future | D1.7.4 | |
| | D1.7.11 | **ferrum_gate_approve_intent remains blocked** | Future | B1 resolved | MUST NOT implement until B1 resolved |
| | D1.7.12 | **ferrum_gate_reject_intent remains blocked** | Future | B1 resolved | MUST NOT implement until B1 resolved |
| | D1.7.13 | Add D-1.7 unit + integration tests | Future | D1.7.6–D1.7.10 | |

### Post-Stage 2 (D-1.8 – D-1.10) — Remain GATED

| Phase | Items | Gated By |
|-------|-------|----------|
| D-1.8: Output Sanitization | Integrate ferrum-firewall, redact sensitive fields | Stage 2 complete |
| D-1.9: Rate Limiting | Integrate tower_governor, per-agent limits | Stage 2 complete |
| D-1.10: Integration | End-to-end tests, load tests, smoke tests | D-1.8, D-1.9 complete |

---

## 10. Stage 2 Acceptance Criteria (Design Review Gate)

Before Stage 2 implementation begins, the following must be reviewed and approved:

| Criterion | Verification | Status |
|-----------|--------------|--------|
| Endpoint paths match actual gateway routes | Explorer findings reviewed; paths in §1.1 match | ⚠️ Needs review |
| Sequential ID flow is clear | §2.1 diagram reviewed; no ambiguity | ⚠️ Needs review |
| Provenance gap resolved | Gateway team confirms internal emission OR endpoint added | 🔴 Blocked (B2) |
| Approval endpoint gap acknowledged | §1.3 reviewed; approval tools remain blocked | ✅ Acknowledged |
| evaluate/submit semantics clarified | Q3 resolved; tool interaction clear | 🔴 Blocked (B3) |
| Error mapping reviewed | §6 reviewed; codes match gateway | ✅ OK |
| Test strategy reviewed | §7 reviewed; feasible | ⚠️ Q7 unknown |
| All blockers listed | §8.1 complete | ✅ OK |

---

## 11. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) | Stage 1 complete; Stage 2 plan |
| This doc | [`73-mcp-server-phase-d-implementation-plan.md`](73-mcp-server-phase-d-implementation-plan.md) | D-0 complete |
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope |
| This doc | [`README.md`](README.md) | Reading order entry |
| This doc | [`artifacts/2026-05-06/73-d0-live-smoke-evidence.md`](artifacts/2026-05-06/73-d0-live-smoke-evidence.md) | D-0 local smoke evidence |

---

## 12. Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| Gateway-internal provenance preferred | TBD | Avoids adding provenance emission endpoint; reduces MCP complexity |
| Approval tools remain blocked | 2026-05-06 | No backend endpoint exists; implementor must not bypass this blocker |
| Doc 74 endpoint paths corrected | 2026-05-06 | Explorer findings reveal actual paths differ from doc 74 design |
| Sequential ID flow requires explicit design | 2026-05-06 | proposal_id → execution_id → capability_id → rollback_contract_id sequence needs clarity |

---

## 13. Status Table

| Item | Status |
|------|--------|
| Stage 1 (D-1.1 + D-1.2) | ✅ Complete |
| Stage 2 Plan (D-1.3 – D-1.7) | 📋 This document — GATED |
| Stage 3 (D-1.8 – D-1.9) | ⏳ Gated on Stage 2 |
| Stage 4 (D-1.10) | ⏳ Gated on Stage 3 |

---

*Document created: 2026-05-06. Planning documentation only. Stage 2 implementation is GATED — requires design review and explicit approval. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
