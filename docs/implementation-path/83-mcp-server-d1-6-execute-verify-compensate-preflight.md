# 83 — MCP Server D-1.6 Execute/Verify/Compensate Preflight Packet

> **Status**: Low-level execute/verify/compensate REST clients implemented; auto_commit verify risk resolved. D-1.6 covers execute/verify/compensate REST routes (POST /v1/executions/{execution_id}/execute, /verify, /compensate). Tool dispatch, approval tools, direct provenance emission, and D1.7+ remain blocked.
> **Non-Claim**: This packet does not enable tool dispatch, does not create any production/G2/operator evidence claim, and does not approve D1.6 for anything beyond low-level FerrumGatewayClient REST methods.
> **Constraint**: D1.6 covers execute/verify/compensate only. It must not include tool dispatch, approval tools, or provenance emission (which is gateway-internal and forbidden for MCP).
> **Oracle Gate**: This doc targets low-level FerrumGatewayClient REST methods only (oracle approval required before any implementation). Tool dispatch is D1.7+.

---

## 0. Executive Summary

D-1.6 is the execute/verify/compensate slice of the governance pipeline. Three gateway routes exist: POST /v1/executions/{execution_id}/execute, /verify, and /compensate. MCP now has low-level `FerrumGatewayClient` REST methods for execute/verify/compensate; `ferrum_gate_execute_prepared` and `ferrum_gate_compensate` remain MUTATING_TOOLS → NOT_IMPLEMENTED, and no `ferrum_gate_verify` tool exists. This packet documents the endpoint contracts, state machines, provenance events, implemented low-level client boundary, and remaining D1.7+ tool-dispatch block.

---

## 1. D1.6 Scope (Execute/Verify/Compensate Only)

### 1.1 What D1.6 Includes

D1.6 covers the three post-prepare REST routes:

| Route | Method | Request Body | Response |
|-------|--------|--------------|----------|
| `/v1/executions/{execution_id}/execute` | POST | `ExecuteExecutionRequest { payload: serde_json::Value }` | `ExecuteExecutionResponse` |
| `/v1/executions/{execution_id}/verify` | POST | None | `VerifyExecutionResponse` |
| `/v1/executions/{execution_id}/compensate` | POST | None | `CompensateExecutionResponse` |

**Execute valid entry**:
- Rollback contract state: `Prepared`
- Execution state: `Prepared` | `Authorized` | `Proposed`
- Transitions: contract → `ExecutedAwaitingVerify`, execution → `Running`
- Emits: `ToolCallExecuted`

**Verify valid entry**:
- Rollback contract state: `ExecutedAwaitingVerify`
- Execution state: `Running` | `AwaitingVerification`
- Success transitions: contract → `Verified`, execution → `Committed`; emits `SideEffectVerified` + `SideEffectCommitted`
- Failure transitions: contract/execution → `Failed`; no provenance emitted

**Compensate valid entry**:
- Rollback contract state: `ExecutedAwaitingVerify`
- Execution state: `Running` | `AwaitingVerification`
- Transitions: contract/execution → `Compensated`
- Emits: `SideEffectCompensated`

### 1.2 What D1.6 Excludes

D1.6 is **execute/verify/compensate REST routes only**. The following are excluded and belong to later slices:

| Item | Slice | Reason |
|------|-------|--------|
| Execute tool dispatch | D1.7 | Tool dispatch wiring blocked pending oracle |
| Verify tool | D1.7 | No `ferrum_gate_verify` MCP tool exists |
| Approval/reject tools | D1.7 | Backend endpoint absent |
| Direct provenance emission | Forbidden | Gateway emits internally; MCP must not emit |
| Tool dispatch wiring | D1.7 | Blocked by semantics gap |

### 1.3 What D1.6 Forbidden Items Remain

| Item | Status | Reason |
| --- | --- | --- |
| Execute tool dispatch | Blocked | D1.7 scope |
| Verify tool | Blocked | No `ferrum_gate_verify` MCP tool exists |
| Compensate tool | Blocked | `ferrum_gate_compensate` is MUTATING_TOOL → NOT_IMPLEMENTED |
| Provenance emission | Forbidden | Gateway emits internally; MCP must not emit |
| Approval/reject execution | Blocked | Backend endpoint absent |
| Tool dispatch | Blocked | D1.7 blocked by semantics |
| Production readiness claim | Blocked | RC-ready only, not production |

---

## 2. Endpoint Contracts

### 2.1 Route: POST /v1/executions/{execution_id}/execute

**Request Body** (JSON):
```json
{
  "payload": { /* serde_json::Value — tool-specific payload */ }
}
```

**Response** (JSON, 200 OK):
```json
{
  "execution_id": "uuid-execution",
  "executed": true,
  "result_digest": "sha256:...",
  "rollback_contract": {
    "contract_id": "uuid-contract",
    "intent_id": "uuid-intent",
    "proposal_id": "uuid-proposal",
    "execution_id": "uuid-execution",
    "action_type": "McpToolMutation",
    "rollback_class": "R0NativeReversible",
    "adapter_key": "fs-adapter",
    "target": {
      "kind": "FilePath",
      "path": "/tmp/output.txt",
      "before_hash": null,
      "after_hash": null
    },
    "prepare_checks": [],
    "verify_checks": [],
    "compensation_plan": [],
    "auto_commit": false,
    "state": "ExecutedAwaitingVerify",
    "created_at": "2026-05-07T00:00:00Z",
    "expires_at": null,
    "metadata": {}
  },
  "warnings": []
}
```

**Error Responses**:
- `400 Bad Request`: Invalid execution_id format or malformed payload
- `401 Unauthorized`: Missing or invalid bearer token
- `404 Not Found`: Execution not found
- `409 Conflict`: Execution not in preparable state (not Prepared/Authorized/Proposed) or contract not Prepared
- `500 Internal Server Error`: Gateway error

### 2.2 Route: POST /v1/executions/{execution_id}/verify

**Request Body**: NONE (empty request)

**Response** (JSON, 200 OK):
```json
{
  "execution_id": "uuid-execution",
  "verified": true,
  "rollback_contract": {
    "contract_id": "uuid-contract",
    "intent_id": "uuid-intent",
    "proposal_id": "uuid-proposal",
    "execution_id": "uuid-execution",
    "action_type": "McpToolMutation",
    "rollback_class": "R0NativeReversible",
    "adapter_key": "fs-adapter",
    "target": {
      "kind": "FilePath",
      "path": "/tmp/output.txt",
      "before_hash": null,
      "after_hash": null
    },
    "prepare_checks": [],
    "verify_checks": [],
    "compensation_plan": [],
    "auto_commit": false,
    "state": "Verified",
    "created_at": "2026-05-07T00:00:00Z",
    "expires_at": null,
    "metadata": {}
  },
  "warnings": []
}
```

**Error Responses**:
- `400 Bad Request`: Invalid execution_id format
- `401 Unauthorized`: Missing or invalid bearer token
- `404 Not Found`: Execution not found
- `409 Conflict`: Contract not in ExecutedAwaitingVerify state or execution not in Running/AwaitingVerification
- `500 Internal Server Error`: Gateway error

### 2.3 Route: POST /v1/executions/{execution_id}/compensate

**Request Body**: NONE (empty request)

**Response** (JSON, 200 OK):
```json
{
  "execution_id": "uuid-execution",
  "compensated": true,
  "rollback_contract": {
    "contract_id": "uuid-contract",
    "intent_id": "uuid-intent",
    "proposal_id": "uuid-proposal",
    "execution_id": "uuid-execution",
    "action_type": "McpToolMutation",
    "rollback_class": "R0NativeReversible",
    "adapter_key": "fs-adapter",
    "target": {
      "kind": "FilePath",
      "path": "/tmp/output.txt",
      "before_hash": null,
      "after_hash": null
    },
    "prepare_checks": [],
    "verify_checks": [],
    "compensation_plan": [],
    "auto_commit": false,
    "state": "Compensated",
    "created_at": "2026-05-07T00:00:00Z",
    "expires_at": null,
    "metadata": {}
  },
  "warnings": []
}
```

**Error Responses**:
- `400 Bad Request`: Invalid execution_id format
- `401 Unauthorized`: Missing or invalid bearer token
- `404 Not Found`: Execution not found
- `409 Conflict`: Contract not in ExecutedAwaitingVerify state or execution not in Running/AwaitingVerification
- `500 Internal Server Error`: Gateway error

---

## 3. Response DTOs

### 3.1 ExecuteExecutionRequest

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteExecutionRequest {
    pub payload: serde_json::Value,
}
```

### 3.2 ExecuteExecutionResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub executed: bool,
    pub result_digest: Option<String>,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}
```

### 3.3 VerifyExecutionResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub verified: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}
```

### 3.4 CompensateExecutionResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompensateExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub compensated: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}
```

---

## 4. Execution State Validation

### 4.1 Valid States for Execute

| State | Exe-cutable | Notes |
|-------|-------------|-------|
| `Prepared` | Yes | Normal path after prepare |
| `Authorized` | Yes | Direct from authorize (dry_run=false) |
| `Proposed` | Yes | Before explicit authorize |
| `Running` | No | Already running |
| `AwaitingVerification` | No | Waiting for verify |
| `AwaitingApproval` | No | Waiting for approval |
| `Committed` | No | Already completed |
| `Compensated` | No | Already compensated |
| `RolledBack` | No | Already rolled back |
| `Denied` | No | Denied |
| `Quarantined` | No | In quarantine |
| `Failed` | No | Already failed |
| `Canceled` | No | Canceled |

### 4.2 Valid States for Verify and Compensate

| State | Verify/Compensatable | Notes |
|-------|----------------------|-------|
| `Running` | Yes | Normal verify/compensate path |
| `AwaitingVerification` | Yes | Re-verification path |
| `Prepared` | No | Not yet executed |
| `Authorized` | No | Not yet executed |
| `Proposed` | No | Not yet executed |
| `Committed` | No | Already completed |
| `Compensated` | No | Already compensated |
| `RolledBack` | No | Already rolled back |
| `Failed` | No | Already failed |
| `Denied` | No | Denied |
| `Quarantined` | No | In quarantine |
| `Canceled` | No | Canceled |

---

## 5. Rollback Contract State Machine

### 5.1 Contract States

| State | Can Transition To | Notes |
|-------|-------------------|-------|
| `Prepared` | `ExecutedAwaitingVerify` | After execute |
| `ExecutedAwaitingVerify` | `Verified`, `Compensated`, `Failed` | After verify (success/fail) or compensate |
| `Verified` | (terminal on success) | After verify success; auto_commit behavior TBD |
| `Compensated` | (terminal) | After compensate |
| `Failed` | (terminal) | After verify failure |

### 5.2 Execution States

| State | Can Transition To | Notes |
|-------|-------------------|-------|
| `Prepared` | `Running` | After execute |
| `Authorized` | `Running` | After execute |
| `Proposed` | `Running` | After execute |
| `Running` | `AwaitingVerification`, `Committed`, `Compensated`, `Failed` | After execute/verify/compensate |
| `AwaitingVerification` | `Committed`, `Compensated`, `Failed` | After verify/compensate |
| `Committed` | (terminal) | After verify success |
| `Compensated` | (terminal) | After compensate |
| `Failed` | (terminal) | After verify failure |

---

## 6. Provenance Events

### 6.1 Execute Emits

- **Event**: `ToolCallExecuted`
- **Trigger**: Successful execute transition
- **Emitter**: Gateway (internal — MCP must not emit)

### 6.2 Verify Emits (verified=true only)

- **Events**: `SideEffectVerified` + `SideEffectCommitted`
- **Trigger**: Successful verify with verified=true
- **Emitter**: Gateway (internal — MCP must not emit)

### 6.3 Compensate Emits

- **Event**: `SideEffectCompensated`
- **Trigger**: Successful compensate
- **Emitter**: Gateway (internal — MCP must not emit)

### 6.4 Verify Emits (Both Success and Failure)

Verify **always** emits `SideEffectVerified` regardless of verification result.

**On verified=true:**
- Emits: `SideEffectVerified` + `SideEffectCommitted` (if `auto_commit=true`) or `SideEffectVerified` only (if `auto_commit=false`)
- If `auto_commit=false`: execution remains in `Running`/`AwaitingVerification` state; `SideEffectCommitted` is suppressed

**On verified=false:**
- Emits: `SideEffectVerified` with `verified=false` in metadata
- Contract state → `Failed`; Execution state → `Failed`
- `SideEffectCommitted` is suppressed

---

## 7. ⚠️ CRITICAL REVIEW RISK: auto_commit Verify Behavior

### 7.1 Risk Description

`RollbackContract.auto_commit` exists as a field, indicating the intent that after verification, the gateway should check `auto_commit` before committing. However, **verify currently always commits on success regardless of the `auto_commit` value**.

### 7.2 Current Behavior (verify success)

```
contract state: ExecutedAwaitingVerify → Verified
execution state: Running → Committed
emits: SideEffectVerified + SideEffectCommitted
auto_commit: IGNORED — always commits on verified=true
```

### 7.3 Intended Behavior

```
contract state: ExecutedAwaitingVerify → Verified (if auto_commit=true) OR stays ExecutedAwaitingVerify (if auto_commit=false)
execution state: Running → Committed (if auto_commit=true) OR stays Running (if auto_commit=false)
emits: SideEffectVerified only (if auto_commit=false) OR SideEffectVerified + SideEffectCommitted (if auto_commit=true)
```

### 7.4 Action Required

This behavior has been **corrected** in the D1.6 implementation. The gateway now checks `auto_commit` before committing on verify success. When `auto_commit=false`, execution remains in Running state and SideEffectCommitted is suppressed.

---

## 8. MCP Client Status

### 8.1 FerrumGatewayClient Methods

Low-level REST client methods are **IMPLEMENTED** in `FerrumGatewayClient`. Tool dispatch via rest_mapper remains NOT_IMPLEMENTED (D1.7+ blocked):

| Method | Status | Notes |
|--------|--------|-------|
| `execute_execution` | IMPLEMENTED | Low-level REST client; tool dispatch D1.7+ |
| `verify_execution` | IMPLEMENTED | Low-level REST client; tool dispatch D1.7+ |
| `compensate_execution` | IMPLEMENTED | Low-level REST client; tool dispatch D1.7+ |

### 8.2 MCP Tool Registry Status

| Tool | Kind | Status |
|------|------|--------|
| `ferrum_gate_execute_prepared` | MUTATING_TOOL | NOT_IMPLEMENTED |
| `ferrum_gate_compensate` | MUTATING_TOOL | NOT_IMPLEMENTED |
| `ferrum_gate_verify` | — | Does not exist |

### 8.3 D1.6 Implemented API

Low-level REST clients are implemented in `FerrumGatewayClient`:

```rust
impl FerrumGatewayClient {
    pub fn execute_execution(
        &self,
        execution_id: &ExecutionId,
        request: &ExecuteExecutionRequest,
    ) -> Result<ExecuteExecutionResponse, GatewayError>;

    pub fn verify_execution(
        &self,
        execution_id: &ExecutionId,
    ) -> Result<VerifyExecutionResponse, GatewayError>;

    pub fn compensate_execution(
        &self,
        execution_id: &ExecutionId,
    ) -> Result<CompensateExecutionResponse, GatewayError>;
}
```

**Tool dispatch (D1.7+)** remains blocked pending oracle approval of the full dispatch semantics.

---

## 9. Test Strategy

### 9.1 Unit Tests
- `execute_execution` sends POST with correct path and payload
- `execute_execution` parses response with result_digest and rollback_contract
- `verify_execution` sends POST with correct path and no body
- `verify_execution` parses response with verified flag
- `compensate_execution` sends POST with correct path and no body
- `compensate_execution` parses response with compensated flag
- Wrong path fails (not calling prepare, etc.)
- Wrong method fails (not POST, not GET, etc.)

### 9.2 Integration Tests (Mocked Gateway)
- Execute authorized execution → 200 OK with ExecutedAwaitingVerify contract
- Execute execution in invalid state → 409 Conflict
- Verify executed execution (success) → 200 OK with Verified/Committed
- Verify executed execution (failure) → 200 OK with Failed (no provenance)
- Compensate executed execution → 200 OK with Compensated
- Compensate in invalid state → 409 Conflict

### 9.3 Forbidden Test Cases
- Verify no tool dispatch call is made during execute/verify/compensate
- Verify no provenance event is emitted by MCP (gateway emits internally)
- Verify auto_commit is NOT silently ignored (when fixed)

---

## 10. Risk and Blocker List

| ID | Item | Severity | Status | Mitigation |
|----|------|----------|--------|------------|
| R1 | **auto_commit verify risk** | **CRITICAL** | **RESOLVED** | Gateway checks auto_commit before SideEffectCommitted; execution stays Running when auto_commit=false |
| R2 | Execution state validation | High | RESOLVED | Gateway validates state before side effects |
| R3 | D1.7+ blocked | High | RESOLVED | Tool dispatch, approval tools remain NOT_IMPLEMENTED |
| R4 | Provenance emission | Forbidden | RESOLVED | Gateway emits internally; MCP must not emit |
| R5 | Low-level clients not implemented | Medium | RESOLVED | execute/verify/compensate REST clients now implemented |
| R6 | No ferrum_gate_verify tool | Medium | RESOLVED | Tool does not exist; verify via REST only |

---

## 11. Approval Criteria

D1.6 is approved for low-level REST client implementation:

- [x] **CRITICAL: auto_commit verify risk resolved** — gateway checks `auto_commit` before committing on verify success
- [x] Execution state validation: Only preparable states can execute; only ExecutedAwaitingVerify can verify/compensate
- [x] Response DTOs documented (ExecuteExecutionRequest/Response, VerifyExecutionResponse, CompensateExecutionResponse)
- [x] Provenance event mapping documented (ToolCallExecuted, SideEffectVerified, SideEffectCommitted, SideEffectCompensated)
- [x] Contract state machine documented (Prepared → ExecutedAwaitingVerify → Verified/Compensated/Failed)
- [x] Low-level REST clients implemented in FerrumGatewayClient
- [x] No tool dispatch enablement (rest_mapper MUTATING_TOOLS remain NOT_IMPLEMENTED)
- [x] Oracle signed off on low-level REST client scope (no tool dispatch, no provenance emission)

---

## 12. Cross-References

- Doc 82 (D-1.5 preflight): Prepare only (previous slice)
- Doc 81 (D-1.4 preflight): Capability mint and authorize
- Doc 80 (D-1.3.4 preflight): Evaluate flow
- Doc 79 (D-1.3.3 preflight): Compile flow
- `crates/ferrum-proto/src/api.rs`: `ExecuteExecutionRequest`, `ExecuteExecutionResponse`, `VerifyExecutionResponse`, `CompensateExecutionResponse` definitions
- `crates/ferrum-proto/src/rollback.rs`: `RollbackContract`, `RollbackState` definitions
- `crates/ferrum-proto/src/execution.rs`: `ExecutionState` enum
- `crates/ferrum-gateway/src/server.rs`: `execute_execution`, `verify_execution`, `compensate_execution` endpoint implementations
- `crates/ferrum-integrations-mcp/src/http_client.rs`: `FerrumGatewayClient` with execute_execution, verify_execution, compensate_execution methods

---

## 13. Bottom Line

D1.6 is the execute/verify/compensate slice. Three gateway REST routes exist (POST /v1/executions/{execution_id}/execute, /verify, /compensate). Low-level `FerrumGatewayClient` methods are implemented (`execute_execution`, `verify_execution`, `compensate_execution`). Tool dispatch via rest_mapper remains NOT_IMPLEMENTED — `ferrum_gate_execute_prepared` and `ferrum_gate_compensate` are MUTATING_TOOLS → NOT_IMPLEMENTED; no `ferrum_gate_verify` tool exists. The **auto_commit verify risk has been resolved**: the gateway now checks `auto_commit` before committing on verify success. This packet does not enable tool dispatch, does not emit provenance from MCP, and does not create production/G2 evidence.

(End of file - total 596 lines)
