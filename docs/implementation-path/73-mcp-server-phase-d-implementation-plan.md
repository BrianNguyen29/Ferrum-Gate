# 73 — MCP Server Phase D Implementation Plan

> **Status**: Phase D-0 (read-only REST client) is implemented. D-1 (mutating governance pipeline) remains deferred and requires separate design.
> **Purpose**: Detailed implementation checklist and execution tracker for FerrumGate MCP server Phase D — gateway REST API integration for read-only tools.
> **Scope**: Phase D-0 = read-only REST client that maps 9 MCP tools to gateway REST routes. Phase D-1 = mutating governance pipeline (deferred, requires separate design).
> **Constraint**: Do not claim MCP server readiness. Phase D-0 produces read-only REST client only; no mutating tool execution, no capability issuance, no provenance emission, no rollback integration.
> **Handoff from**: [`72-mcp-server-phase-a-implementation-plan.md`](72-mcp-server-phase-a-implementation-plan.md) (Phases A, B, C complete)

---

## Explicit Non-Claims

- **No MCP server readiness claim.** Phase D-0 is a read-only REST client only; tool execution, auth, provenance, and rollback are out of scope.
- **No production-ready claim.** Phase D-0 is implemented but not production-ready.
- **No G2 complete claim.** G2.1–G2.8 remain pending.
- **MCP server is post-v1 scope.** Phase D is planning for v1.4 MCP Governance Beta.
- **No mutating tools in Phase D-0.** Only read-only REST API calls; no POST/PUT/DELETE to ferrumd.
- **No governance pipeline in Phase D-0.** Policy evaluation, capability issuance, provenance emission, and rollback are Phase D-1.
- **Phase D-1 is deferred.** Mutating tool execution requires separate design before implementation.

---

## 0. Phase D Split Overview

### 0.1 Why Split Phase D?

Phase D was originally designed as a single phase covering all gateway/governance integration. However, the work naturally divides into two distinct parts with different risk profiles and dependencies:

| Sub-phase | Focus | Risk | Dependencies | Tools |
|-----------|-------|------|--------------|-------|
| **D-0** | Read-only REST client | Low | None (standalone HTTP client) | 9 read-only tools only |
| **D-1** | Mutating governance pipeline | High | Full policy engine, capability system, provenance, rollback | All mutating tools |

### 0.2 Phase D-0: Read-Only REST Client

D-0 adds the HTTP client that maps MCP `tools/call` to read-only FerrumGate REST API endpoints. It does **not** include:
- Auth middleware (bearer token validation)
- Policy evaluation
- Capability issuance
- Provenance emission
- Rollback preparation

D-0 acceptance criteria:
1. All 9 read-only tools call the correct REST endpoint and return parsed responses
2. HTTP error responses from gateway are mapped to appropriate MCP error codes
3. Auth errors (401/403 from gateway) are distinguished from gateway-unreachable errors (connection refused, timeout)
4. No secrets are logged (bearer tokens, response bodies containing sensitive data)
5. `cargo test -p ferrum-integrations-mcp` passes
6. `cargo check --workspace` passes
7. No secret values appear in debug output (use `Debug` fmt for sensitive structs, not `Display`)

### 0.3 Phase D-1: Deferred Governance Pipeline

D-1 covers mutating tool execution through the full governance pipeline. It is **deferred** and requires separate design because:

1. **Auth middleware**: Bearer token validation, ActorRef mapping
2. **Policy evaluation**: Every mutating tool must be evaluated against policy bundle
3. **Capability issuance**: Single-use capability with TTL ≤ 300s before execution
4. **Rollback preparation**: Rollback contract must be prepared before execution
5. **Provenance emission**: Events must be emitted for the full lineage chain
6. **Output sanitization**: Response must be sanitized before return to agent

D-1 requires a separate design document before implementation begins.

---

## 1. Current State

### 1.1 What Exists After Phase C

As of Phase C complete (commit `d319a67`):
- `crates/ferrum-integrations-mcp` crate exists with read-only tool registry (9 tools)
- JSON-RPC 2.0 request/response types and handler stubs exist
- `ferrum-mcp-server` binary with stdio transport exists
- `tools/call` returns NOT_IMPLEMENTED for all tools
- No HTTP client exists
- No REST API calls are made

### 1.2 What Phase D-0 Adds

Phase D-0 adds:
1. HTTP client module (`src/http_client.rs`)
2. REST endpoint mapper (`src/rest_mapper.rs`)
3. Response parser for each of the 9 read-only tools
4. Error classification (auth error vs. gateway unreachable vs. server error)
5. `handle_tools_call` implementation for read-only tools (returns parsed response, NOT NOT_IMPLEMENTED)

### 1.3 Phase Sequencing

```
Doc 71 (Feasibility & Design)
    │
    ▼
Phase A (skeleton) ✅ COMPLETE
    │
    ▼
Phase B (JSON-RPC handlers) ✅ COMPLETE
    │
    ▼
Phase C (stdio transport + binary) ✅ COMPLETE
    │
    ▼
Phase D-0 (read-only REST client) ✅ COMPLETE
    │
    ▼
Phase D-1 (mutating governance pipeline) ☐ DEFERRED — requires separate design
    │
    ▼
MVP 0 → 1 → 2 → 3
```

---

## 2. Tool-to-REST Route Mapping

### 2.1 Corrected Route Table

The following table shows the 9 read-only MCP tools and their corresponding FerrumGate REST API endpoints. **Note**: The route for `ferrum_gate_list_policy_bundles` was incorrectly documented in Phase A as `/v1/policies`; the correct route is `/v1/policy-bundles`.

| # | MCP Tool | REST Method | REST Endpoint | Description |
|---|----------|-------------|---------------|-------------|
| 1 | `ferrum_gate_health` | GET | `/v1/healthz` | Health probe |
| 2 | `ferrum_gate_readyz_deep` | GET | `/v1/readyz/deep` | Deep readiness probe |
| 3 | `ferrum_gate_list_intents` | GET | `/v1/intents` | List intents with optional filters |
| 4 | `ferrum_gate_get_execution` | GET | `/v1/executions/{execution_id}` | Get execution status by ID |
| 5 | `ferrum_gate_query_lineage` | GET | `/v1/provenance/query` | Query provenance events |
| 6 | `ferrum_gate_list_approvals` | GET | `/v1/approvals` | List pending approvals |
| 7 | `ferrum_gate_list_policy_bundles` | GET | `/v1/policy-bundles` | List available policy bundles |
| 8 | `ferrum_gate_list_bridges` | GET | `/v1/bridges` | List registered runtime bridges |
| 9 | `ferrum_gate_list_bridge_tools` | GET | `/v1/bridges/{bridge_id}/tools` | List tools for a specific bridge |

### 2.2 Route Correction Log

| Date | Bug | Correction |
|------|-----|------------|
| 2026-05-06 | Doc 72 line 175 listed `ferrum_gate_list_policy_bundles` → `GET /v1/policies` | Corrected to `GET /v1/policy-bundles` per `ferrum-gateway/src/server.rs` |

### 2.3 Response Shapes

Each tool maps to a specific response shape. The HTTP client must parse the JSON response into the correct type.

| MCP Tool | Response Shape | Notes |
|----------|----------------|-------|
| `ferrum_gate_health` | `HealthResponse` | `{ "status": "ok" }` |
| `ferrum_gate_readyz_deep` | `DeepHealthResponse` | `{ "status": "ok", "store_healthy": true, "write_queue_depth": 0 }` |
| `ferrum_gate_list_intents` | `IntentsListResponse` | `{ "intents": [...], "cursor": "..." }` |
| `ferrum_gate_get_execution` | `ExecutionResponse` | `{ "execution_id": "...", "state": "...", ... }` |
| `ferrum_gate_query_lineage` | `ProvenanceQueryResponse` | `{ "events": [...] }` |
| `ferrum_gate_list_approvals` | `ApprovalsListResponse` | `{ "approvals": [...] }` |
| `ferrum_gate_list_policy_bundles` | `PolicyBundlesListResponse` | `{ "bundles": [...] }` |
| `ferrum_gate_list_bridges` | `BridgesListResponse` | `{ "bridges": [...] }` |
| `ferrum_gate_list_bridge_tools` | `BridgeToolsListResponse` | `{ "tools": [...] }` |

---

## 3. Error Classification

### 3.1 Error Categories

Phase D-0 must classify gateway errors into three categories:

| Category | HTTP Status | MCP Error Code | Cause | Behavior |
|----------|-------------|---------------|-------|----------|
| **Auth Error** | 401, 403 | `-32002` (Authentication failed) | Invalid/missing bearer token | Return auth error; do not retry |
| **Gateway Unreachable** | N/A (connection failed) | `-32003` (Gateway unreachable) | ferrumd not running, wrong host/port, network error | Return unreachable error; log non-sensitive details only |
| **Server Error** | 4xx, 5xx | `-32004` (Gateway server error) | ferrumd returned error response | Return server error with gateway error message |

### 3.2 Error Response Format

All MCP error responses must use the standard JSON-RPC error format:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32002,
    "message": "Authentication failed",
    "data": {
      "detail": "Bearer token invalid or expired"
    }
  },
  "id": 1
}
```

### 3.3 Non-Goals for Error Handling

- **Do not log bearer tokens** — even at debug level
- **Do not log full response bodies** for auth errors — may contain sensitive data
- **Do not attempt retry logic** — this is out of scope for D-0
- **Do not implement rate limit handling (429)** — this is D-1 concern

---

## 4. Security Constraints

### 4.1 No Secret Logging

| Do | Don't |
|----|-------|
| Log `Authorization: Bearer ***` (redacted) | Log `Authorization: Bearer <actual_token>` |
| Log host/port for connection errors | Log bearer token value |
| Log HTTP status code | Log response body for auth errors |
| Use `Debug` fmt for sensitive structs | Use `Display` fmt for sensitive structs |

### 4.2 HTTP Client Security

| Requirement | Implementation |
|-------------|----------------|
| No TLS verification bypass | Use default reqwest TLS settings |
| Configurable gateway URL | Gateway URL from environment/config, not hardcoded |
| Timeout enforcement | Use reqwest `Timeout` middleware |
| No redirect following | Disable automatic redirects |

---

## 5. Phase D-0 Todo-List

### 5.1 HTTP Client Module

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.1 | Create `src/http_client.rs` module | Engineering | ☑ DONE | Reqwest-based HTTP client |
| D.2 | Add `reqwest` dependency to `Cargo.toml` | Engineering | ☑ DONE | Use `default-features = false` for lighter deps |
| D.3 | Add `tokio` runtime dependency if needed | Engineering | ☑ DONE | Phase C binary already has tokio; tokio used in tests |
| D.4 | Implement `FerrumGatewayClient` struct | Engineering | ☑ DONE | Holds base URL, HTTP client, timeout config |
| D.5 | Implement `base_url()` getter | Engineering | ☑ DONE | Returns configured gateway URL (named `base_url`) |
| D.6 | Implement `health()` method | Engineering | ☑ DONE | GET /v1/healthz |
| D.7 | Implement `readyz_deep()` method | Engineering | ☑ DONE | GET /v1/readyz/deep |
| D.8 | Implement `list_intents()` method | Engineering | ☑ DONE | GET /v1/intents with query params |
| D.9 | Implement `get_execution()` method | Engineering | ☑ DONE | GET /v1/executions/{id} |
| D.10 | Implement `query_lineage()` method | Engineering | ☑ DONE | GET /v1/provenance/query |
| D.11 | Implement `list_approvals()` method | Engineering | ☑ DONE | GET /v1/approvals |
| D.12 | Implement `list_policy_bundles()` method | Engineering | ☑ DONE | GET /v1/policy-bundles |
| D.13 | Implement `list_bridges()` method | Engineering | ☑ DONE | GET /v1/bridges |
| D.14 | Implement `list_bridge_tools()` method | Engineering | ☑ DONE | GET /v1/bridges/{id}/tools |

### 5.2 Response Parsing

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.15 | Define `HealthResponse` struct | Engineering | ☑ DONE | Parse /v1/healthz response |
| D.16 | Define `DeepHealthResponse` struct | Engineering | ☑ DONE | Parse /v1/readyz/deep response |
| D.17 | Define `IntentsListResponse` struct | Engineering | ☑ DONE | Parse /v1/intents response |
| D.18 | Define `ExecutionResponse` struct | Engineering | ☑ DONE | Parse /v1/executions/{id} response |
| D.19 | Define `ProvenanceQueryResponse` struct | Engineering | ☑ DONE | Parse /v1/provenance/query response |
| D.20 | Define `ApprovalsListResponse` struct | Engineering | ☑ DONE | Parse /v1/approvals response |
| D.21 | Define `PolicyBundlesListResponse` struct | Engineering | ☑ DONE | Parse /v1/policy-bundles response |
| D.22 | Define `BridgesListResponse` struct | Engineering | ☑ DONE | Parse /v1/bridges response |
| D.23 | Define `BridgeToolsListResponse` struct | Engineering | ☑ DONE | Parse /v1/bridges/{id}/tools response |

### 5.3 Error Classification

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.24 | Define `McpToolError` struct | Engineering | ☑ DONE | AuthError, GatewayUnreachable, ServerError variants via `GatewayError` |
| D.25 | Implement error classification for HTTP responses | Engineering | ☑ DONE | Map 401/403 → AuthError, connection fail → Unreachable, 4xx/5xx → ServerError |
| D.26 | Implement `From<reqwest::Error>` for `McpToolError` | Engineering | ☑ DONE | Error classification implemented inline in `execute()`; no standalone `From<reqwest::Error>` impl |
| D.27 | Ensure no secret logging in error paths | Engineering | ☑ DONE | Use Debug fmt, not Display, for sensitive data |

### 5.4 REST Mapper Integration

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.28 | Create `src/rest_mapper.rs` module | Engineering | ☑ DONE | Maps tool name → HTTP call → MCP response |
| D.29 | Wire `FerrumGatewayClient` into `handle_tools_call` | Engineering | ☑ DONE | Replace NOT_IMPLEMENTED with actual REST calls |
| D.30 | Implement `map_tool_to_rest()` function | Engineering | ☑ DONE | Route tool name to correct client method |
| D.31 | Implement response serialization to MCP format | Engineering | ☑ DONE | Convert gateway JSON response to MCP `CallToolResult` |

### 5.5 Configuration

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.32 | Add `FERRUM_GATEWAY_URL` environment variable | Engineering | ☑ DONE | Default: `http://127.0.0.1:8080` |
| D.33 | Add gateway URL to `FerrumGatewayClient` constructor | Engineering | ☑ DONE | Read from env var or config |
| D.34 | Add timeout configuration | Engineering | ☑ DONE | Default: 30s, configurable |

### 5.6 Testing

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.35 | Add unit tests for `FerrumGatewayClient` methods | Engineering | ☑ DONE | Mock HTTP responses |
| D.36 | Add unit tests for error classification | Engineering | ☑ DONE | Test 401, 403, 404, 500, connection error |
| D.37 | Add unit tests for response parsing | Engineering | ☑ DONE | Test each response struct |
| D.38 | Add integration test for full tool call flow | Engineering | ☑ DONE | Use mock server or test against running ferrumd |
| D.39 | Verify `cargo test -p ferrum-integrations-mcp` passes | Engineering | ☑ DONE | All D-0 tests pass |
| D.40 | Verify `cargo check --workspace` passes | Engineering | ☑ DONE | No new warnings |

### 5.7 Documentation

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.41 | Document `FerrumGatewayClient` usage | Engineering | ☑ DONE | Example code for initializing client |
| D.42 | Document error classification | Engineering | ☑ DONE | Explain auth error vs. unreachable vs. server error |
| D.43 | Update `src/bin/ferrum-mcp-server.rs` comments | Engineering | ☑ DONE | Note gateway URL config |
| D.44 | Update this document | Engineering | ☑ DONE | Mark D-0 items done after implementation |

---

## 6. Acceptance Criteria

### 6.1 D-0 Gate: Read-Only REST Client

| Criterion | Verification |
|-----------|--------------|
| All 9 read-only tools call the correct REST endpoint | Unit test with mock HTTP |
| HTTP error responses are mapped to appropriate MCP error codes | Unit test for 401, 403, 404, 500, connection refused |
| Auth errors (401/403) are distinguished from gateway-unreachable errors | Unit test for connection refused vs. 401 response |
| No bearer token is logged in any code path | Code review + grep for "token", "Authorization" in logging |
| No secret values appear in debug output | Code review + grep for Debug/Display misuse on sensitive structs |
| `cargo test -p ferrum-integrations-mcp` passes | Run tests |
| `cargo check --workspace` passes | Run check |

### 6.2 D-0 Gate: Route Correctness

| Criterion | Verification |
|-----------|--------------|
| `ferrum_gate_list_policy_bundles` calls `GET /v1/policy-bundles` | Unit test or integration test |
| All 9 tool routes match the table in §2.1 | Code review |

### 6.3 D-0 Gate: No Side Effects

| Criterion | Verification |
|-----------|--------------|
| No POST/PUT/DELETE HTTP calls in D-0 | Code review |
| No capability issuance | Code review |
| No provenance emission | Code review |
| No rollback preparation | Code review |

---

## 7. What Phase D-0 Does NOT Include

The following are **explicitly out of scope** for Phase D-0:

| Item | Reason |
|------|--------|
| Auth middleware (bearer token validation) | D-1; requires separate design |
| Policy evaluation | D-1; requires `ferrum-pdp` integration |
| Capability issuance | D-1; requires `ferrum-cap` integration |
| Provenance emission | D-1; requires `ferrum-ledger` integration |
| Rollback preparation | D-1; requires `ferrum-rollback` integration |
| Mutating tool execution | D-1; all 9 tools in D-0 are read-only |
| Streamable HTTP transport | Already deferred from Phase C |
| MCP SDK dependency | Already deferred from Phase A |
| Rate limit handling (429) | D-1 concern |
| Retry logic | D-0 returns errors immediately |

---

## 8. Phase D-1: Deferred Governance Pipeline

### 8.1 Why D-1 is Deferred

D-1 requires integrating with multiple FerrumGate subsystems that themselves require careful design:

| Subsystem | Integration Point | Design Complexity |
|-----------|------------------|------------------|
| Auth middleware | Bearer token validation, ActorRef mapping | Medium |
| Policy engine (PDP) | Evaluate tool call against policy bundle | High |
| Capability system | Mint single-use capability with TTL ≤ 300s | High |
| Provenance | Emit full lineage chain events | High |
| Rollback | Prepare rollback contract before execution | High |

### 8.2 D-1 Prerequisites

Before D-1 implementation begins:

1. **Separate design document** for D-1 governance pipeline integration
2. **Auth middleware design**: How MCP auth (bearer token in Authorization header) maps to FerrumGate `ActorRef`
3. **Policy evaluation design**: How MCP tool calls are evaluated against policy bundles
4. **Capability issuance design**: How single-use capabilities are minted before execution
5. **Rollback design**: How rollback contracts are prepared and stored
6. **Provenance design**: Which events to emit and how

### 8.3 D-1 Scope (Indicative)

When D-1 design is complete, it will cover:

| # | Item | Notes |
|---|------|-------|
| D.1.1 | Auth middleware (bearer token validation) | Map to ActorRef |
| D.1.2 | Policy evaluation gate | Before tool execution |
| D.1.3 | Capability issuance | TTL ≤ 300s, single-use |
| D.1.4 | Rollback preparation | Before execution |
| D.1.5 | Provenance emission | Full lineage chain |
| D.1.6 | Output sanitization | Before return to agent |
| D.1.7 | Mutating tool execution | submit_intent, evaluate_intent, etc. |

### 8.4 D-1 Non-Goals

- D-1 does **not** include adapter-backed tools (fs, git, http, sqlite) — those are a separate phase
- D-1 does **not** include Streamable HTTP transport — that was deferred from Phase C
- D-1 does **not** include production hardening (rate limits, load testing) — MVP 3

---

## 9. Risks and Decision Log

### 9.1 Risks for D-0

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **HTTP response shape mismatch** | Medium | Medium | Match response structs to actual ferrumd OpenAPI schema |
| **Error classification edge cases** | Low | Medium | Unit tests cover 401, 403, 404, 500, connection refused |
| **Secret logging in error paths** | Medium | High | Strict code review; grep for logging of sensitive fields |
| **Gateway URL misconfiguration** | Low | Low | Clear error message when connection fails |

### 9.2 Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| Split Phase D into D-0 and D-1 | 2026-05-06 | D-0 is low-risk read-only REST client; D-1 requires complex governance pipeline design |
| Use reqwest for HTTP client | 2026-05-06 | Established crate; async support; used in ferrumctl |
| No retry logic in D-0 | 2026-05-06 | Simplicity; errors returned immediately |
| No auth middleware in D-0 | 2026-05-06 | Auth is D-1; D-0 tools are all read-only and public |
| Default gateway URL `http://127.0.0.1:8080` | 2026-05-06 | Matches ferrumd default bind address |

---

## 10. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`71-mcp-server-feasibility-and-design.md`](71-mcp-server-feasibility-and-design.md) | Feasibility and design basis |
| This doc | [`72-mcp-server-phase-a-implementation-plan.md`](72-mcp-server-phase-a-implementation-plan.md) | Handoff source; Phases A, B, C complete |
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope |
| This doc | [`README.md`](README.md) | Reading order entry |
| This doc | [`../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md`](../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md) | Crate naming alignment |
| This doc | [`09-phase-checklists.md`](09-phase-checklists.md) | Phase D adapter status reference |

---

## 11. Checklist Summary

### Phase D-0 Summary

| Phase | Status |
|-------|--------|
| Phase A (skeleton + tool registry) | ✅ Complete |
| Phase B (JSON-RPC + handlers) | ✅ Complete |
| Phase C (stdio + binary) | ✅ Complete |
| Phase D-0 (read-only REST client) | ✅ Complete |

### Phase D-0 Checklist (D.1–D.44)

- [x] D.1 Create `src/http_client.rs` module
- [x] D.2 Add `reqwest` dependency to `Cargo.toml`
- [x] D.3 Add `tokio` runtime dependency if needed
- [x] D.4 Implement `FerrumGatewayClient` struct
- [x] D.5 Implement `base_url()` getter
- [x] D.6 Implement `health()` method
- [x] D.7 Implement `readyz_deep()` method
- [x] D.8 Implement `list_intents()` method
- [x] D.9 Implement `get_execution()` method
- [x] D.10 Implement `query_lineage()` method
- [x] D.11 Implement `list_approvals()` method
- [x] D.12 Implement `list_policy_bundles()` method
- [x] D.13 Implement `list_bridges()` method
- [x] D.14 Implement `list_bridge_tools()` method
- [x] D.15 Define `HealthResponse` struct
- [x] D.16 Define `DeepHealthResponse` struct
- [x] D.17 Define `IntentsListResponse` struct
- [x] D.18 Define `ExecutionResponse` struct
- [x] D.19 Define `ProvenanceQueryResponse` struct
- [x] D.20 Define `ApprovalsListResponse` struct
- [x] D.21 Define `PolicyBundlesListResponse` struct
- [x] D.22 Define `BridgesListResponse` struct
- [x] D.23 Define `BridgeToolsListResponse` struct
- [x] D.24 Define `McpToolError` struct
- [x] D.25 Implement error classification for HTTP responses
- [x] D.26 Implement `From<reqwest::Error>` for `McpToolError` (inline in `execute()`)
- [x] D.27 Ensure no secret logging in error paths
- [x] D.28 Create `src/rest_mapper.rs` module
- [x] D.29 Wire `FerrumGatewayClient` into `handle_tools_call`
- [x] D.30 Implement `map_tool_to_rest()` function
- [x] D.31 Implement response serialization to MCP format
- [x] D.32 Add `FERRUM_GATEWAY_URL` environment variable
- [x] D.33 Add gateway URL to `FerrumGatewayClient` constructor
- [x] D.34 Add timeout configuration
- [x] D.35 Add unit tests for `FerrumGatewayClient` methods
- [x] D.36 Add unit tests for error classification
- [x] D.37 Add unit tests for response parsing
- [x] D.38 Add integration test for full tool call flow
- [x] D.39 Verify `cargo test -p ferrum-integrations-mcp` passes
- [x] D.40 Verify `cargo check --workspace` passes
- [x] D.41 Document `FerrumGatewayClient` usage
- [x] D.42 Document error classification
- [x] D.43 Update `src/bin/ferrum-mcp-server.rs` comments
- [x] D.44 Update this document

---

*Document created: 2026-05-06. Phase D-0 (read-only REST client) implemented. Phase D-1 (mutating governance pipeline) deferred — requires separate design. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
