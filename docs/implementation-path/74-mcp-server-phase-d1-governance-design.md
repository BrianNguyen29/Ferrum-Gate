# 74 — MCP Server Phase D-1 Governance Pipeline Design

> **Status**: Design + partial implementation. D1.3–D1.8 and the D1.9 field-redaction work are implemented/reviewed; D1.9 rate limiting and full D1.10 smoke/load coverage remain open gaps. D-1 Slice 1 (auth gate + token-hash actor fallback) landed in commit `34d00ab`.
> **Purpose**: Detailed design for FerrumGate MCP server Phase D-1 — governance pipeline integration including auth, policy evaluation, capability issuance, rollback, and provenance. Also records implementation reality post-Slice 1.
> **Scope**: Phase D-1 = mutating tool execution through the full governance pipeline. Auth gate (Slice 1) is complete; remaining gaps are listed in §14.
> **Constraint**: Do not claim MCP server readiness. No production-ready claim. No mutating tool approval until governance smoke evidence is recorded.
> **Handoff from**: [`73-mcp-server-phase-d-implementation-plan.md`](73-mcp-server-phase-d-implementation-plan.md) (D-0 complete with local smoke evidence; D-1 design is next step)

---

## Explicit Non-Claims

- **No production-ready claim.** D-1 implementation is in progress; Slice 1 is complete but not production-hardened.
- **No G2 complete claim.** G2.1–G2.8 remain pending.
- **MCP server is post-v1 scope.** Phase D-1 is for v1.4 MCP Governance Beta.
- **No mutating tools approved for unattended use.** Mutating tools are wired and gated by auth, but governance smoke evidence is still missing.
- **D-0 smoke is local evidence only.** The D-0 local smoke evidence (`docs/implementation-path/artifacts/2026-05-06/73-d0-live-smoke-evidence.md`) is local dev evidence, not production, operator, or target-host evidence.

---

## 0. D-1 Overview

### 0.1 Why D-1 Design is Required

Phase D-0 implemented a read-only REST client for 9 MCP tools. Phase D-1 adds the governance pipeline integration required for mutating tool execution. This involves:

1. **Auth middleware**: Bearer token validation, ActorRef mapping
2. **Policy evaluation**: Every mutating tool must be evaluated against policy bundle
3. **Capability issuance**: Single-use capability with TTL ≤ 300s before execution
4. **Rollback preparation**: Rollback contract must be prepared before execution
5. **Provenance emission**: Events must be emitted for the full lineage chain
6. **Output sanitization**: Response must be sanitized before return to agent

Each of these integrates with core FerrumGate infrastructure and requires careful design to avoid bypassing governance invariants.

### 0.2 Core Invariants (Must Not Break)

D-1 design must preserve all FerrumGate invariants:

| Invariant | Description |
|-----------|-------------|
| **Intent-scoped execution** | Every action must be scoped to an intent |
| **Single-use capability** | Capabilities are single-use only; TTL ≤ 300s |
| **Provenance-first lineage** | Provenance events must be emitted before any side effect |
| **Rollback-by-default** | Rollback must be prepared before execution |
| **No gateway bypass** | All tool calls must go through governance pipeline |
| **No policy bypass** | Policy evaluation cannot be skipped |
| **No capability bypass** | Capability issuance cannot be skipped |
| **No provenance bypass** | Provenance emission cannot be skipped |
| **No rollback bypass** | Rollback preparation cannot be skipped |

### 0.3 D-0 to D-1 Handoff

D-0 (complete):
- Read-only REST client implemented
- 9 tools mapped to gateway REST endpoints
- Local smoke evidence documented in `73-d0-live-smoke-evidence.md`
- Only 3/9 tools smoke-tested (health, readyz_deep, policy_bundles)
- Auth disabled in dev config

D-1 (this document):
- Design for governance pipeline integration
- **D1.3–D1.10 implemented in code** (lifecycle tool dispatch, output sanitization, field-level redaction, full pipeline sequential test)
- **Slice 1 (auth gate) complete** — commit `34d00ab`: mutating tools require bearer token before REST dispatch; read-only tools preserve D-0 behavior
- Remaining gaps: per-agent rate limiting, full MCP→gateway governance smoke, out-of-order pipeline negative tests

---

## 1. Authentication Design

### 1.1 Auth Requirements

MCP clients must authenticate to FerrumGate to access protected endpoints. The design must address:

| Requirement | Description |
|-------------|-------------|
| **Bearer token validation** | Validate bearer token on every protected request |
| **Token source** | Use existing `FERRUMD_BEARER_TOKEN` or config-file bearer token |
| **ActorRef mapping** | Map MCP client identity to `ActorRef` for provenance |
| **Auth errors** | Return JSON-RPC error -32002 (AUTH_FAILED) for 401/403 |

### 1.2 Bearer Token vs OAuth Decision

| Option | Pros | Cons |
|--------|------|------|
| **Bearer token (recommended)** | Simple; matches existing ferrumd auth; aligns with MCP stdio transport | Token in Authorization header |
| **OAuth 2.0** | Industry standard; supports token rotation | Adds complexity; overkill for local MCP |

**Recommendation**: Use bearer token for D-1. OAuth can be considered in a future phase if enterprise integration is required.

### 1.3 ActorRef Mapping

When an MCP client authenticates, the bearer token must be mapped to a FerrumGate `ActorRef` for provenance tracking:

```rust
struct ActorIdentity {
    actor_type: ActorType,   // ActorType::Agent for MCP clients
    actor_id: String,        // See sourcing rules below
    display_name: Option<String>,
    auth_method: AuthMethod,  // AuthMethod::Bearer for MCP
}
```

**actor_id sourcing rules** (in priority order):

| Priority | Source | When Used |
|----------|--------|-----------|
| 1 | `FERRUMD_MCP_AGENT_ID` env var | If set, use this value directly |
| 2 | MCP initialize `client_info.name` | From MCP initialize params |
| 3 | SHA256(`FERRUM_GATEWAY_BEARER_TOKEN`)[0..12] | If token is configured; token itself is never exposed |
| 4 | Fallback `ferrum-mcp-local` | Default if nothing configured |

**JWT vs simple bearer tokens**:
- For **JWT bearer tokens**: `actor_id` MAY be extracted from the JWT `sub` claim if present
- For **simple hex bearer tokens**: There is no JWT `sub` claim; use the sourcing rules above
- **Do NOT assume JWT structure** unless the token is validated as a JWT

| Field | Source | Notes |
|-------|--------|-------|
| `actor_type` | Fixed | `ActorType::Agent` for all MCP clients |
| `actor_id` | See sourcing rules above | Never log the raw token |
| `display_name` | MCP initialize `client_info.name` | Optional; defaults to `actor_id` |
| `auth_method` | Fixed | `AuthMethod::Bearer` for MCP stdio/HTTP |

### 1.4 Auth Middleware Implementation

The auth middleware must:

1. Extract bearer token from `Authorization` header or environment
2. Validate token against configured bearer token
3. Map to `ActorRef`
4. Store `ActorRef` in request context for downstream use
5. Return -32002 (AUTH_FAILED) for invalid/missing token

---

## 2. Mutating Tools Design

### 2.1 Mutating Tool List

The following tools are mutating and require governance pipeline:

| Tool | Description | Risk |
|------|-------------|------|
| `ferrum_gate_submit_intent` | Submit a new intent | Medium |
| `ferrum_gate_evaluate_intent` | Evaluate intent against policies | Medium |
| `ferrum_gate_prepare_execution` | Prepare execution with rollback | Medium |
| `ferrum_gate_execute_prepared` | Execute prepared action | High |
| `ferrum_gate_compensate` | Rollback an execution | High |
| `ferrum_gate_approve_intent` | Approve a pending intent | Medium |
| `ferrum_gate_reject_intent` | Reject a pending intent | Medium |

### 2.2 Mutating Tool Auth Gate (Slice 1)

**Critical**: Mutating tools are wired to gateway REST endpoints but require a configured bearer token before dispatch (fail-closed). Read-only tools bypass the MCP-server auth gate; gateway REST still enforces auth where applicable.

Unknown tools return -32601 (METHOD_NOT_FOUND). Unimplemented tools return -32001 (NOT_IMPLEMENTED).

The tool registry separates:

```rust
pub const READ_ONLY_TOOLS: &[&str] = &[
    "ferrum_gate_health",
    "ferrum_gate_readyz_deep",
    "ferrum_gate_list_intents",
    "ferrum_gate_get_execution",
    "ferrum_gate_query_lineage",
    "ferrum_gate_list_approvals",
    "ferrum_gate_list_policy_bundles",
    "ferrum_gate_list_bridges",
    "ferrum_gate_list_bridge_tools",
];

pub const MUTATING_TOOLS: &[&str] = &[
    "ferrum_gate_submit_intent",
    "ferrum_gate_evaluate_intent",
    "ferrum_gate_mint_capability",
    "ferrum_gate_authorize_execution",
    "ferrum_gate_prepare_execution",
    "ferrum_gate_execute_prepared",
    "ferrum_gate_verify",
    "ferrum_gate_compensate",
    "ferrum_gate_approve_intent",
    "ferrum_gate_reject_intent",
];
```

---

## 3. Governance Pipeline Design

### 3.1 Pipeline Flow

Each mutating MCP tool call must go through the full governance pipeline in this exact order:

```
MCP tools/call request
        │
        ▼
┌───────────────────┐
│ 1. Auth Middleware │ ← Validate bearer token, map to ActorRef
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 2. Tool Registry  │ ← Default-deny unknown/mutating tools
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 3. Compile Intent │ ← Map MCP call to ActionProposal (not yet submitted)
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 4. Policy Eval    │ ← Evaluate ActionProposal against policy bundle
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 5. Capability Mint │ ← Mint single-use capability TTL≤300s
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 6. Authorize      │ ← Verify capability covers scope of call
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 7. Prepare Rollback│ ← Prepare rollback contract before execution
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 8. Provenance     │ ← Emit ToolCallPrepared event
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 9. Execute        │ ← Call adapter or REST endpoint
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 10. Verify         │ ← Verify side effect completed
└───────────────────┘
        │
        ▼
┌───────────────────┐
│ 11. Sanitize       │ ← Sanitize response before return
└───────────────────┘
        │
        ▼
MCP CallToolResult response
```

**Note**: Steps 1-6 do NOT make side effects. Step 8 (Execute) is the first side effect. Steps 9-11 verify and sanitize.

### 3.2 Architecture Decision: REST Calls to ferrumd

D-1 uses **REST calls to ferrumd** for governance steps (Option A per doc 71). This preserves the MCP-to-REST adapter pattern established in Phase D-0:

| Governance Step | Implementation |
|----------------|---------------|
| Auth Middleware | Validate bearer token presence in `FerrumGatewayClient`; no ferrumd call needed |
| Tool Registry | Local check only |
| Compile Intent | POST /v1/intents/compile |
| Policy Eval | POST /v1/proposals/{proposal_id}/evaluate |
| Capability Mint | POST /v1/capabilities/mint |
| Authorize | POST /v1/executions/authorize |
| Prepare Rollback | POST /v1/executions/{execution_id}/prepare |
| Provenance | Gateway-owned (ferrum-ledger) |
| Execute | POST /v1/executions/{execution_id}/execute |
| Verify | POST /v1/executions/{execution_id}/verify |
| Compensate | POST /v1/executions/{execution_id}/compensate |
| Approve/Reject | POST /v1/approvals/{approval_id}/resolve |
| Sanitize | Local (ferrum-firewall + field-key redaction) |

This approach keeps governance logic in ferrumd and keeps `ferrum-mcp-server` as a thin adapter.

### 3.3 Intent/Action Proposal Mapping

MCP tool calls must be mapped to FerrumGate `ActionProposal` before policy evaluation:

```rust
struct ToolCallAction {
    intent_id: Uuid,           // Must be associated with an intent
    action_type: ActionType,  // e.g., ActionType::McpToolMutation
    scope: Scope,              // Narrow scope of the action
    target: Target,           // What to act upon (bridge, adapter, etc.)
    parameters: JsonValue,    // Tool arguments
}
```

| MCP Tool | ActionType | Notes |
|----------|------------|-------|
| `submit_intent` | `SubmitIntent` | Creates new intent |
| `evaluate_intent` | `EvaluateIntent` | Policy evaluation only |
| `prepare_execution` | `PrepareExecution` | Rollback prepare |
| `execute_prepared` | `ExecutePrepared` | Actual execution |
| `compensate` | `Compensate` | Rollback |

### 3.3 Policy Evaluation Gate

Before any mutating tool executes, the policy engine (`ferrum-pdp`) must evaluate:

1. **Who**: The `ActorRef` from auth middleware
2. **What**: The tool being called and its arguments
3. **Scope**: The scope of the action (what resources it affects)
4. **Policy**: The active policy bundle

```rust
async fn evaluate_policy(
    actor: &ActorRef,
    tool_name: &str,
    arguments: &JsonValue,
    scope: &Scope,
) -> Result<PolicyDecision, PolicyError> {
    // Returns Allow or Deny with reason
}
```

**Default behavior**: If no policy rule matches, deny the tool call.

### 3.4 Capability Issuance Gate

After policy evaluation passes, a single-use capability must be minted:

```rust
struct CapabilityGrant {
    capability_id: Uuid,
    actor: ActorRef,
    scope: Scope,
    ttl_seconds: u32,     // Must be ≤ 300
    single_use: bool,     // Always true
    expires_at: DateTime<Utc>,
}
```

| Requirement | Value |
|-------------|-------|
| TTL max | 300 seconds |
| Single-use | Always |
| Scope | Must match tool call scope |

### 3.5 Rollback Preparation Gate

Before tool execution, rollback must be prepared:

```rust
async fn prepare_rollback(
    action: &ToolCallAction,
    capability: &CapabilityGrant,
) -> Result<RollbackContract, RollbackError> {
    // Creates compensation actions
    // Stores contract in ferrum-rollback
}
```

The rollback contract must be stored before execution proceeds.

### 3.6 Provenance Emission Gate

Provenance events must be emitted in order:

| Event | When | Purpose |
|-------|------|---------|
| `ActionProposalSubmitted` | Before policy eval | Lineage start |
| `PolicyEvaluated` | After policy eval | Decision record |
| `CapabilityMinted` | After capability issuance | Authorization record |
| `ToolCallPrepared` | After rollback prep | Execution record |
| `ToolCallExecuted` | After execution | Completion record |
| `SideEffectPrepared` | Before side effect | Effect record |
| `SideEffectVerified` | After effect verification | Verification record |
| `SideEffectCommitted` | Final | Committed record |
| `SideEffectCompensated` | If rolled back | Compensation record |

### 3.7 Output Sanitization Gate

Before returning to the MCP client, output must be sanitized:

```rust
async fn sanitize_output(
    output: JsonValue,
    actor: &ActorRef,
) -> JsonValue {
    // Remove sensitive fields
    // Redact based on actor permissions
    // Use ferrum-firewall
}
```

---

## 4. Sync vs Async Tool Calls

### 4.1 Decision: Pending/Polling for Long-Running Calls

Governed tool calls may take time (policy evaluation, rollback preparation, adapter execution). The MCP client should not block waiting for completion.

**Recommendation**: Use a **pending/polling** model for tool execution:

1. **Immediate return**: `tools/call` returns immediately with a `pending` status and `call_id`
2. **Polling endpoint**: Client polls `GET /v1/executions/{call_id}` for status
3. **Completion**: When complete, polling returns the result or error

### 4.2 MCP Response for Pending Call

```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"status\": \"pending\", \"call_id\": \"uuid-here\"}"
    }],
    "is_error": false
  },
  "id": 1
}
```

### 4.3 Polling Sequence

```
1. tools/call → returns {status: "pending", call_id: "..."}
2. GET /v1/executions/{call_id} → returns {status: "running"}
3. GET /v1/executions/{call_id} → returns {status: "completed", result: {...}}
```

---

## 5. Per-Agent Rate Limiting

### 5.1 Rate Limit Requirements

| Limit | Default | Configurable |
|-------|---------|--------------|
| Requests per second | 2 req/s | Yes |
| Burst | 50 | Yes |

Rate limiting uses `tower_governor` with per-ActorRef enforcement.

### 5.2 Implementation

```rust
async fn rate_limit(actor: &ActorRef) -> Result<(), RateLimitError> {
    // Check rate limit for actor
    // If exceeded, return -32005 (Rate limited)
}
```

---

## 6. Error Handling

### 6.1 Error Code Summary

| Code | Name | When |
|------|------|------|
| -32001 | NOT_IMPLEMENTED | Tool not yet implemented |
| -32002 | AUTH_FAILED | 401/403 from gateway |
| -32003 | GATEWAY_UNREACHABLE | Connection failed |
| -32004 | GATEWAY_SERVER_ERROR | 4xx/5xx from gateway |
| -32005 | RATE_LIMITED | Per-agent rate limit exceeded |
| -32601 | METHOD_NOT_FOUND | Unknown tool |
| -32602 | INVALID_PARAMS | Missing/invalid arguments |

### 6.2 Error Response Format

All errors use JSON-RPC error format:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32002,
    "message": "Authentication failed",
    "data": {
      "detail": "Bearer token invalid or missing"
    }
  },
  "id": 1
}
```

---

## 7. D-1 Ordered Implementation Todo-List

### Staged Implementation Order

D-1 implementation is split into stages to allow safe progression while preserving design review gates:

| Stage | Items | Status | Notes |
|-------|-------|--------|-------|
| **Stage 1** | D-1.1 (Auth) + D-1.2 (Tool Registry) | **IMPLEMENTED** | Slice 1 — commit `34d00ab` |
| **Stage 2** | D-1.3–D-1.7 (Policy/Cap/Rollback/Provenance/Execute) | **IMPLEMENTED** | All lifecycle tools wired; sequential pipeline test passes |
| **Stage 3** | D-1.8–D-1.9 (Sanitize/RateLimit) | **PARTIAL** | D1.8–D1.8.3 sanitization + D1.9 field-key redaction implemented; rate limiting NOT implemented |
| **Stage 4** | D-1.10 (Integration/Load) | **PARTIAL** | Sequential lifecycle integration test passes; load testing and full governance smoke still missing |

**Slice 1 Rationale**: Auth gate validates bearer token presence before REST dispatch for mutating tools. Read-only tools bypass the gate. Token-hash actor fallback provides deterministic identity without exposing secrets.

**Remaining gaps**: See §14 Known Gaps.

### Phase D-1.1: Auth Middleware

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.1.1 | Define `ActorIdentity` struct | **Implemented** | `ActorIdentity` + `ActorSource` enum in `lib.rs` |
| D1.1.2 | Implement bearer token extraction | **Implemented** | `FerrumGatewayClient` reads `FERRUM_GATEWAY_BEARER_TOKEN` via `ClientConfig::from_env()` |
| D1.1.3 | Implement token validation | **Implemented** | Presence check via `client.has_auth()`; gateway validates actual token |
| D1.1.4 | Implement ActorRef mapping | **Implemented** | `ActorIdentity::resolve()` with env → client_info → token_hash → local precedence |
| D1.1.5 | Add auth tests | **Implemented** | `test_mutating_tool_auth_gate_*`, `test_actor_identity_*` |
| D1.1.6 | Consolidate error_codes | **Implemented** | `error_codes` module in `lib.rs` |

### Phase D-1.2: Mutating Tool Registry

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.2.1 | Define `MUTATING_TOOLS` constant | **Implemented** | 10 tools: 8 lifecycle + 2 approval |
| D1.2.2 | Update tool registry | **Implemented** | All 19 tools registered (9 read-only + 10 mutating) |
| D1.2.3 | Default-deny unknown tools | **Implemented** | Returns -32601 (METHOD_NOT_FOUND) |
| D1.2.4 | Default-deny unimplemented mutating | **Implemented** | Returns -32001 (NOT_IMPLEMENTED) for blocked tools |
| D1.2.5 | Add registry tests | **Implemented** | `test_mutating_tools_set_contains_expected_tools`, `test_approval_tools_*` |

### Phase D-1.3: Policy Evaluation Integration

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.3.1 | Define `ToolCallAction` struct | **Implemented** | `stage2_types::ToolCallAction` in `lib.rs` |
| D1.3.2 | Integrate with `ferrum-pdp` | **Implemented** | Gateway evaluates via `POST /v1/proposals/{id}/evaluate` |
| D1.3.3 | Implement default-deny on no match | **Implemented** | Gateway-owned; MCP adapter dispatches |
| D1.3.4 | Add policy eval tests | **Implemented** | `test_evaluate_proposal_*` in `http_client.rs` |

### Phase D-1.4: Capability Issuance

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.4.1 | Integrate with `ferrum-cap` | **Implemented** | `POST /v1/capabilities/mint` wired in `rest_mapper.rs` |
| D1.4.2 | Enforce TTL ≤ 300s | **Implemented** | `ttl_secs.min(300)` in `call_mint_capability` |
| D1.4.3 | Enforce single-use | **Implemented** | Gateway-owned (`ferrum-cap`) |
| D1.4.4 | Add capability tests | **Implemented** | `test_mint_capability_*` in `http_client.rs` |

### Phase D-1.5: Rollback Preparation

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.5.1 | Integrate with `ferrum-rollback` | **Implemented** | `POST /v1/executions/{id}/prepare` wired in `rest_mapper.rs` |
| D1.5.2 | Store rollback contract | **Implemented** | Gateway-owned (`ferrum-rollback`) |
| D1.5.3 | Add rollback tests | **Implemented** | `test_prepare_execution_*` in `http_client.rs` |

### Phase D-1.6: Provenance Emission

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.6.1 | Emit `ActionProposalSubmitted` | **Gateway-owned** | `ferrum-ledger` emits during gateway processing |
| D1.6.2 | Emit `PolicyEvaluated` | **Gateway-owned** | `ferrum-ledger` emits during gateway processing |
| D1.6.3 | Emit `CapabilityMinted` | **Gateway-owned** | `ferrum-ledger` emits during gateway processing |
| D1.6.4 | Emit `ToolCallPrepared` | **Gateway-owned** | `ferrum-ledger` emits during gateway processing |
| D1.6.5 | Emit `ToolCallExecuted` | **Gateway-owned** | `ferrum-ledger` emits during gateway processing |
| D1.6.6 | Add provenance tests | **Implemented** | `test_query_lineage` for read-only query; full pipeline provenance is gateway-owned |

### Phase D-1.7: Tool Execution

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.7.1 | Implement `submit_intent` | **Implemented** | `POST /v1/intents/compile` |
| D1.7.2 | Implement `evaluate_intent` | **Implemented** | `POST /v1/proposals/{id}/evaluate` |
| D1.7.3 | Implement `prepare_execution` | **Implemented** | `POST /v1/executions/{id}/prepare` |
| D1.7.4 | Implement `execute_prepared` | **Implemented** | `POST /v1/executions/{id}/execute` |
| D1.7.5 | Implement `compensate` | **Implemented** | `POST /v1/executions/{id}/compensate` |
| D1.7.6 | Add execution tests | **Implemented** | `test_d1_10_full_lifecycle_sequential` covers all 8 steps end-to-end |

### Phase D-1.8: Output Sanitization

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.8.1 | Integrate with `ferrum-firewall` | **Implemented** | `TaintScoringFirewall::sanitize_output()` strips control chars at `handle_tools_call_with_client` boundary |
| D1.8.2 | Redact sensitive fields | **Implemented** | D1.9 Phase 1+2 field-key redaction (`raw_arguments`, `metadata`, `target`, `trust_context`, etc.) |
| D1.8.3 | Add sanitization tests | **Implemented** | `test_sanitize_output_*`, `test_redact_*`, `test_d1_9_3_*` boundary tests |

### Phase D-1.9: Rate Limiting

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.9.1 | Integrate with `tower_governor` | **Not implemented** | Deferred to future slice |
| D1.9.2 | Configurable limits | **Not implemented** | Deferred to future slice |
| D1.9.3 | Add rate limit tests | **Not implemented** | Deferred to future slice |

### Phase D-1.10: Integration and Testing

| # | Item | Status | Notes |
|---|------|--------|-------|
| D1.10.1 | End-to-end integration test | **Implemented** | `test_d1_10_full_lifecycle_sequential` (mock-based, 8-step pipeline) |
| D1.10.2 | Load testing | **Not implemented** | Deferred to future slice |
| D1.10.3 | Smoke test all mutating tools | **Not implemented** | Full MCP→gateway governance smoke still missing |

---

## 8. Acceptance Criteria

### 8.1 D-1 Design Gate (Completed)

The design review is complete. The following were satisfied:

| Criterion | Verification |
|-----------|--------------|
| Design document reviewed | Approved |
| Core invariants preserved | Pipeline order respects intent-scoped, single-use, provenance-first, rollback-by-default |
| Default-deny confirmed | Mutating tools fail closed when no auth configured |
| Pipeline order corrected | Auth → Registry → Compile → PolicyEval → CapMint → Authorize → PrepareRollback → Provenance → Execute → Verify → Sanitize |
| REST architecture confirmed | D-1 uses REST calls to ferrumd for governance (not embedded) |
| actor_id sourcing clarified | FERRUMD_MCP_AGENT_ID / MCP init / token-hash / fallback |
| Non-claims confirmed | Explicit non-claims in document |

### 8.2 Slice 1 Acceptance Criteria (D-1.1 + D-1.2) — COMPLETE

Slice 1 is complete. Verified by tests:

| Criterion | Test Assertion |
|-----------|---------------|
| Missing bearer token returns -32002 | `test_mutating_tool_auth_gate_fails_closed_without_auth` |
| Mutating tool with auth dispatches | `test_mutating_tool_auth_gate_allows_with_auth` |
| Read-only tools bypass auth gate | `test_read_only_tool_bypasses_auth_gate` |
| actor_id set from env or init | `test_actor_identity_resolve_precedence_with_token_hash` |
| Token-hash fallback works | `test_actor_identity_from_token_hash` |
| Token not exposed in actor_id | `test_actor_identity_token_hash_does_not_expose_token` |

### 8.3 D-1.3–D-1.8 Acceptance Criteria — COMPLETE

All lifecycle tools, sanitization, and field-key redaction are implemented and tested:

| Criterion | Verification |
|-----------|--------------|
| Sequential 8-step pipeline test passes | `test_d1_10_full_lifecycle_sequential` |
| Output sanitization strips control chars | `test_sanitize_output_strips_control_chars` |
| Field-level redaction works | `test_redact_*` (20+ tests) |
| Boundary integration tests pass | `test_d1_9_3_*`, `test_d1_10_*` |

### 8.4 D-1 Implementation Gate (Remaining)

Before any mutating tool is enabled in production:

| Criterion | Verification |
|-----------|--------------|
| Unit tests pass | `cargo test -p ferrum-integrations-mcp` (217 tests passing) |
| Integration tests pass | End-to-end MCP → governance → rollback test |
| Full governance smoke evidence | Recorded in `artifacts/` |
| Load testing complete | Performance acceptable under load |
| Security review | Auth, rate limiting, output sanitization reviewed |
| Operator approval | Operator signs off on governance pipeline |

---

## 9. Risks and Mitigations

### 9.1 Design Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Auth bypass** | Low | Critical | Default-deny; explicit auth checks |
| **Policy bypass** | Low | Critical | Policy evaluation is mandatory gate |
| **Capability bypass** | Low | Critical | Capability issuance is mandatory gate |
| **Rollback bypass** | Low | Critical | Rollback prep is mandatory gate |
| **Provenance bypass** | Low | Critical | Provenance emission is mandatory gate |
| **Secret leakage** | Medium | High | Output sanitization; no secret logging |
| **Rate limit abuse** | Medium | Medium | Per-agent rate limiting |

### 9.2 Implementation Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Response shape mismatch** | Medium | Medium | Match to actual ferrumd OpenAPI schema |
| **Timeout issues** | Medium | Medium | Polling model for long-running calls |
| **Rollback complexity** | High | High | Careful design; integration tests |

---

## 10. Open Questions

### 10.1 Auth

1. **Token validation**: Should we validate token expiry, or is that handled by the bearer token mechanism?
2. **Token rotation**: Should D-1 support token rotation, or is that a future phase?

### 10.2 Policy

1. **Policy bundle selection**: How does the MCP client specify which policy bundle to use?
2. **Policy cache**: Should policy evaluation results be cached to reduce latency?

### 10.3 Execution

1. **Long-running calls**: Is the pending/polling model acceptable, or is there a blocking model preference?
2. **Parallel execution**: Can multiple pending calls be in flight simultaneously for the same actor?

### 10.4 Rollback

1. **Compensation ordering**: If a multi-step action partially succeeds, what is the compensation order?
2. **Compensation timeout**: What happens if compensation itself fails?

---

## 11. Non-Goals

The following are explicitly **NOT** in scope for D-1:

| Item | Reason |
|------|--------|
| **Adapter-backed tools** | fs, git, http, sqlite tools are separate phase |
| **Streamable HTTP transport** | Deferred from Phase C |
| **OAuth 2.0** | Future consideration; bearer token sufficient for D-1 |
| **Production hardening** | Rate limit stress testing, load testing are MVP 3 |
| **Multi-node/multi-region** | Out of v1 scope |
| **PostgreSQL support** | Path 3 item |

---

## 12. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`73-mcp-server-phase-d-implementation-plan.md`](73-mcp-server-phase-d-implementation-plan.md) | D-0 complete; D-1 deferred |
| This doc | [`72-mcp-server-phase-a-implementation-plan.md`](72-mcp-server-phase-a-implementation-plan.md) | Phases A, B, C complete |
| This doc | [`71-mcp-server-feasibility-and-design.md`](71-mcp-server-feasibility-and-design.md) | Feasibility and design basis |
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope |
| This doc | [`artifacts/2026-05-06/73-d0-live-smoke-evidence.md`](artifacts/2026-05-06/73-d0-live-smoke-evidence.md) | D-0 local smoke evidence |
| This doc | [`README.md`](README.md) | Reading order entry |

---

## 14. Known Gaps (Post-Slice 3)

The following items are explicitly **not yet implemented** and remain tracked:

| Gap | Impact | Owner | Blocker |
|-----|--------|-------|---------|
| **Per-agent rate limiting** | Medium | Future slice | tower_governor integration deferred |
| **Full MCP→gateway governance smoke** | High | Operator | Requires live gateway with auth enabled |
| **Real-gateway state-machine negative tests** | Medium | Operator | Out-of-order mock tests exist (Slice 3); real gateway state-machine validation still missing |
| **Load testing** | Medium | Future slice | Performance baseline not yet established |
| **Token rotation** | Low | Future slice | Not required for v1.4 beta |
| **OAuth 2.0** | Low | Future slice | Bearer token sufficient for current scope |

**Slice 3 scope clarification**: Mock-level out-of-order pipeline negative tests are now implemented (7 tests covering execute-before-prepare, verify-before-execute, prepare-without-auth, compensate-before-verify, mint-on-denied-proposal, approve-nonexistent, and error-message-propagation). These verify MCP→gateway error mapping but do not validate the actual gateway state machine. Full real-gateway negative smoke remains a gap.

---

## 13. Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| Bearer token over OAuth | 2026-05-06 | Simple; matches existing ferrumd auth |
| Pending/polling for long calls | 2026-05-06 | Avoids blocking; allows async governance |
| Default-deny mutating tools | 2026-05-06 | Fail-closed; no accidental exposure |
| TTL ≤ 300s for capabilities | 2026-05-06 | Per AGENTS.md invariant |
| REST calls to ferrumd for governance | 2026-05-06 | Preserves MCP-to-REST Option A pattern |
| actor_id from FERRUMD_MCP_AGENT_ID or MCP init | 2026-05-06 | Simple hex tokens have no JWT sub claim |
| Staged implementation | 2026-05-06 | D-1.1/D-1.2 safe to proceed; D-1.3+ gated |
| Error codes consolidated in lib.rs | 2026-05-06 | Implementation cleanup in D-1.1 |
| **D-1 Slice 1 auth gate implemented** | **2026-05-16** | **Commit `34d00ab`: mutating tools require bearer token before REST dispatch; read-only bypass; token-hash actor fallback** |
| **D1.3–D1.10 marked implemented** | **2026-05-16** | **Code reality: lifecycle dispatch, sanitization, redaction, sequential pipeline test all landed and reviewed** |
| **D-1 Slice 3 negative tests implemented** | **2026-05-16** | **7 mock-based out-of-order pipeline tests added; verify gateway 409/400/404/422 errors map to MCP GATEWAY_SERVER_ERROR; real-gateway state-machine smoke still tracked as gap** |

---

*Document created: 2026-05-06. Last updated: 2026-05-16. D-1 Slice 1 implemented; remaining gaps tracked in §14. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
