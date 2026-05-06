# 72 — MCP Server Phase A Implementation Plan

> **Status**: Phase A skeleton partially implemented — crate + read-only tool registry only.
> **Purpose**: Detailed Phase A implementation checklist and execution tracker for FerrumGate MCP server work.
> **Scope**: Phase A = crate skeleton + read-only tool surface + stdio transport skeleton. No mutating tools.
> **Constraint**: Do not claim MCP server readiness. The current crate is schema/registry-only; no MCP transport, handlers, or mutating tools exist.
> **Handoff from**: [`71-mcp-server-feasibility-and-design.md`](71-mcp-server-feasibility-and-design.md)

---

## Explicit Non-Claims

- **No MCP server implementation exists.** A Phase A Rust crate skeleton now exists, but it is not an MCP server.
- **No production-ready claim.** Phase A is a skeleton only.
- **No G2 complete claim.** G2.1–G2.8 remain pending.
- **MCP server is post-v1 scope.** Phase A is pre-implementation planning for v1.4 MCP Governance Beta.
- **No mutating tools in Phase A.** The read-only tool surface is intentionally limited.

---

## 0. Current Implementation Snapshot

As of the Phase A skeleton pass:

- `crates/ferrum-integrations-mcp` exists and is registered in the workspace.
- The crate defines a read-only tool registry with 9 tools.
- The crate uses only `serde` and `serde_json`.
- `MUTATING_TOOLS` is intentionally empty.
- Tests prove the registry contains read-only tools only and excludes mutating tool-name patterns.
- `cargo check -p ferrum-integrations-mcp`, `cargo test -p ferrum-integrations-mcp`, and `cargo check --workspace` passed during the skeleton pass.

Still not implemented:

- No MCP SDK dependency.
- No stdio transport.
- No JSON-RPC parser/serializer.
- No `initialize`, `ping`, `tools/list`, or `tools/call` handlers.
- No binary entry point.
- No gateway/governance pipeline integration.
- No mutating tools.

As of the Phase B JSON-RPC skeleton pass:

- JSON-RPC 2.0 request/response/error types exist in `crates/ferrum-integrations-mcp/src/lib.rs`.
- Handler stubs exist for `initialize`, `ping`, `tools/list`, and `tools/call`.
- `tools/list` returns the 9 read-only registry tools.
- `tools/call` returns an explicit not-implemented error for all calls.
- `dispatch()` routes known methods and returns method-not-found for unknown methods.
- `parse_request()` parses a JSON-RPC request string.
- `cargo test -p ferrum-integrations-mcp` passed with 19 tests during the Phase B pass.

Still not implemented after Phase B:

- No stdio transport loop.
- No Streamable HTTP transport.
- No MCP SDK dependency.
- No gateway/governance pipeline integration.
- No tool execution.
- No mutating tools.

---

## 1. Purpose and Scope

### 1.1 Purpose

Phase A is the **planning and pre-implementation phase** for FerrumGate MCP server. It produces:
1. A `Cargo.toml` skeleton for `crates/ferrum-integrations-mcp`
2. A binary stub for `ferrum-mcp-server`
3. A JSON Schema draft for all MCP tool definitions (read-only first)
4. An `initialize` handler stub
5. A `tools/list` handler stub that returns read-only tools only
6. A stdio transport skeleton
7. Test stubs proving no mutating tools are registered
8. This document as the implementation reference

### 1.2 Scope Boundaries

**In Scope for Phase A:**
- Create `crates/ferrum-integrations-mcp` crate skeleton
- Add `ferrum-mcp-server` binary stub
- Implement stdio JSON-RPC transport skeleton
- Implement `initialize` handler stub
- Implement `tools/list` handler stub (read-only tools only)
- Implement `ping` handler stub
- Define JSON Schema for all read-only tools
- Document all security gates that will apply to later phases
- Write test stubs proving no mutating tools are registered
- Pass `cargo check` on the new crate

**Out of Scope for Phase A (Non-Goals):**
- No `tools/call` implementation (mutating tools)
- No Streamable HTTP transport
- No OAuth or advanced auth schemes
- No adapter-backed tools (fs, git, http, etc.)
- No governance pipeline integration (Phase D)
- No CI changes
- No rollback integration
- No provenance emission
- No rate limiting implementation
- No production-ready claim

### 1.3 Phase Sequencing

```
Doc 71 (Feasibility & Design)
    │
    ▼
Phase A ← THIS DOCUMENT (planning + skeleton)
    │
    ▼
Phase B (JSON-RPC skeleton + handlers)
    │
    ▼
Phase C (protocol handlers + read-only tool stubs)
    │
    ▼
Phase D (governance pipeline integration)
    │
    ▼
MVP 0 → 1 → 2 → 3
```

---

## 2. Crate Naming Decision

### 2.1 Decision

**Use `crates/ferrum-integrations-mcp`** for the crate name.

### 2.2 Alternatives Considered

| Option | Crate | Binary | Rationale |
|--------|-------|--------|-----------|
| A | `ferrum-mcp-server` | `ferrum-mcp-server` | Clear, focused; considered but not selected |
| **B (Selected)** | `ferrum-integrations-mcp` | `ferrum-mcp-server` | Aligns with roadmap v1 `90-upgrade-and-integration-plan.md §5.1`; future-proofs for `ferrum-integrations-local`, `ferrum-integrations-nemoclaw` |

### 2.3 Decision Log

```
Decision: Use crates/ferrum-integrations-mcp
Date: 2026-05-06
Alternatives: ferrum-mcp-server (Option A)
Factors:
  1. Roadmap alignment: 90-upgrade-and-integration-plan.md §5.1 proposes ferrum-integrations-mcp
  2. Future-proofing: allows ferrum-integrations-* family
  3. Consistency: matches ferrum-adapter-* naming pattern
  4. Binary name: ferrum-mcp-server (清晰; separate from crate name)
Rejected: ferrum-mcp-server crate name
  Reason: less descriptive; doesn't signal integration layer
```

---

## 3. Read-Only Tool Surface (Phase A MVP 0)

### 3.1 Tool List

Phase A exposes **only read-only tools** that query FerrumGate state without making changes:

| Tool Name | Description | REST API Equivalent | Risk |
|-----------|-------------|-------------------|------|
| `ferrum_gate_health` | Health probe | `GET /v1/healthz` | Low |
| `ferrum_gate_readyz_deep` | Deep readiness probe | `GET /v1/readyz/deep` | Low |
| `ferrum_gate_list_intents` | List intents with optional filters | `GET /v1/intents` | Low |
| `ferrum_gate_get_execution` | Get execution status by ID | `GET /v1/executions/{execution_id}` | Low |
| `ferrum_gate_query_lineage` | Query provenance events for an execution | `GET /v1/lineage` | Low |
| `ferrum_gate_list_approvals` | List pending approvals | `GET /v1/approvals` | Low |
| `ferrum_gate_list_policy_bundles` | List available policy bundles | `GET /v1/policies` | Low |
| `ferrum_gate_list_bridges` | List registered runtime bridges | `GET /v1/bridges` | Low |
| `ferrum_gate_list_bridge_tools` | List tools for a specific bridge | `GET /v1/bridges/{bridge_id}/tools` | Low |

### 3.2 Non-Goals (Explicit)

These tools are **NOT** in Phase A scope:

| Tool | Reason for Deferral |
|------|-------------------|
| `ferrum_gate_submit_intent` | Mutating; requires governance pipeline (Phase D) |
| `ferrum_gate_evaluate_intent` | Mutating; requires governance pipeline (Phase D) |
| `ferrum_gate_prepare_execution` | Mutating; requires governance pipeline (Phase D) |
| `ferrum_gate_execute_prepared` | Mutating; requires rollback (Phase D) |
| `ferrum_gate_compensate` | Mutating; requires rollback (Phase D) |
| `ferrum_gate_fs_*` | Adapter-backed; requires adapter integration (Phase D) |
| `ferrum_gate_git_*` | Adapter-backed; requires adapter integration (Phase D) |
| `ferrum_gate_http_*` | Adapter-backed; requires adapter integration (Phase D) |

### 3.3 Tool Naming Convention

All MCP tools use the `ferrum_gate_` prefix followed by the operation:

```
ferrum_gate_<resource>_<operation>
```

Examples:
- `ferrum_gate_list_intents` — list operation on intents resource
- `ferrum_gate_get_execution` — get operation on execution resource
- `ferrum_gate_list_bridges` — list operation on bridges resource

---

## 4. Security Gates (For Later Phases)

Phase A does **not** implement these gates, but they **must** be implemented before mutating tools in later phases:

### 4.1 Authentication Gate

| Requirement | Description |
|-------------|-------------|
| Bearer token | MCP clients must present bearer token via `Authorization` header or `Bearer` scheme |
| Token validation | Validate bearer token on every request; reject with 401 if invalid |
| Token source | Use existing `FERRUMD_BEARER_TOKEN` or config-file bearer token |

### 4.2 Policy Evaluation Gate

| Requirement | Description |
|-------------|-------------|
| Policy bundle | Each tool call must be evaluated against active policy bundle |
| Policy engine | Use existing `ferrum-pdp` for policy evaluation |
| Deny on no match | If no policy rule matches, deny the tool call |

### 4.3 Capability Issuance Gate

| Requirement | Description |
|-------------|-------------|
| Single-use | Every mutating tool call requires a single-use capability |
| TTL max | Capability TTL ≤ 300 seconds |
| Scope binding | Capability scope must cover the tool call scope |

### 4.4 Scope Validation Gate

| Requirement | Description |
|-------------|-------------|
| Scope match | Tool call scope must not exceed capability scope |
| Fail-closed | Scope mismatch → deny with clear error |

### 4.5 Rollback Prepare Gate

| Requirement | Description |
|-------------|-------------|
| Prepare before execute | Rollback contract must be prepared before execution |
| Contract storage | Store rollback contract in `ferrum-rollback` |
| Compensate on failure | If execution fails, invoke compensation |

### 4.6 Provenance Emission Gate

| Requirement | Description |
|-------------|-------------|
| Emit events | Every tool call must emit provenance events |
| Minimum chain | `ActionProposalSubmitted` → `PolicyEvaluated` → `ToolCallPrepared` → `ToolCallExecuted` |
| Store events | Store in `ferrum-ledger` |

### 4.7 Output Sanitization Gate

| Requirement | Description |
|-------------|-------------|
| Sanitize output | Tool outputs must be sanitized before return to agent |
| Use firewall | Use existing `ferrum-firewall` for sanitization |
| Default-deny | Unknown or sensitive fields → redact |

### 4.8 Default-Deny Unknown/Noop Gate

| Requirement | Description |
|-------------|-------------|
| Unknown tools | Return error for tools not in registry |
| Noop tools | Return error for tools marked as noop |
| Explicit registry | Only tools in the explicit registry are callable |

### 4.9 Per-Agent Rate Limit Gate

| Requirement | Description |
|-------------|-------------|
| Rate limit | Per-agent rate limit: 2 req/s default, configurable |
| Burst | Burst limit: 50 default, configurable |
| Enforcement | Use existing `tower_governor` |

### 4.10 ActorRef Mapping Gate

| Requirement | Description |
|-------------|-------------|
| Map identity | Map MCP client identity → `ActorRef` for provenance |
| actor_type | Use `ActorType::Agent` for MCP clients |
| actor_id | Use bearer token subject or client-provided ID |
| display_name | Optional; use client-provided name if available |

---

## 5. Phase A Todo-List

### 5.1 Crate and Project Setup

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.1 | Create `crates/ferrum-integrations-mcp` directory structure | Engineering | ✅ DONE | Library skeleton exists; binary stub intentionally deferred |
| A.2 | Create `Cargo.toml` for `ferrum-integrations-mcp` | Engineering | ✅ DONE | Package name: `ferrum-integrations-mcp`; uses only `serde` and `serde_json` |
| A.3 | Add `ferrum-integrations-mcp` to workspace `Cargo.toml` | Engineering | ✅ DONE | Added to workspace members; MCP SDK not added |
| A.4 | Create `src/lib.rs` with module structure | Engineering | ◐ PARTIAL | Single-file library skeleton exists; module split deferred |
| A.5 | Create `src/bin/ferrum-mcp-server.rs` main entry point | Engineering | ☐ TODO | Stdin/stdout async loop stub |
| A.6 | Add `ferrum-proto`, `ferrum-gateway` (or `ferrum-sync`) as lib dependencies | Engineering | ☐ TODO | For types (ActorRef, ProvenanceEvent, etc.) |
| A.7 | Verify `cargo check -p ferrum-integrations-mcp` passes | Engineering | ✅ DONE | Skeleton compiles without errors |

### 5.2 JSON Schema Draft

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.8 | Draft JSON Schema for `ferrum_gate_health` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.9 | Draft JSON Schema for `ferrum_gate_readyz_deep` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.10 | Draft JSON Schema for `ferrum_gate_list_intents` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.11 | Draft JSON Schema for `ferrum_gate_get_execution` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.12 | Draft JSON Schema for `ferrum_gate_query_lineage` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.13 | Draft JSON Schema for `ferrum_gate_list_approvals` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.14 | Draft JSON Schema for `ferrum_gate_list_policy_bundles` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.15 | Draft JSON Schema for `ferrum_gate_list_bridges` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.16 | Draft JSON Schema for `ferrum_gate_list_bridge_tools` | Engineering | ◐ PARTIAL | Input schema exists; output schema deferred |
| A.17 | Create `src/schema/mod.rs` with all tool schemas | Engineering | ☐ TODO | Reusable; will be used by handlers |

### 5.3 Stdio Transport Skeleton

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.18 | Implement `Transport` trait stub in `src/transport/stdio.rs` | Engineering | ☐ TODO | Stdio loop remains deferred |
| A.19 | Implement JSON-RPC 2.0 request parser | Engineering | ✅ DONE | `parse_request()` parses JSON-RPC request strings |
| A.20 | Implement JSON-RPC 2.0 response serializer | Engineering | ◐ PARTIAL | `JsonRpcResponse` is serializable; no stdout writer yet |
| A.21 | Implement batch request handling | Engineering | ☐ TODO | MCP allows batch requests |
| A.22 | Implement error response format | Engineering | ✅ DONE | Standard JSON-RPC errors plus Phase B `NOT_IMPLEMENTED` |
| A.23 | Create `src/transport/mod.rs` | Engineering | ☐ TODO | Export `StdioTransport` |

### 5.4 MCP Protocol Handlers (Stubs)

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.24 | Implement `initialize` handler stub in `src/handlers/initialize.rs` | Engineering | ◐ PARTIAL | `handle_initialize()` exists in `src/lib.rs`; module split deferred |
| A.25 | Implement `ping` handler stub in `src/handlers/ping.rs` | Engineering | ◐ PARTIAL | `handle_ping()` exists in `src/lib.rs`; module split deferred |
| A.26 | Implement `tools/list` handler stub in `src/handlers/tools_list.rs` | Engineering | ◐ PARTIAL | `handle_tools_list()` returns read-only registry; module split deferred |
| A.27 | Implement `tools/call` handler stub in `src/handlers/tools_call.rs` | Engineering | ◐ PARTIAL | `handle_tools_call()` returns not-implemented for all tools; module split deferred |
| A.28 | Create `src/handlers/mod.rs` | Engineering | ☐ TODO | Export all handlers |
| A.29 | Create `src/server.rs` dispatch loop | Engineering | ◐ PARTIAL | `dispatch()` exists in `src/lib.rs`; module split deferred |

### 5.5 Tool Registry

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.30 | Create `src/tools/mod.rs` | Engineering | ☐ TODO | Export `TOOL_REGISTRY` |
| A.31 | Define `TOOL_REGISTRY: Vec<Tool>` constant | Engineering | ◐ PARTIAL | Implemented as lazy `tool_registry()` in `src/lib.rs`; contains all 9 read-only tools |
| A.32 | Define `READ_ONLY_TOOLS: HashSet<&str>` constant | Engineering | ✅ DONE | Implemented as const slice with all 9 read-only tool names |
| A.33 | Define `MUTATING_TOOLS: HashSet<&str>` constant | Engineering | ✅ DONE | Implemented as empty const slice in Phase A |
| A.34 | Create `Tool` struct in `src/tools/tool.rs` | Engineering | ◐ PARTIAL | `Tool` struct exists in `src/lib.rs`; module split deferred |

### 5.6 Test Stubs

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.35 | Add `#[cfg(test)]` module in `src/tools/mod.rs` | Engineering | ☐ TODO | |
| A.36 | Write test: `test_tool_registry_contains_only_read_only_tools` | Engineering | ✅ DONE | All registered tools have `read_only: true` |
| A.37 | Write test: `test_mutating_tools_set_is_empty` | Engineering | ✅ DONE | `MUTATING_TOOLS` is empty |
| A.38 | Write test: `test_all_read_only_tools_have_schemas` | Engineering | ✅ DONE | All read-only tools have non-null schemas |
| A.39 | Write test: `test_stdio_transport_parses_valid_json_rpc` | Engineering | ◐ PARTIAL | `test_parse_valid_request` covers parser; stdio transport remains deferred |
| A.40 | Write test: `test_tools_list_returns_only_read_only_tools` | Engineering | ✅ DONE | `tools/list` tests return 9 read-only tool names |

### 5.7 Documentation

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.41 | Create `crates/ferrum-integrations-mcp/README.md` | Engineering | ☐ TODO | Overview, building, running, testing |
| A.42 | Add module-level doc comments to all `pub` items | Engineering | ☐ TODO | |
| A.43 | Create `docs/mcp-server-read-only-tool-schema.md` | Engineering | ☐ TODO | All 9 tool schemas with examples |
| A.44 | Update this document if any checkbox is checked | Engineering | ✅ DONE | Updated after Phase A and Phase B skeleton passes |

---

## 6. Acceptance Criteria

### 6.1 Phase A Gate: Skeleton Compiles

| Criterion | Verification |
|-----------|--------------|
| `cargo check -p ferrum-integrations-mcp` passes | Engineering |
| `cargo check --workspace` passes | Engineering |
| No new warnings in clippy for new crate | Engineering |

### 6.2 Phase A Gate: Tool Schema Complete

| Criterion | Verification |
|-----------|--------------|
| All 9 read-only tools have JSON Schema defined | Engineering |
| All schemas are valid JSON | Engineering |
| All schemas have `input_schema` and `output_schema` | Engineering |

### 6.3 Phase A Gate: Handler Stubs Return Correct Shape

| Criterion | Verification |
|-----------|--------------|
| `initialize` returns protocol version `2024-11-05` | Engineering |
| `tools/list` returns 9 tools | Engineering |
| `tools/call` returns error for all tools | Engineering |
| `ping` returns `{success: true}` | Engineering |

### 6.4 Phase A Gate: Tests Prove Read-Only Constraint

| Criterion | Verification |
|-----------|--------------|
| `test_tool_registry_contains_only_read_only_tools` passes | Engineering |
| `test_mutating_tools_set_is_empty` passes | Engineering |
| `test_all_read_only_tools_have_schemas` passes | Engineering |

### 6.5 Phase A Gate: No Side Effects

| Criterion | Verification |
|-----------|--------------|
| No mutating REST API calls in handler stubs | Engineering |
| No database writes | Engineering |
| No file system writes | Engineering |
| No capability issuance | Engineering |
| No provenance emission | Engineering |

---

## 7. Risks and Decision Log

### 7.1 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **MCP SDK dependency adds bloat** | Medium | Low | Phase A deliberately does NOT add MCP SDK; use raw JSON-RPC |
| **JSON-RPC parsing edge cases** | Medium | Medium | Use `serde_json`; handle batch and error cases |
| **Schema drift** | Low | Medium | Define schemas early; validate against REST API shapes |
| **Tool naming inconsistency** | Low | Low | Use `ferrum_gate_` prefix consistently |
| **Binary name confusion** | Low | Low | Document: crate = `ferrum-integrations-mcp`, binary = `ferrum-mcp-server` |

### 7.2 Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| Use `ferrum-integrations-mcp` crate name | 2026-05-06 | Roadmap alignment; future-proofing |
| Use `ferrum-mcp-server` binary name | 2026-05-06 | Clear; separate from crate name |
| No MCP SDK in Phase A | 2026-05-06 | Avoid bloat; raw JSON-RPC is sufficient for skeleton |
| No Streamable HTTP in Phase A | 2026-05-06 | Stdio first; HTTP+SSE is Phase B+ |
| No auth in Phase A handler stubs | 2026-05-06 | Auth is Phase D; stubs return mock data |
| All Phase A tools are read-only | 2026-05-06 | Minimize risk; validate protocol before mutating |
| No governance pipeline in Phase A | 2026-05-06 | Phase D; handler stubs call REST API directly (read-only) |

---

## 8. Future Handoff

### 8.1 Handoff to Phase B

When Phase A is complete, Phase B should:

1. **Add MCP SDK dependency** to `Cargo.toml` when the SDK is needed for full protocol compliance
2. **Implement full JSON-RPC 2.0** error handling (batch, notification, etc.)
3. **Add Streamable HTTP transport** option (stdio is primary for local agents)
4. **Add protocol version negotiation** in `initialize` handler
5. **Add client capabilities handling** in `initialize` handler

### 8.2 Handoff to Phase C

When Phase B is complete, Phase C should:

1. **Implement real `tools/list`** by querying REST API instead of static registry
2. **Implement real `tools/call`** by calling REST API with proper request/response mapping
3. **Add MCP SDK types** for `CallToolResult`, `ListToolsResult`, etc.
4. **Implement `resources/list`** handler (optional)
5. **Implement `prompts/list`** handler (optional)

### 8.3 Handoff to Phase D

When Phase C is complete, Phase D should:

1. **Add auth middleware** to validate bearer token
2. **Add ActorRef mapping** for provenance
3. **Add capability issuance** before mutating tool calls
4. **Add policy evaluation** before tool calls
5. **Add rollback preparation** for mutating tools
6. **Add provenance emission** for all tool calls
7. **Add output sanitization** after tool calls
8. **Add rate limiting** per-agent
9. **Add default-deny** for unknown tools

---

## 9. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`71-mcp-server-feasibility-and-design.md`](71-mcp-server-feasibility-and-design.md) | Handoff source; feasibility and design |
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope; MCP Governance Beta (v1.4) |
| This doc | [`README.md`](README.md) | Reading order entry |
| This doc | [`../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md`](../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md) | Crate naming alignment |
| This doc | [`../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/02-release-plan.md`](../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/02-release-plan.md) | v1.4 MCP Governance Beta scope |
| This doc | [`09-phase-checklists.md`](09-phase-checklists.md) | Phase D adapter status (fs, git, http, sqlite) |

---

## 10. Checklist Summary

### Crate Setup (A.1–A.7)
- [ ] A.1 Create directory structure
- [ ] A.2 Create Cargo.toml
- [ ] A.3 Add to workspace
- [ ] A.4 Create lib.rs module structure
- [ ] A.5 Create main binary entry point
- [ ] A.6 Add dependencies
- [ ] A.7 Verify cargo check passes

### Schema Draft (A.8–A.17)
- [ ] A.8–A.17 Draft all 9 tool schemas

### Stdio Transport (A.18–A.23)
- [ ] A.18–A.23 Implement stdio transport skeleton

### Handler Stubs (A.24–A.29)
- [ ] A.24–A.29 Implement handler stubs

### Tool Registry (A.30–A.34)
- [ ] A.30–A.34 Define tool registry

### Test Stubs (A.35–A.40)
- [ ] A.35–A.40 Write tests proving read-only constraint

### Documentation (A.41–A.44)
- [ ] A.41–A.44 Create docs

---

*Document created: 2026-05-06. Pre-implementation planning only. Phase A skeleton only. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
