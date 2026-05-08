# D1.10 MCP Full-Pipeline Local Validation

## Status: IMPLEMENTED

## Overview

D1.10 validates the complete MCP lifecycle pipeline end-to-end through `handle_tools_call_with_client` using mockito-backed `FerrumGatewayClient`. It exercises all 8 governance pipeline steps (compile → evaluate → mint → authorize → prepare → execute → verify → compensate) in a single sequential test, asserting success at each step and verifying D1.8/D1.9 inherited redaction boundaries.

D1.10 does **not** claim production-ready, G2-complete, or live target validation status. It is bounded local/mock validation only.

---

## 1. Validation Scope

### 1.1 What D1.10 Validates

| Validation Item | Description | Status |
|-----------------|-------------|--------|
| Full 8-step sequential lifecycle | All 8 MCP lifecycle tools chained through `handle_tools_call_with_client` | **Implemented** |
| ID chaining across steps | `intent_id` → `proposal_id` → `capability_id` → `execution_id` flows correctly | **Implemented** |
| Success assertion at each step | Each step returns `JsonRpcResponse::Success` | **Implemented** |
| D1.8/D1.9 redaction inheritance | Sensitive fields redacted in lifecycle responses (metadata, trust_context, result_digest, etc.) | **Implemented** |
| No-over-redaction | IDs, tool_binding, decision, reason preserved | **Implemented** |
| Path-aware compensation_plan[].args redaction | `args` redacted only inside compensation_plan array elements | **Implemented** |
| Blocked approve/reject regression | Missing-arg/blocked approve-reject behavior unchanged | **Verified via existing D1.9.3 tests** |

### 1.2 What D1.10 Does NOT Validate

| Item | Reason |
|------|--------|
| Production/G2 claims | Not in scope; local/mock validation only |
| Live target ferrumd | All tests use mockito; no live gateway |
| Multi-node/PostgreSQL (Phase 3) | Not in scope |
| Approval/reject backend endpoints | Backend absent; permanently blocked |
| Performance/throughput | Not measured |
| Actual rollback/compensation semantics | Mocked responses only; real adapter behavior not executed |

---

## 2. Evidence

### 2.1 Implementation Evidence

| Evidence | Location | Citation |
|----------|----------|----------|
| Full sequential lifecycle test | `lib.rs:3087-3434` | `test_d1_10_full_lifecycle_sequential` |
| D1.8/D1.9 redaction assertions | `lib.rs` D1.10 tests | Phase 1+2 key redaction verified |
| Restricted approve/reject tests | `lib.rs:2672-3079` | D1.9.3 tests already cover blocked tool behavior |
| handle_tools_call_with_client choke | `lib.rs:974-1018` | D1.9 redact_tool_content_text + D1.8 sanitize_output |

### 2.2 Sequential Pipeline Choke Point

```
handle_tools_call_with_client (lib.rs:974)
    │
    ▼ rest_mapper::map_tool_to_rest routes to lifecycle function
    ▼ FerrumGatewayClient calls mockito endpoint
    ▼ D1.9 redact_tool_content_text(result)  ← Phase 1 + Phase 2 redaction
    ▼ D1.8 TaintScoringFirewall::sanitize_output  ← Control char stripping
    ▼ JsonRpcResponse::success(sanitized, id)
```

### 2.3 ID Flow in Sequential Test

1. `ferrum_gate_submit_intent` → returns `intent_id`
2. `ferrum_gate_evaluate_intent` → uses `intent_id`, returns `proposal_id`
3. `ferrum_gate_mint_capability` → uses `intent_id` + `proposal_id`, returns `capability_id`
4. `ferrum_gate_authorize_execution` → uses `proposal_id` + `capability_id`, returns `execution_id`
5. `ferrum_gate_prepare_execution` → uses `execution_id`
6. `ferrum_gate_execute_prepared` → uses `execution_id`
7. `ferrum_gate_verify` → uses `execution_id`
8. `ferrum_gate_compensate` → uses `execution_id`

---

## 3. Test Coverage

### 3.1 Implemented Tests

| Test | Description | Key Assertions |
|------|-------------|----------------|
| `test_d1_10_full_lifecycle_sequential` | Full 8-step pipeline through handle_tools_call_with_client | All 8 steps succeed; IDs chained correctly |
| `test_d1_10_lifecycle_metadata_redaction` | Metadata redaction in lifecycle response | `metadata` → `[REDACTED]`; IDs preserved |
| `test_d1_10_lifecycle_trust_context_redaction` | Trust context redaction in lifecycle response | `trust_context` → `[REDACTED]` |
| `test_d1_10_lifecycle_result_digest_redaction` | Result digest redaction in execute response | `result_digest` → `[REDACTED]` |
| `test_d1_10_lifecycle_compensation_plan_args_redaction` | Path-aware args redaction in rollback_contract | `compensation_plan[].args` → `[REDACTED]` |
| `test_d1_10_lifecycle_no_over_redaction` | IDs, tool_binding, decision preserved | No-over-redaction confirmed |

### 3.2 Test Pattern

Each D1.10 integration test:
1. Creates mockito server with appropriate endpoint mocks
2. Creates `FerrumGatewayClient` pointed at mockito server
3. Calls `handle_tools_call_with_client` with tool name and arguments
4. Asserts `JsonRpcResponse::Success` result
5. Validates D1.8/D1.9 redaction in response text

---

## 4. Non-Claims

D1.10 validation does **not** establish:

- **Production-ready**: Bounded mock-based validation only
- **G2-complete**: No G2 signoff claimed
- **Live target evidence**: All tests use mockito; no live ferrumd
- **Real rollback/compensation**: Responses are mocked; actual adapter behavior not executed
- **Phase 3 PostgreSQL/multi-node**: Not validated
- **Approval/reject endpoints**: Backend absent; permanently blocked

---

## 5. Next Gates

| Gate | Owner | Status |
|------|-------|--------|
| Live target validation with real ferrumd | Explorer | Optional future work |
| G2 readiness evidence | Operator | Future |
| Production-ready claim | Operator | Future |

---

## 6. References

| From | To | Purpose |
|------|-----|---------|
| This doc | [`87-mcp-server-d1-9-3-dlp-integration-validation.md`](87-mcp-server-d1-9-3-dlp-integration-validation.md) | D1.9.3 integration tests (D1.10 extends) |
| This doc | [`84-mcp-server-d1-7-tool-dispatch-preflight.md`](84-mcp-server-d1-7-tool-dispatch-preflight.md) | D1.7 tool dispatch design |
| This doc | [`lib.rs:974-1018`](lib.rs) | `handle_tools_call_with_client` choke point |
| This doc | [`lib.rs:3087-end`](lib.rs) | D1.10 integration tests |
