# 81 — MCP Server D-1.4 Capability/Authorize Preflight Packet

> **Status**: Low-level client implemented. D-1.4 is the capability-mint and authorize slice (POST /v1/capabilities/mint, POST /v1/executions/authorize). I5–I7 approved; low-level HTTP client implemented in `FerrumGatewayClient::mint_capability` and `FerrumGatewayClient::authorize_execution`. Tool dispatch and D1.5+ remain blocked.
> **Non-Claim**: This packet does not enable tool dispatch, prepare, execute, verify, compensate, rollback, or any D1.5+/D1.7 behavior, and does not create any production/G2/operator evidence claim.
> **Constraint**: D1.4 covers capability mint and authorize only. It must not include prepare, execute, verify, compensate, rollback, or tool dispatch.

---

## 0. Executive Summary

D-1.4 is the capability-mint and authorize slice of the governance pipeline. Three preflight items (I5–I7) have been resolved by an oracle/reviewer and recorded here. The low-level HTTP client (`FerrumGatewayClient::mint_capability`, `FerrumGatewayClient::authorize_execution`) is implemented. Tool dispatch and D1.5+ remain blocked.

---

## 1. D1.4 Scope (Capability Mint + Authorize Only)

### 1.1 What D1.4 Includes

D1.4 is the capability-mint and authorize slice:
- `POST /v1/capabilities/mint` with `CapabilityMintRequest` request body
- `POST /v1/executions/authorize` with `AuthorizeExecutionRequest` request body
- `CapabilityMintResponse { lease, warnings }` response parsing
- `AuthorizeExecutionResponse { execution, warnings }` response parsing
- `CapabilityLease` / `ExecutionRecord` field extraction
- Single-use capability enforcement (capability consumed after authorize)
- TTL ≤ 300s enforcement per AGENTS.md invariant
- I5 scope validation (resource_bindings subset_of intent.resource_scope)
- I6 approval binding digest validation (canonical digest match)
- `dry_run: bool` parameter handling in AuthorizeExecutionRequest
- `ActionProposalSubmitted` provenance event emitted internally by gateway after successful authorize
- MCP does NOT emit provenance directly — gateway owns emission

### 1.2 What D1.4 Excludes

D1.4 is **capability-mint and authorize only**. The following are excluded and belong to later slices:

| Item | Slice | Reason |
|------|-------|--------|
| Prepare execution | D1.5 | Requires execution_id from authorize |
| Execute tool | D1.7 | Blocked by capability semantics; requires execution_id |
| Verify execution | D1.7 | Requires execution_id + completion |
| Compensate execution | D1.7 | Requires execution_id + failure state |
| Rollback preparation | D1.5 | Requires execution_id + rollback contract |
| Approval/reject tools | D1.7 | Backend endpoint absent |
| Tool dispatch wiring | D1.7 | Blocked by semantics gap |
| Direct provenance emission | Forbidden | Gateway emits internally; MCP must not emit |

### 1.3 What D1.4 Forbidden Items Remain

| Item | Status | Reason |
| --- | --- | --- |
| Prepare execution | Blocked | D1.5 scope |
| Execute tool | Blocked | D1.7 scope (semantics gap) |
| Verify execution | Blocked | D1.7 scope |
| Compensate execution | Blocked | D1.7 scope |
| Rollback preparation | Blocked | D1.5 scope |
| Provenance emission | Forbidden | Gateway emits internally; MCP must not emit |
| Approval/reject execution | Blocked | Backend endpoint absent |
| Tool dispatch | Blocked | D1.7 blocked by semantics |
| Production readiness claim | Blocked | RC-ready only, not production |

---

## 2. Endpoint Contracts

### 2.1 Route 1: POST /v1/capabilities/mint

**Request Body** (JSON):
```json
{
  "intent_id": "uuid-intent",
  "proposal_id": "uuid-proposal",
  "tool_binding": {
    "server_name": "fs-server",
    "tool_name": "filesystem.write",
    "tool_version": "1.0.0"
  },
  "resource_bindings": [
    {
      "kind": "File",
      "path": "/tmp/output.txt",
      "mode": "Write",
      "required_hash": null
    }
  ],
  "argument_constraints": [
    { "type": "JsonPointerMustExist", "pointer": "/content" }
  ],
  "taint_budget": {
    "max_taint_score": 30,
    "allow_external_tool_output": false,
    "allow_external_metadata": false,
    "allow_untrusted_text": false
  },
  "approval_binding": null,
  "requested_ttl_secs": 120,
  "metadata": {}
}
```

**Response** (JSON, 200 OK):
```json
{
  "lease": {
    "capability_id": "uuid-capability",
    "intent_id": "uuid-intent",
    "proposal_id": "uuid-proposal",
    "tool_binding": { "server_name": "fs-server", "tool_name": "filesystem.write", "tool_version": "1.0.0" },
    "resource_bindings": [...],
    "argument_constraints": [...],
    "taint_budget": {...},
    "approval_binding": null,
    "issued_by": "ferrum-cap",
    "policy_bundle_id": "uuid-bundle",
    "tool_manifest_id": null,
    "manifest_hash": null,
    "status": "Active",
    "issued_at": "2026-05-07T00:00:00Z",
    "expires_at": "2026-05-07T00:02:00Z",
    "revoked_at": null,
    "metadata": {}
  },
  "warnings": []
}
```

**Error Responses**:
- `400 Bad Request`: Invalid capability request, missing required fields, or TTL exceeds maximum (300s)
- `401 Unauthorized`: Missing or invalid bearer token
- `409 Conflict`: (N/A for mint — capability not found or AlreadyUsed occurs at authorize)
- `422 Unprocessable Entity`: (N/A for mint — authorize policy denies return 403 Forbidden with PolicyDenied)
- `500 Internal Server Error`: Gateway error

**Note**: The mint endpoint does NOT validate that intent_id or proposal_id exist. That check happens at authorize time. If intent/proposal is not found at authorize, a 404 is returned.

---

### 2.2 Route 2: POST /v1/executions/authorize

**Request Body** (JSON):
```json
{
  "proposal_id": "uuid-proposal",
  "capability_id": "uuid-capability",
  "dry_run": false
}
```

**Response** (JSON, 200 OK):
```json
{
  "execution": {
    "execution_id": "uuid-execution",
    "proposal_id": "uuid-proposal",
    "intent_id": "uuid-intent",
    "capability_id": "uuid-capability",
    "rollback_contract_id": null,
    "decision": "Allow",
    "state": "Authorized",
    "started_at": "2026-05-07T00:00:00Z",
    "finished_at": null,
    "result_digest": null,
    "metadata": {}
  },
  "warnings": []
}
```

**Error Responses**:
- `400 Bad Request`: Invalid authorize request, missing required fields, or expired capability
- `401 Unauthorized`: Missing or invalid bearer token
- `403 Forbidden`: I5 scope violation (PolicyDenied) or I6 digest mismatch (IntegrityMismatch)
- `404 Not Found`: Proposal or capability not found
- `409 Conflict`: Capability already used
- `500 Internal Server Error`: Gateway error

---

## 3. Request DTOs

### 3.1 CapabilityMintRequest

Defined in `crates/ferrum-proto/src/capability.rs`:

```rust
pub struct CapabilityMintRequest {
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub tool_binding: ToolBinding,
    pub resource_bindings: Vec<ResourceBinding>,
    pub argument_constraints: Vec<ArgumentConstraint>,
    pub taint_budget: TaintBudget,
    pub approval_binding: Option<ApprovalBinding>,
    pub requested_ttl_secs: u64,
    pub metadata: JsonMap,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `intent_id` | `IntentId` | From compile response `envelope.intent_id` |
| `proposal_id` | `ProposalId` | MCP/client-generated ID from evaluate |
| `tool_binding` | `ToolBinding` | Server name + tool name binding |
| `resource_bindings` | `Vec<ResourceBinding>` | File/Git/Sqlite/Http/EmailDraft resources |
| `argument_constraints` | `Vec<ArgumentConstraint>` | ExactString, StringOneOf, StringRegex, etc. |
| `taint_budget` | `TaintBudget` | Taint scoring limits |
| `approval_binding` | `Option<ApprovalBinding>` | Required for Critical risk; null otherwise |
| `requested_ttl_secs` | `u64` | **Must be ≤ 300** per AGENTS.md invariant |
| `metadata` | `JsonMap` | Arbitrary key-value metadata |

### 3.2 AuthorizeExecutionRequest

Defined in `crates/ferrum-proto/src/api.rs`:

```rust
pub struct AuthorizeExecutionRequest {
    pub proposal_id: crate::ProposalId,
    pub capability_id: crate::CapabilityId,
    pub dry_run: bool,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `proposal_id` | `ProposalId` | Proposal ID from evaluate |
| `capability_id` | `CapabilityId` | Capability ID from mint response `lease.capability_id` |
| `dry_run` | `bool` | controls ExecutionRecord.state false->Prepared true->Authorized; both values consume capability, persist record, and emit provenance |

### 3.3 Key Constraints

| Constraint | Value | Source |
|------------|-------|--------|
| TTL max | 300 seconds | AGENTS.md invariant |
| Capability usage | Single-use only | AGENTS.md invariant; `CapabilityStatus::Used` after authorize |
| I5 validation | resource_bindings ⊆ intent.resource_scope | Invariant matrix |
| I6 validation | approval_binding.digest == canonical_action_digest | Invariant matrix + ADR-49 |

---

## 4. Response DTOs

### 4.1 CapabilityMintResponse

```rust
pub struct CapabilityMintResponse {
    pub lease: CapabilityLease,
    pub warnings: Vec<String>,
}
```

**CapabilityLease fields** (from `crates/ferrum-proto/src/capability.rs`):

| Field | Type | Description |
|-------|------|-------------|
| `capability_id` | `CapabilityId` | Gateway-generated single-use token ID |
| `intent_id` | `IntentId` | Links to compiled intent |
| `proposal_id` | `ProposalId` | Links to evaluated proposal |
| `tool_binding` | `ToolBinding` | Server + tool binding from request |
| `resource_bindings` | `Vec<ResourceBinding>` | From request |
| `argument_constraints` | `Vec<ArgumentConstraint>` | From request |
| `taint_budget` | `TaintBudget` | From request |
| `approval_binding` | `Option<ApprovalBinding>` | From request (or null) |
| `issued_by` | `String` | "ferrum-cap" |
| `policy_bundle_id` | `PolicyBundleId` | Active policy bundle ID |
| `status` | `CapabilityStatus` | `Active`, `Used`, `Expired`, `Revoked`, `Quarantined` |
| `issued_at` | `Timestamp` | Mint timestamp |
| `expires_at` | `Timestamp` | issued_at + requested_ttl_secs (capped at 300s) |

### 4.2 AuthorizeExecutionResponse

```rust
pub struct AuthorizeExecutionResponse {
    pub execution: ExecutionRecord,
    pub warnings: Vec<String>,
}
```

**ExecutionRecord fields** (from `crates/ferrum-proto/src/execution.rs`):

| Field | Type | Description |
|-------|------|-------------|
| `execution_id` | `ExecutionId` | Gateway-generated execution ID |
| `proposal_id` | `ProposalId` | From request |
| `intent_id` | `IntentId` | From capability lease |
| `capability_id` | `CapabilityId` | From request (consumed after authorize) |
| `rollback_contract_id` | `Option<RollbackContractId>` | Set during prepare (D1.5) |
| `decision` | `Decision` | Allow/Deny/Quarantine/RequireApproval/AllowDraftOnly |
| `state` | `ExecutionState` | Authorized (→ Prepared → Running → ...) |
| `started_at` | `Timestamp` | Authorize timestamp |
| `finished_at` | `Option<Timestamp>` | Set on terminal state |
| `result_digest` | `Option<String>` | Set after execute (D1.7) |

---

## 5. TTL Constraint

**AGENTS.md Invariant**: Capabilities: `ttl_max=300s`, `single-use only`

- `requested_ttl_secs` in `CapabilityMintRequest` **must be ≤ 300**
- Gateway rejects requests with `requested_ttl_secs > 300` with `400 Bad Request` (`ValidationError`)
- MCP client must validate TTL locally before sending request
- Default TTL recommendation: 120 seconds (2 minutes) for typical operations

---

## 6. Single-Use Constraint

**AGENTS.md Invariant**: `single-use only`

- A capability can only be used once for authorize
- After successful `POST /v1/executions/authorize`, the capability status changes to `Used`
- Subsequent authorize calls with the same `capability_id` return `409 Conflict`
- MCP client must not reuse capability IDs

---

## 7. I5 Validation: Scope Subset Check

**Invariant**: I5 — Scope cannot expand beyond intent (`resource_bindings subset_of intent.resource_scope`)

**Location**: `crates/ferrum-gateway/src/server.rs:validate_resource_bindings_subset_of_scope`

**MCP role**:
- Thread `intent.resource_scope` (from compile response `envelope`) through to authorize call
- Gateway performs the actual subset check internally at authorize time
- If I5 fails, authorize returns `403 Forbidden` with `PolicyDenied`

**I5 Validation Steps** (gateway-side):
1. Load intent from `intent_id` → get `intent.resource_scope`
2. For each `resource_binding` in `CapabilityMintRequest`:
   - If `ResourceBinding::File`: check `path` is within `intent.resource_scope` paths
   - If `ResourceBinding::Sqlite`: check `db_path` is within scope
   - If `ResourceBinding::Git`: check `repo_path` is within scope
   - If `ResourceBinding::Http`: check `base_url + path_prefix` is within scope
3. If any resource binding is outside scope → I5 fails

**MCP client handling for I5 failure**:
- Gateway returns `403 Forbidden` with `PolicyDenied`
- Return `McpToolError::forbidden()` with reason string
- Do not retry without user modification of resource bindings

---

## 8. I6 Validation: Approval Binding Digest

**Invariant**: I6 — Approval binding matches action digest

**Location**: `crates/ferrum-gateway/src/server.rs:validate_approval_binding_digest`

**ADR Reference**: ADR-49 (docs/implementation-path/49-i6-approval-binding-digest-adr.md)

**When I6 applies**: Only when `approval_binding` is `Some(...)` in `CapabilityMintRequest`

**Canonical Action Digest Computation** (from `crates/ferrum-proto/src/execution.rs:canonical_action_digest`):

Fields **included** (sorted by key):
- `intent_id`
- `proposal_id`
- `tool_name`
- `server_name`
- `raw_arguments` (normalized recursively by sorted object keys)
- `expected_effect`
- `estimated_risk`
- `requested_rollback_class`

Fields **excluded**:
- `step_index`
- `taint_inputs`
- `metadata`
- `created_at`

**Digest algorithm**: SHA-256 over deterministic JSON (key-sorted at all levels)

**I6 Validation Steps** (gateway-side, after I5):
1. Load `approval` from `approval_binding.approval_id`
2. Check `approval.state == Granted`
3. Check `approval.expires_at > now()`
4. Compute `canonical_action_digest()` from proposal
5. Compare computed digest with `approval_binding.approved_action_digest`
6. Compare `approval_binding.approved_action_digest` with `approval.digest` (chain validation)
7. If any check fails → I6 fails

**When `approval_binding` is `None`**: I6 check is **skipped silently**

**MCP client handling for I6 failure**:
- Gateway returns `403 Forbidden` with `IntegrityMismatch` (digest chain validation failures) or `PolicyDenied` (approval state/expired failures)
- Return `McpToolError::forbidden()` with reason string
- Do not retry — approval binding mismatch is non-recoverable

---

## 9. Provenance Boundary

**Gateway emits `CapabilityMinted` and `ActionProposalSubmitted` internally** after successful mint and authorize. MCP must NOT emit provenance directly.

From `server.rs` line ~1866–1898 (mint):
```rust
// Emit CapabilityMinted provenance event after mint succeeds.
let cap_event = ProvenanceEvent {
    event_id: EventId::new(),
    kind: ferrum_proto::ProvenanceEventKind::CapabilityMinted,
    occurred_at: Utc::now(),
    actor: ActorRef { actor_type: ActorType::Gateway, actor_id: "ferrum-gateway", ... },
    intent_id: Some(response.lease.intent_id),
    proposal_id: Some(response.lease.proposal_id),
    capability_id: Some(response.lease.capability_id),
    ...
};
```

From `server.rs` line ~2376–2407 (authorize):
```rust
// Emit ActionProposalSubmitted provenance event after authorize succeeds.
let auth_event = ProvenanceEvent {
    event_id: EventId::new(),
    kind: ferrum_proto::ProvenanceEventKind::ActionProposalSubmitted,
    occurred_at: Utc::now(),
    actor: ActorRef { actor_type: ActorType::Gateway, actor_id: "ferrum-gateway", ... },
    intent_id: Some(record.intent_id),
    proposal_id: Some(record.proposal_id),
    execution_id: Some(record.execution_id),
    capability_id: Some(record.capability_id),
    ...
};
```

**MCP role**: Trigger the mint and authorize REST calls. Gateway handles all provenance emission internally. There is no `ExecutionAuthorized` provenance event kind.

---

## 10. dry_run Caveat

**`dry_run: bool`** in `AuthorizeExecutionRequest` controls the `ExecutionState` in the returned record:

| `dry_run` | `ExecutionState` | Other effects |
|-----------|-----------------|--------------|
| `false` (default) | `Prepared` | Capability consumed, record persisted, provenance emitted |
| `true` | `Authorized` | Capability consumed, record persisted, provenance emitted |

**IMPORTANT**: Both `dry_run: true` and `dry_run: false` consume the capability, create and persist the `ExecutionRecord`, and emit `ActionProposalSubmitted` provenance. The **only** difference is the `ExecutionState` field value (`Authorized` vs `Prepared`).

**Use case for `dry_run: true`**: Signals to later pipeline stages (prepare/execute) that this execution was pre-flight validated without yet entering the normal execution flow. Both states advance to subsequent stages; the distinction is advisory.

**MCP implementation**: Default to `dry_run: false` for normal flow. Set `dry_run: true` when caller explicitly requests pre-flight validation signaling.

---

## 11. Preflight Blockers (I5–I6)

### I5 — Scope Subset Validation

**Issue**: D1.4 requires I5 validation (resource_bindings ⊆ intent.resource_scope). MCP must thread `intent.resource_scope` from compile response to authorize request.

**Current State**:
- I5 is implemented in gateway at `authorize_execution` endpoint (doc 45)
- `validate_resource_bindings_subset_of_scope` exists in `server.rs`
- MCP/client must provide `intent.resource_scope` from compile response `envelope`
- No MCP-side handling yet

**Decision Required**:
1. Confirm that MCP threads `intent.resource_scope` from compile to mint/authorize flow?
2. Confirm that I5 failure returns `McpToolError::invalid_params()` with scope-violation reason?

**Recommendation**: Accept I5 as gateway-enforced invariant. MCP client threads `intent.resource_scope` through the flow. I5 failure surfaces as `403 Forbidden` with `PolicyDenied` → `McpToolError::forbidden()`.

**Dependencies**: D1.3.3 (compile) must be implemented first.

---

### I6 — Approval Binding Digest Validation

**Issue**: D1.4 requires I6 validation (approval_binding.digest == canonical_action_digest). MCP must compute canonical digest and include in `approval_binding` when minting capability for Critical-risk actions.

**Current State**:
- I6 is implemented in gateway at `authorize_execution` endpoint (doc 45, ADR-49)
- `canonical_action_digest()` method exists in `ActionProposal` (`execution.rs`)
- MCP/client does not yet compute digest or build `approval_binding`

**Decision Required**:
1. Confirm that MCP computes `canonical_action_digest()` from `ActionProposal` for Critical-risk proposals?
2. Confirm that `approval_binding` is built with `approval_id` + `approved_action_digest` when `estimated_risk == Critical`?
3. Confirm that `approval_binding = None` for non-Critical risk (I6 skipped)?

**Recommendation**: Accept I6 as gateway-enforced invariant with chain-validation. MCP client:
- For `estimated_risk == Critical`: build `ApprovalBinding { approval_id, approver_roles, approved_action_digest: computed_digest, expires_at }`
- For `estimated_risk != Critical`: set `approval_binding = None`
- I6 failures surface as `403 Forbidden` with `IntegrityMismatch` (digest chain failures) or `PolicyDenied` (approval state/expired) → `McpToolError::forbidden()`

**Dependencies**: D1.3.4 (evaluate) must be implemented first; approval system must exist.

---

## 12. D1.4 Implementation Plan (After Preflight)

If I5–I6 are approved, D1.4 implementation may proceed with:

### Allowed (Capability Mint + Authorize):
- Implement `POST /v1/capabilities/mint` using `FerrumGatewayClient`
- Implement `POST /v1/executions/authorize` using `FerrumGatewayClient`
- Parse `CapabilityMintResponse { lease, warnings }` fields
- Parse `AuthorizeExecutionResponse { execution, warnings }` fields
- Thread `intent.resource_scope` through mint → authorize flow (I5)
- Compute `canonical_action_digest()` for Critical-risk proposals (I6)
- Build `ApprovalBinding` when `estimated_risk == Critical`
- Handle `dry_run` parameter (default: false)
- Handle TTL validation (≤ 300s) locally before sending
- Handle single-use constraint (no capability reuse)
- Tests for capability mint flow (mocked responses)
- Tests for authorize flow with I5/I6 validation (mocked responses)

### Forbidden:
- Prepare/execute/verify/compensate calls (D1.5/D1.7 scope)
- Rollback preparation (D1.5 scope)
- Approval/reject tools (backend endpoint absent)
- Direct provenance emission (gateway owns emission)
- Tool dispatch wiring (D1.7 blocked by semantics)
- Claiming D1.5+ readiness

---

## 13. Test Strategy

### 13.1 Unit Tests
- `requested_ttl_secs` ≤ 300 enforced locally
- `canonical_action_digest()` produces deterministic 64-char SHA-256 hex
- `canonical_action_digest()` excludes correct fields (step_index, taint_inputs, metadata, created_at)
- `canonical_action_digest()` includes correct fields (intent_id, proposal_id, tool_name, server_name, raw_arguments, expected_effect, estimated_risk, requested_rollback_class)
- ApprovalBinding construction for Critical risk
- No ApprovalBinding for non-Critical risk

### 13.2 Integration Tests (Mocked Gateway)
- Mint capability with TTL ≤ 300 → Active lease returned
- Mint capability with TTL > 300 → 400 Bad Request (ValidationError)
- Mint capability for Critical risk with valid ApprovalBinding → Active lease
- Authorize with valid capability_id, dry_run=false → ExecutionRecord with state=Prepared, capability consumed
- Authorize with valid capability_id, dry_run=true → ExecutionRecord with state=Authorized, capability consumed (same as dry_run=false for capability consumption)
- Authorize with reused capability_id → 409 Conflict
- Authorize with expired capability → 400 Bad Request (CapabilityExpired)
- Authorize with I5 scope violation → 403 Forbidden (PolicyDenied)
- Authorize with I6 digest mismatch → 403 Forbidden (IntegrityMismatch)
- Authorize with I6 approval not found → 403 Forbidden (IntegrityMismatch)

### 13.3 Forbidden Test Cases
- Verify no prepare/execute/verify call is made during mint or authorize
- Verify no provenance event is emitted by MCP (gateway emits internally)
- Verify capability cannot be reused after authorize

---

## 14. Risk and Blocker List

| ID | Item | Severity | Status | Mitigation |
|----|------|----------|--------|------------|
| R1 | TTL validation not enforced locally | High | **OPEN** | Local validation before send |
| R2 | I5 scope threading not implemented | High | **OPEN** | Thread intent.resource_scope from compile |
| R3 | I6 digest computation not implemented | High | **OPEN** | Use ActionProposal::canonical_action_digest() |
| R4 | Single-use constraint not enforced | High | **OPEN** | Track used capability IDs in MCP state |
| R5 | dry_run semantics resolved | Medium | **RESOLVED** | Both values consume capability, persist record, emit provenance; only ExecutionState differs |
| R6 | No real gateway integration test yet | Medium | Open | D1.3.3/D1.3.4 flow exists; integration testable |
| R7 | D1.5+ blocked on D1.4 | Low | By design | Later slice |

---

## 15. Approval Criteria

For D1.4 preflight to be considered approved, the following must be true:

- [ ] I5: Scope threading (intent.resource_scope from compile → mint → authorize) is confirmed and documented
- [ ] I6: Canonical digest computation for Critical-risk proposals is confirmed and documented
- [ ] I7: dry_run semantics (both values consume capability, persist record, emit provenance; only ExecutionState differs: false->Prepared, true->Authorized) is confirmed and documented
- [ ] TTL ≤ 300 local validation is confirmed
- [ ] Single-use (no capability reuse) enforcement is confirmed
- [ ] Oracle signs off that capability-mint + authorize scope is correct (no prepare/execute/verify/compensate/rollback)
- [ ] Doc 81 updated with approval decisions in §Decision Log

---

## 16. Implementation Todo-List

After I5–I6 approval:

- [ ] Thread `intent.resource_scope` from compile response through to mint/authorize flow
- [ ] Implement `POST /v1/capabilities/mint` using `FerrumGatewayClient`
- [ ] Parse `CapabilityMintResponse { lease, warnings }` fields
- [ ] Extract `capability_id` from `lease.capability_id`
- [ ] Enforce TTL ≤ 300 locally before sending mint request
- [ ] Build `ApprovalBinding` for Critical-risk proposals using `canonical_action_digest()`
- [ ] Implement `POST /v1/executions/authorize` using `FerrumGatewayClient`
- [ ] Parse `AuthorizeExecutionResponse { execution, warnings }` fields
- [ ] Extract `execution_id` from `execution.execution_id`
- [ ] Handle `dry_run: true` → ExecutionRecord with state=Authorized (capability consumed, provenance emitted)
- [ ] Handle `dry_run: false` → ExecutionRecord with state=Prepared (capability consumed, provenance emitted)
- [ ] Track used `capability_id` to prevent reuse (single-use enforcement)
- [ ] Handle I5 failure (scope violation) → `403 Forbidden` with `PolicyDenied` → `McpToolError::forbidden()`
- [ ] Handle I6 failure (digest mismatch) → `403 Forbidden` with `IntegrityMismatch` → `McpToolError::forbidden()`
- [ ] Add unit tests for TTL validation
- [ ] Add unit tests for canonical_action_digest() (all included/excluded fields)
- [ ] Add integration tests with mocked gateway (mint + authorize flows)

---

## 17. Oracle Review Questions

Before I5–I6 can be approved, Oracle must confirm:

1. **I5 confirm**: Accept that MCP threads `intent.resource_scope` from compile response `envelope` through mint/authorize flow? Gateway enforces subset check; MCP client surfaces 403 (PolicyDenied) as `forbidden()`?

2. **I6 confirm**: Accept that MCP computes `canonical_action_digest()` from `ActionProposal` for Critical-risk proposals and includes `ApprovalBinding` in `CapabilityMintRequest`? For non-Critical risk, `approval_binding = None` (I6 skipped)? 403 surfaces as `forbidden()`?

3. **I7 confirm (dry_run)**: Accept that BOTH `dry_run: true` and `dry_run: false` consume the capability, create and persist the ExecutionRecord, and emit ActionProposalSubmitted provenance? Only difference is `ExecutionState` (Authorized vs Prepared)?

4. **Scope confirm**: Is capability-mint + authorize (no prepare/execute/verify/compensate/rollback) the correct D1.4 boundary? Or should any of those be included?

---

## 18. Decision Log

| Decision | Status | Alternatives | Current Recommendation |
| --- | --- | --- | --- |
| I5: scope threading | **Approved** | thread vs re-fetch | Thread `intent.resource_scope` from compile envelope through mint→authorize; gateway enforces subset; 403 PolicyDenied on failure |
| I6: digest computation | **Approved** | compute vs defer | MCP computes `canonical_action_digest()` for Critical risk; includes ApprovalBinding; gateway validates chain; 403 IntegrityMismatch/PolicyDenied on failure |
| I7: dry_run semantics | **Approved** | state-only vs no-state | Both dry_run=true and dry_run=false: consume capability, create/persist record, emit provenance; ONLY difference is ExecutionState (Authorized vs Prepared) |
| Scope: mint+authorize only | **Approved** | confirm vs expand | Confirm mint+authorize only; D1.5 for prepare; D1.7 for execute/verify/compensate |

---

## 19. Cross-References

- Doc 75 (Phase D-1 Stage 2 Plan): Original design for governance pipeline
- Doc 79 (D-1.3.3 preflight): Compile flow (D1.3.3 complete)
- Doc 80 (D-1.3.4 preflight): Evaluate flow (D1.3.4 preflight complete; low-level HTTP client implemented)
- Doc 74 (D-1 governance design): Original governance pipeline design
- Doc 49 (I6 ADR): Approval binding digest validation ADR
- `crates/ferrum-proto/src/capability.rs`: `CapabilityMintRequest`, `CapabilityMintResponse`, `CapabilityLease` definitions
- `crates/ferrum-proto/src/api.rs`: `AuthorizeExecutionRequest`, `AuthorizeExecutionResponse` definitions
- `crates/ferrum-proto/src/execution.rs`: `ActionProposal::canonical_action_digest()`, `ExecutionRecord` definition
- `crates/ferrum-gateway/src/server.rs`: `validate_resource_bindings_subset_of_scope`, `validate_approval_binding_digest`, mint and authorize endpoint implementations

---

## 20. Bottom Line

D1.4 is the capability-mint and authorize slice. Three preflight items (I5–I7) have been approved. The low-level HTTP client (`FerrumGatewayClient::mint_capability`, `FerrumGatewayClient::authorize_execution`) is implemented in `crates/ferrum-integrations-mcp/src/http_client.rs`. Tool dispatch, prepare, execute, verify, compensate, rollback remain in later slices (D1.5+). This packet does not enable D1.5+ behavior.

(End of file - total 589 lines)
