# 82 — MCP Server D-1.5 Prepare/Rollback Preflight Packet

> **Status**: Low-level client implemented. D-1.5 is the prepare slice (POST /v1/executions/{execution_id}/prepare). Route already implemented in gateway; low-level HTTP client implemented in `FerrumGatewayClient::prepare_execution`. Tool dispatch, execute, verify, compensate, rollback remain blocked.
> **Non-Claim**: This packet does not enable tool dispatch, execute, verify, compensate, rollback, or any D1.6+/D1.7 behavior, and does not create any production/G2/operator evidence claim.
> **Constraint**: D1.5 covers prepare only. It must not include execute, verify, compensate, rollback, or tool dispatch.

---

## 0. Executive Summary

D-1.5 is the prepare slice of the governance pipeline. The low-level HTTP client (`FerrumGatewayClient::prepare_execution`) is implemented in `crates/ferrum-integrations-mcp/src/http_client.rs`. Tool dispatch, execute, verify, compensate, rollback remain in later slices (D1.6+). This packet does not enable D1.6+ behavior.

---

## 1. D1.5 Scope (Prepare Only)

### 1.1 What D1.5 Includes

D1.5 is the prepare slice:
- `POST /v1/executions/{execution_id}/prepare` with NO request body
- `PrepareExecutionResponse { execution_id, prepared, rollback_contract, warnings }` response parsing
- Execution state validation (only Authorized or Prepared states can be prepared)
- DraftOnly guard enforcement at prepare checkpoint
- Rollback contract creation with `ActionType::McpToolMutation` support
- `RollbackTarget` as tagged enum (`#[serde(tag = "kind")]`)
- Tool dispatch and D1.6+ remain blocked

### 1.2 What D1.5 Excludes

D1.5 is **prepare only**. The following are excluded and belong to later slices:

| Item | Slice | Reason |
|------|-------|--------|
| Execute tool | D1.6 | Requires execution_id + preparation |
| Verify execution | D1.7 | Requires execution_id + completion |
| Compensate execution | D1.7 | Requires execution_id + failure state |
| Rollback execution | D1.7 | Requires execution_id + failure state |
| Approval/reject tools | D1.7 | Backend endpoint absent |
| Tool dispatch wiring | D1.7 | Blocked by semantics gap |
| Direct provenance emission | Forbidden | Gateway emits internally; MCP must not emit |

### 1.3 What D1.5 Forbidden Items Remain

| Item | Status | Reason |
| --- | --- | --- |
| Execute tool | Blocked | D1.6 scope |
| Verify execution | Blocked | D1.7 scope |
| Compensate execution | Blocked | D1.7 scope |
| Rollback execution | Blocked | D1.7 scope |
| Provenance emission | Forbidden | Gateway emits internally; MCP must not emit |
| Approval/reject execution | Blocked | Backend endpoint absent |
| Tool dispatch | Blocked | D1.7 blocked by semantics |
| Production readiness claim | Blocked | RC-ready only, not production |

---

## 2. Endpoint Contract

### Route: POST /v1/executions/{execution_id}/prepare

**Request Body**: NONE (empty request)

**Response** (JSON, 200 OK):
```json
{
  "execution_id": "uuid-execution",
  "prepared": true,
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
    "state": "Prepared",
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
- `409 Conflict`: Execution in non-preparable state (not Authorized or Prepared), or draft-only intent cannot proceed
- `500 Internal Server Error`: Gateway error

---

## 3. Execution State Validation

### 3.1 Valid States for Prepare

Only the following execution states can transition to Prepared via prepare:

| State | Preparable | Notes |
|-------|-----------|-------|
| `Authorized` | Yes | From authorize with dry_run=true |
| `Prepared` | Yes | From authorize with dry_run=false |
| `Proposed` | No | Not yet authorized |
| `Running` | No | Already running |
| `AwaitingApproval` | No | Waiting for approval |
| `AwaitingVerification` | No | Waiting for verification |
| `Committed` | No | Already completed successfully |
| `Compensated` | No | Already compensated |
| `RolledBack` | No | Already rolled back |
| `Denied` | No | Denied by policy |
| `Quarantined` | No | In quarantine |
| `Failed` | No | Execution failed |
| `Canceled` | No | Execution canceled |

### 3.2 State Transition Guard

The gateway enforces state validation **before** any side effects (database writes, provenance emission). If an execution is not in `Authorized` or `Prepared` state, the gateway returns `409 Conflict` with `ApiErrorCode::Conflict`.

---

## 4. DraftOnly Guard

### 4.1 WS3: Draft-Only Intent Guard

Per the oracle verdict, there is a **DraftOnly guard** at the prepare checkpoint:

**Location**: `crates/ferrum-gateway/src/server.rs:prepare_execution`

**Logic**:
1. Look up the intent linked to the execution
2. If `intent.approval_mode == ApprovalMode::DraftOnly`:
   - Reject prepare with `403 Forbidden` (`ApiErrorCode::PolicyDenied`)
   - Message: "draft-only intent cannot proceed to prepare"

This prevents a draft-only intent from bypassing evaluate and reaching prepare.

---

## 5. RollbackTarget Tagged Enum

### 5.1 RollbackTarget Structure

The `RollbackTarget` enum uses `#[serde(tag = "kind")]` for JSON serialization:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum RollbackTarget {
    FilePath {
        path: String,
        before_hash: Option<String>,
        after_hash: Option<String>,
    },
    GitRef {
        repo_path: String,
        before_ref: Option<String>,
        after_ref: Option<String>,
    },
    SqliteTxn {
        db_path: String,
        tx_id: String,
    },
    HttpRequest {
        method: HttpMethod,
        url: String,
        request_digest: String,
    },
    EmailDraft {
        draft_id: Option<String>,
        recipients: Vec<String>,
    },
    Generic {
        namespace: String,
        identifier: String,
    },
}
```

### 5.2 Example JSON Serialization

```json
{
  "kind": "FilePath",
  "path": "/tmp/output.txt",
  "before_hash": null,
  "after_hash": null
}
```

---

## 6. ActionType::McpToolMutation

### 6.1 ActionType Enum

The `ActionType` enum includes `McpToolMutation` for MCP tool operations:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ActionType {
    FileWrite,
    FileDelete,
    FileMove,
    FileCopy,
    FileAppend,
    FileChmod,
    DirCreate,
    DirDelete,
    GitCommit,
    GitBranchCreate,
    GitBranchDelete,
    GitTagCreate,
    GitTagDelete,
    GitPush,
    GitPull,
    GitFetch,
    SqlMutation,
    HttpMutation,
    EmailDraftCreate,
    EmailSend,
    MailDraft,
    McpToolMutation,
    Unknown,
}
```

---

## 7. Response DTOs

### 7.1 PrepareExecutionResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareExecutionResponse {
    pub execution_id: crate::ExecutionId,
    pub prepared: bool,
    pub rollback_contract: Option<crate::RollbackContract>,
    pub warnings: Vec<String>,
}
```

### 7.2 RollbackContract Fields

| Field | Type | Description |
|-------|------|-------------|
| `contract_id` | `RollbackContractId` | Gateway-generated contract ID |
| `intent_id` | `IntentId` | Links to compiled intent |
| `proposal_id` | `ProposalId` | Links to evaluated proposal |
| `execution_id` | `ExecutionId` | Links to execution record |
| `action_type` | `ActionType` | Type of action including `McpToolMutation` |
| `rollback_class` | `RollbackClass` | R0, R1, R2, etc. |
| `adapter_key` | `String` | Adapter that handles this action |
| `target` | `RollbackTarget` | Tagged enum for target resource |
| `prepare_checks` | `Vec<CheckSpec>` | Pre-execution checks |
| `verify_checks` | `Vec<CheckSpec>` | Post-execution verification |
| `compensation_plan` | `Vec<CompensationStep>` | Steps to undo the action |
| `auto_commit` | `bool` | Whether to auto-commit after verification |
| `state` | `RollbackState` | Contract state |
| `created_at` | `Timestamp` | Contract creation time |
| `expires_at` | `Option<Timestamp>` | Expiration time |

---

## 8. Client Implementation

### 8.1 FerrumGatewayClient::prepare_execution

Located in `crates/ferrum-integrations-mcp/src/http_client.rs`:

```rust
/// Prepare execution: POST /v1/executions/{execution_id}/prepare
///
/// Per doc 82: This is the prepare-only gate (D1.5).
/// It does NOT implement execute/verify/compensate/rollback (D1.6+).
///
/// Takes an `execution_id` in the URL path with NO request body.
/// The gateway validates the execution is in Authorized or Prepared state,
/// then creates a rollback contract for the execution.
pub fn prepare_execution(
    &self,
    execution_id: &ferrum_proto::ExecutionId,
) -> Result<ferrum_proto::PrepareExecutionResponse, GatewayError>
```

### 8.2 Request Format

- **Method**: POST
- **Path**: `/v1/executions/{execution_id}/prepare`
- **Body**: None (empty)

### 8.3 Response Parsing

The client parses `PrepareExecutionResponse` from JSON:
- `execution_id`: Execution ID from request
- `prepared`: Boolean indicating success
- `rollback_contract`: Optional rollback contract
- `warnings`: Advisory warnings

---

## 9. Test Strategy

### 9.1 Unit Tests
- `prepare_execution` sends POST with correct path and no body
- `prepare_execution` parses response with rollback_contract
- `prepare_execution` handles warnings in response
- Wrong path fails (not calling execute/verify/compensate)
- Wrong method fails (not GET, etc.)

### 9.2 Integration Tests (Mocked Gateway)
- Prepare authorized execution → 200 OK with rollback_contract
- Prepare execution in invalid state → 409 Conflict
- Prepare draft-only intent → 403 Forbidden
- Prepare execution not found → 404 Not Found

### 9.3 Forbidden Test Cases
- Verify no execute/verify/compensate/rollback call is made during prepare
- Verify no provenance event is emitted by MCP (gateway emits internally)

---

## 10. Risk and Blocker List

| ID | Item | Severity | Status | Mitigation |
|----|------|----------|--------|------------|
| R1 | Execution state validation in gateway | High | **RESOLVED** | Gateway validates state before side effects |
| R2 | DraftOnly guard | High | **RESOLVED** | Gateway rejects draft-only intents at prepare |
| R3 | D1.6+ blocked | High | **RESOLVED** | Tool dispatch, execute, verify, compensate, rollback remain NOT_IMPLEMENTED |
| R4 | Provenance emission | Forbidden | **RESOLVED** | Gateway emits internally; MCP must not emit |

---

## 11. Approval Criteria

For D1.5 to be considered complete:

- [ ] Execution state validation: Only Authorized or Prepared states can be prepared
- [ ] DraftOnly guard: Reject prepare for draft-only intents
- [ ] RollbackTarget tagged enum documented
- [ ] ActionType::McpToolMutation documented
- [ ] Low-level client `FerrumGatewayClient::prepare_execution` implemented
- [ ] Tests verify wrong path/method fails
- [ ] Tests verify D1.6+ paths not called
- [ ] Oracle signs off that prepare scope is correct (no execute/verify/compensate/rollback)

---

## 12. Cross-References

- Doc 81 (D-1.4 preflight): Capability mint and authorize (previous slice)
- Doc 80 (D-1.3.4 preflight): Evaluate flow
- Doc 79 (D-1.3.3 preflight): Compile flow
- `crates/ferrum-proto/src/api.rs`: `PrepareExecutionResponse` definition
- `crates/ferrum-proto/src/rollback.rs`: `RollbackContract`, `RollbackTarget`, `ActionType` definitions
- `crates/ferrum-proto/src/execution.rs`: `ExecutionState` enum
- `crates/ferrum-gateway/src/server.rs`: `prepare_execution` endpoint implementation
- `crates/ferrum-integrations-mcp/src/http_client.rs`: `FerrumGatewayClient::prepare_execution`

---

## 13. Bottom Line

D1.5 is the prepare slice. The low-level HTTP client (`FerrumGatewayClient::prepare_execution`) is implemented in `crates/ferrum-integrations-mcp/src/http_client.rs`. Execution state validation and DraftOnly guard are enforced in the gateway. Tool dispatch, execute, verify, compensate, rollback remain in later slices (D1.6+). This packet does not enable D1.6+ behavior.
