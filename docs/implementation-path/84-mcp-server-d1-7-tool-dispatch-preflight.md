# D1.7 MCP Server Tool Dispatch Preflight

## Status: IMPLEMENTED (per oracle verdict)

## Oracle Verdict Summary

Approved subset for lifecycle step tools as separate step tools (not atomic full pipeline):
- **APPROVED**: submit_intent, evaluate_intent, mint_capability, authorize_execution, prepare_execution, execute_prepared, verify, compensate
- **PERMANENTLY BLOCKED**: approve_intent, reject_intent (backend endpoints absent)

## Implemented Tools (D1.7 Wired)

| MCP Tool | REST Endpoint | HTTP Method | Notes |
|----------|---------------|-------------|-------|
| `ferrum_gate_submit_intent` | POST /v1/intents/compile | POST | Compiles intent and returns envelope |
| `ferrum_gate_evaluate_intent` | POST /v1/proposals/{id}/evaluate | POST | Evaluates proposal against policy |
| `ferrum_gate_mint_capability` | POST /v1/capabilities/mint | POST | Mints capability token |
| `ferrum_gate_authorize_execution` | POST /v1/executions/authorize | POST | Authorizes execution with capability |
| `ferrum_gate_prepare_execution` | POST /v1/executions/{id}/prepare | POST | Prepares execution with rollback contract |
| `ferrum_gate_execute_prepared` | POST /v1/executions/{id}/execute | POST | Executes prepared tool call |
| `ferrum_gate_verify` | POST /v1/executions/{id}/verify | POST | Verifies execution result |
| `ferrum_gate_compensate` | POST /v1/executions/{id}/compensate | POST | Compensates/rolls back execution |

## Blocked Tools (Backend Absent)

| MCP Tool | Status | Reason |
|----------|--------|--------|
| `ferrum_gate_approve_intent` | NOT_IMPLEMENTED | Backend endpoint absent |
| `ferrum_gate_reject_intent` | NOT_IMPLEMENTED | Backend endpoint absent |

## Architecture

### HTTP Client Layer (D1.3.3 - D1.6)
- Low-level HTTP client methods implemented in `http_client.rs`
- Methods: `compile_intent`, `evaluate_proposal`, `mint_capability`, `authorize_execution`, `prepare_execution`, `execute_execution`, `verify_execution`, `compensate_execution`
- Error handling: `GatewayError` with codes -32002 (auth), -32003 (unreachable), -32004 (server error)

### Tool Dispatch Layer (D1.7 - NOW IMPLEMENTED)
- `rest_mapper.rs` wires MCP tools to HTTP client methods
- Argument parsing and validation
- Response serialization
- No direct provenance emission (gateway-owned)
- No direct state management (gateway-owned)

## Key Design Decisions

### 1. Separate Step Tools (Per Oracle Verdict)
Each lifecycle step is a separate MCP tool, not an atomic full-pipeline tool. This allows:
- Fine-grained control over each step
- Gateway 409 enforcement at each step
- Better error handling per step

### 2. Gateway 409 Guards Enforce Sequence
The gateway returns HTTP 409 Conflict when:
- Attempting to skip prepare (UX risk, not security risk)
- Compensate called after verify/commit (invalid state transition)

This enforces proper sequencing without requiring MCP to track state.

### 3. No Direct Provenance Emission
Per design: MCP does not emit provenance events directly. The gateway:
- Emits `ActionProposalSubmitted` after submit_intent
- Emits `PolicyEvaluated` after evaluate_intent
- Emits `CapabilityMinted` after mint_capability
- Emits `ToolCallPrepared` after prepare_execution
- Emits `ToolCallExecuted` after execute_prepared
- Emits `SideEffectVerified` after verify
- Emits `SideEffectCommitted` or `SideEffectCompensated` as terminal events

### 4. Skip-Prepare is UX Risk, Not Security Risk
If a client attempts to execute without prepare, the gateway returns 409. This:
- Does not bypass any security controls
- Is enforced by gateway state machine
- Is a UX consideration (clients should follow sequence)

## Schema Validation

Each tool has strict input schemas enforced by argument parsing:

### submit_intent
- Required: `principal_id`, `title`, `goal`, `action_type`, `target`, `scope`
- Optional: `parameters`, `risk_tier`

### evaluate_intent
- Required: `proposal_id`, `intent_id`, `title`, `tool_name`, `server_name`, `arguments`, `expected_effect`, `estimated_risk`
- Optional: `rollback_class`

### mint_capability
- Required: `intent_id`, `proposal_id`, `tool_name`, `server_name`
- Optional: `resource_path`, `resource_mode`, `ttl_secs`

### authorize_execution
- Required: `proposal_id`, `capability_id`
- Optional: `dry_run` (default: false)

### prepare_execution
- Required: `execution_id`

### execute_prepared
- Required: `execution_id`
- Optional: `payload`

### verify
- Required: `execution_id`

### compensate
- Required: `execution_id`

## Test Coverage

- Tool registry: 17 tools (9 read-only + 8 lifecycle)
- Lifecycle tools correctly marked as `read_only=false`
- Blocked tools (approve/reject) not in registry
- Argument validation returns NOT_IMPLEMENTED for blocked tools
- Schema validation prevents missing required args

## Files Changed

- `crates/ferrum-integrations-mcp/src/lib.rs`: Added 8 lifecycle tool definitions, updated registry, updated constants
- `crates/ferrum-integrations-mcp/src/rest_mapper.rs`: Wired 8 lifecycle tool match arms, added `blocked` error variant
- `crates/ferrum-integrations-mcp/Cargo.toml`: Added `chrono` dependency

## References

- Doc 79: D1.3.3 compile preflight
- Doc 80: D1.3.4 evaluate preflight
- Doc 81: D1.4 capability/authorize preflight
- Doc 82: D1.5 prepare preflight
- Doc 83: D1.6 execute/verify/compensate preflight
