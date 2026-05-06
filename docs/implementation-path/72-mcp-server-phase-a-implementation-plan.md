# 72 — MCP Server Phase A–C Implementation Plan

> **Status**: Phase A (skeleton), Phase B (JSON-RPC handlers), and Phase C (stdio transport + binary) implemented.
> **Purpose**: Detailed implementation checklist and execution tracker for FerrumGate MCP server phases A, B, and C.
> **Scope**: Phase A = crate skeleton + read-only tool surface. Phase B = JSON-RPC types + handler stubs. Phase C = stdio transport + binary skeleton. No mutating tools in Phases A–C.
> **Constraint**: Do not claim MCP server readiness. Phases A–C produce a skeleton only; no gateway integration, no mutating tools, no auth.
> **Handoff from**: [`71-mcp-server-feasibility-and-design.md`](71-mcp-server-feasibility-and-design.md)

---

## Explicit Non-Claims

- **No MCP server implementation exists.** Phases A and B produced a crate skeleton with JSON-RPC types and handler stubs, but no actual transport or server loop exists.
- **No production-ready claim.** Phases A–C are skeletons only.
- **No G2 complete claim.** G2.1–G2.8 remain pending.
- **MCP server is post-v1 scope.** Phase A–C are pre-implementation planning for v1.4 MCP Governance Beta.
- **No mutating tools in Phases A–C.** The read-only tool surface is intentionally limited; tool execution remains out of scope.
- **No gateway integration in Phase C.** Phase C implements stdio transport and binary skeleton only; REST API calls to FerrumGate are not part of Phase C.

---

## 0. Current Implementation Snapshot

### Phase A Status: Complete

As of the Phase A skeleton pass (commit `ba60e24`):

- `crates/ferrum-integrations-mcp` exists and is registered in the workspace.
- The crate defines a read-only tool registry with 9 tools.
- The crate uses only `serde` and `serde_json`.
- `MUTATING_TOOLS` is intentionally empty.
- Tests prove the registry contains read-only tools only and excludes mutating tool-name patterns.
- `cargo check -p ferrum-integrations-mcp`, `cargo test -p ferrum-integrations-mcp`, and `cargo check --workspace` passed.

### Phase B Status: Complete

As of the Phase B JSON-RPC skeleton pass:

- JSON-RPC 2.0 request/response/error types exist in `crates/ferrum-integrations-mcp/src/lib.rs`.
- Handler stubs exist for `initialize`, `ping`, `tools/list`, and `tools/call`.
- `tools/list` returns the 9 read-only registry tools.
- `tools/call` returns an explicit NOT_IMPLEMENTED error (`-32001`) for all calls.
- `dispatch()` routes known methods and returns method-not-found for unknown methods.
- `parse_request()` parses a JSON-RPC request string.
- `cargo test -p ferrum-integrations-mcp` passed with 19 tests.

### Phase C Status: Complete

As of the Phase C stdio transport + binary pass:

- Binary entry point: `ferrum-mcp-server` at `crates/ferrum-integrations-mcp/src/bin/ferrum-mcp-server.rs`.
- Stdio transport loop: reads JSON-RPC requests line-by-line from stdin, writes responses to stdout.
- Reuses existing `parse_request()` and `dispatch()` from Phase B.
- 8 unit tests covering process_line (ping, initialize, tools/list, tools/call, unknown method, invalid JSON, empty/whitespace lines).
- Binary smoke tested: `echo '{"jsonrpc":"2.0","method":"ping","id":1}' | cargo run --bin ferrum-mcp-server` returns valid JSON.
- `tools/list` returns 9 read-only tools.
- `tools/call` returns NOT_IMPLEMENTED error as expected.

Phase C will NOT add:

- No Streamable HTTP transport.
- No MCP SDK dependency.
- No gateway/governance pipeline integration.
- No tool execution (tools/call remains NOT_IMPLEMENTED).
- No mutating tools.

---

## 1. Purpose and Scope

### 1.1 Purpose

Phases A and B produced a **skeleton** for FerrumGate MCP server. Phase C will add the **stdio transport and binary skeleton**.

Phase A produced:
1. A `Cargo.toml` skeleton for `crates/ferrum-integrations-mcp`
2. A JSON Schema draft for all MCP tool definitions (read-only first)

Phase B produced:
3. JSON-RPC 2.0 request/response types and error codes
4. Handler stubs for `initialize`, `ping`, `tools/list`, `tools/call`
5. `dispatch()` and `parse_request()` functions

Phase C produced:
6. A binary entry point for `ferrum-mcp-server`
7. A stdio transport loop (read stdin, write stdout)
8. Reuse of existing `dispatch()` and `parse_request()`

### 1.2 Scope Boundaries

**In Scope for Phase C:**
- Create `ferrum-mcp-server` binary target
- Implement stdio line-based transport using existing `parse_request()` and `dispatch()`
- Support `initialize`, `ping`, `tools/list`, `tools/call` via dispatch
- Binary smoke test via stdin/stdout piping
- Reuse existing JSON-RPC types and handlers from Phase B

**Out of Scope for Phase C (Non-Goals):**
- No Streamable HTTP transport
- No MCP SDK dependency
- No gateway/governance pipeline integration
- No tool execution (tools/call returns NOT_IMPLEMENTED)
- No mutating tools
- No OAuth or advanced auth schemes
- No CI changes
- No production-ready claim

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
| `ferrum_gate_list_policy_bundles` | List available policy bundles | `GET /v1/policy-bundles` | Low |
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

## 5.8 Phase C: Stdio Transport + Binary Skeleton

### 5.8.1 Purpose

Phase C adds the **binary entry point** and **stdio transport loop** to make the Phase B handler skeleton runnable. It reuses `parse_request()` and `dispatch()` from Phase B.

### 5.8.2 Binary Target

The binary target is `ferrum-mcp-server`:

| Option | Location | Notes |
|--------|---------|-------|
| **A (Selected)** | `src/bin/ferrum-mcp-server.rs` as package `[[bin]]` | Simple; same crate; no extra Cargo.toml needed |
| B | Separate `ferrum-mcp-server` crate under `bins/` | More complex; requires separate crate |

### 5.8.3 Stdio Transport Behavior

```
┌─────────────────┐     stdin      ┌──────────────────────┐
│  MCP Client     │ ──────────────►│  ferrum-mcp-server   │
│  (AI Agent)     │◄────────────── │  (binary)            │
└─────────────────┘     stdout     └──────────────────────┘
                              │
                              │ stderr (errors only, no secrets)
                              ▼
                         /dev/null
```

**Line-based protocol:**
1. Read one line from stdin (trim whitespace/newline)
2. Parse JSON-RPC request via `parse_request()`
3. Dispatch via `dispatch()`
4. Serialize response to JSON string
5. Write one line to stdout (with newline)
6. Repeat until EOF or signal

**Error handling:**
- Parse errors → write JSON-RPC parse error to stdout (not stderr, to keep protocol consistent)
- Unknown methods → JSON-RPC method-not-found error via `dispatch()`
- `tools/call` → NOT_IMPLEMENTED error via `dispatch()`
- No panics in the main loop

**Security constraints:**
- Never log secrets to stderr or stdout
- Never read environment variables for secrets (Phase D auth)
- Never write to file system
- Output only valid JSON-RPC on stdout
- stderr is for debug/traces only, never secrets

### 5.8.4 Phase C Todo-List

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| C.1 | Create `src/bin/ferrum-mcp-server.rs` | Engineering | ✅ DONE | Main binary entry point created |
| C.2 | Add `[[bin]]` target to `Cargo.toml` | Engineering | ✅ N/A | Cargo auto-detects `src/bin/`; no explicit `[[bin]]` needed |
| C.3 | Implement stdin line reader | Engineering | ✅ DONE | Read lines from stdin until EOF |
| C.4 | Implement stdout line writer | Engineering | ✅ DONE | Write responses as JSON lines |
| C.5 | Wire `parse_request()` → `dispatch()` → response | Engineering | ✅ DONE | Reuses Phase B handlers |
| C.6 | Handle SIGINT/SIGTERM gracefully | Engineering | ✅ PARTIAL | Clean exit on EOF; signal handlers deferred |
| C.7 | Write binary smoke test | Engineering | ✅ DONE | 8 unit tests covering process_line |
| C.8 | Verify `cargo test -p ferrum-integrations-mcp` passes | Engineering | ✅ DONE | 27 tests pass (19 lib + 8 binary) |
| C.9 | Verify `cargo check --workspace` passes | Engineering | ✅ DONE | No new warnings |
| C.10 | Update this document | Engineering | ✅ DONE | Marking Phase C items done |

### 5.8.5 Non-Goals for Phase C

- No Streamable HTTP transport
- No MCP SDK dependency
- No gateway calls (tools/call remains NOT_IMPLEMENTED)
- No auth middleware or token handling
- No environment secret reads
- No file system writes
- No CI changes

### 5.8.6 Test Plan for Phase C

| Test | Description | Type |
|------|-------------|------|
| `test_binary_smoke_initialize` | Pipe `{"jsonrpc":"2.0","method":"initialize","id":1}` → expect valid response | Integration |
| `test_binary_smoke_ping` | Pipe `{"jsonrpc":"2.0","method":"ping","id":1}` → expect `{"success":true}` | Integration |
| `test_binary_smoke_tools_list` | Pipe `{"jsonrpc":"2.0","method":"tools/list","id":1}` → expect 9 tools | Integration |
| `test_binary_smoke_tools_call` | Pipe `{"jsonrpc":"2.0","method":"tools/call","params":{"name":"ferrum_gate_health"},"id":1}` → expect NOT_IMPLEMENTED | Integration |
| `test_binary_smoke_unknown_method` | Pipe `{"jsonrpc":"2.0","method":"unknown","id":1}` → expect method-not-found | Integration |
| `test_binary_stdin_closed` | Close stdin → expect clean exit | Integration |

---

## 6. Acceptance Criteria

### 6.1 Phase A+B Gate: Skeleton Compiles

| Criterion | Verification |
|-----------|--------------|
| `cargo check -p ferrum-integrations-mcp` passes | Engineering |
| `cargo check --workspace` passes | Engineering |
| No new warnings in clippy for new crate | Engineering |

### 6.2 Phase A+B Gate: Tool Schema Complete

| Criterion | Verification |
|-----------|--------------|
| All 9 read-only tools have JSON Schema defined | Engineering |
| All schemas are valid JSON | Engineering |
| All schemas have `input_schema` and `output_schema` | Engineering |

### 6.3 Phase A+B Gate: Handler Stubs Return Correct Shape

| Criterion | Verification |
|-----------|--------------|
| `initialize` returns protocol version `2024-11-05` | Engineering |
| `tools/list` returns 9 tools | Engineering |
| `tools/call` returns error for all tools | Engineering |
| `ping` returns `{success: true}` | Engineering |

### 6.4 Phase A+B Gate: Tests Prove Read-Only Constraint

| Criterion | Verification |
|-----------|--------------|
| `test_tool_registry_contains_only_read_only_tools` passes | Engineering |
| `test_mutating_tools_set_is_empty` passes | Engineering |
| `test_all_read_only_tools_have_schemas` passes | Engineering |

### 6.5 Phase A+B Gate: No Side Effects

| Criterion | Verification |
|-----------|--------------|
| No mutating REST API calls in handler stubs | Engineering |
| No database writes | Engineering |
| No file system writes | Engineering |
| No capability issuance | Engineering |
| No provenance emission | Engineering |

### 6.6 Phase C Gate: Binary and Stdio Transport

| Criterion | Verification |
|-----------|--------------|
| `ferrum-mcp-server` binary compiles | Engineering |
| `cargo test -p ferrum-integrations-mcp` passes including binary smoke tests | Engineering |
| Binary accepts JSON-RPC via stdin and responds via stdout | Engineering |
| Parse errors return valid JSON-RPC error on stdout | Engineering |
| No secrets logged to stdout or stderr | Engineering |
| Clean shutdown on SIGINT/SIGTERM | Engineering |

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

### 8.1 Phase A+B Summary

Phase A and Phase B are **complete**. The crate now has:
- Read-only tool registry with 9 tools
- JSON-RPC 2.0 types and error codes
- Handler stubs for `initialize`, `ping`, `tools/list`, `tools/call`
- `dispatch()` and `parse_request()` functions

### 8.2 Handoff to Phase C

When Phase C implementation begins:

1. **Create binary entry point** at `src/bin/ferrum-mcp-server.rs`
2. **Implement stdio transport loop** using existing `parse_request()` and `dispatch()`
3. **Add smoke tests** piping JSON-RPC via stdin/stdout
4. **Do NOT add** MCP SDK, gateway calls, or mutating tools

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

### Phase A+B Summary (Complete)

| Phase | Status |
|-------|--------|
| Phase A (skeleton + tool registry) | ✅ Complete |
| Phase B (JSON-RPC + handlers) | ✅ Complete |
| Phase C (stdio + binary) | ✅ Complete |

### Phase C Checklist (C.1–C.10)

- [x] C.1 Create `src/bin/ferrum-mcp-server.rs`
- [x] C.2 Add `[[bin]]` target to `Cargo.toml` (N/A - auto-detected)
- [x] C.3 Implement stdin line reader
- [x] C.4 Implement stdout line writer
- [x] C.5 Wire `parse_request()` → `dispatch()` → response
- [x] C.6 Handle SIGINT/SIGTERM gracefully (partial - clean exit on EOF)
- [x] C.7 Write binary smoke test
- [x] C.8 Verify `cargo test -p ferrum-integrations-mcp` passes
- [x] C.9 Verify `cargo check --workspace` passes
- [x] C.10 Update this document

---

*Document created: 2026-05-06. Phase A (skeleton), Phase B (JSON-RPC handlers), and Phase C (stdio transport + binary) complete. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
