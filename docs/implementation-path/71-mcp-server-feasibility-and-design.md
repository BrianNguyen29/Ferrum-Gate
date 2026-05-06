# 71 — MCP Server Feasibility and Design

> **Status**: Design documentation only. No implementation.
> **Purpose**: Detailed planning reference for building FerrumGate into an MCP server for AI agents, based on completed feasibility research.
> **Scope**: FerrumGate v1.4 MCP Governance Beta (post-v1). Not v1 single-node scope.
> **Constraint**: Docs-only. Do not implement MCP server. Do not claim production-ready, G2 complete, MCP server ready, or target evidence collected.

---

## Explicit Non-Claims

- **FerrumGate is NOT currently an MCP server.** FerrumGate has an MCP bridge/client skeleton (`McpBridge` in `ferrum-sync/src/mcp_bridge.rs`), which allows FerrumGate to call external MCP runtimes. It does not currently act as an MCP server that AI agents can call.
- **No MCP server implementation exists.** This document is a design and todo-list for future work. It does not represent existing code.
- **MCP server is post-v1 scope.** MCP Governance Beta is targeted for v1.4, which is explicitly outside the v1 single-node support baseline per `../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`.
- **No production-ready claim.** MCP server implementation does not make FerrumGate production-ready.
- **No G2 complete claim.** G2.1–G2.8 remain pending regardless of MCP server progress.
- **No target evidence.** No target-host execution evidence has been collected for MCP flows.

---

## 1. Current State

### 1.1 What Exists Today

FerrumGate currently has an **MCP bridge/client skeleton**, not an MCP server:

| Component | Location | Description |
|-----------|----------|-------------|
| `RuntimeBridge` trait | `ferrum-sync/src/mcp_bridge.rs:43` | Extended trait for runtime bridges with tool discovery and event submission |
| `McpBridge` struct | `ferrum-sync/src/mcp_bridge.rs:61` | MCP bridge implementation (client-side); wraps a `BoxedTransport` for future real transport usage |
| `BridgeToolInfo` struct | `ferrum-sync/src/mcp_bridge.rs:16` | Tool metadata (name, description, input_schema) |
| `BridgeSubmitResult` struct | `ferrum-sync/src/mcp_bridge.rs:27` | Result of submitting an event through a runtime bridge |
| `GET /v1/bridges` endpoint | `ferrum-gateway/src/server.rs` | Lists available runtime bridges |
| `GET /v1/bridges/{id}/tools` endpoint | `ferrum-gateway/src/server.rs` | Lists tools for a specific bridge |
| `GatewayRuntime.bridges` | `ferrum-gateway/src/state.rs:15` | `Vec<Arc<dyn RuntimeBridge>>` field for registered bridges |
| `ActionType::McpToolMutation` | `ferrum-proto` | Action type for MCP tool mutations |
| `RollbackTarget::Generic` | `ferrum-rollback` | Generic rollback target for bridge operations |
| U4 (MCP/local/NemoClaw integrations) | Roadmap v1.4 | Planned as post-v1 MCP Governance Beta |

### 1.2 What This Means

The existing `McpBridge` is a **client-side** component that allows FerrumGate to:
1. Connect to an external MCP runtime
2. Discover tools available in that runtime
3. Submit events to that runtime

This is the inverse of an MCP server, which would allow an AI agent to:
1. Connect to FerrumGate as an MCP server
2. Discover what tools FerrumGate exposes
3. Call those tools via FerrumGate's governance pipeline (intent → capability → provenance → rollback)

### 1.3 Relationship to Roadmap

| Roadmap Item | Status | Relationship |
|-------------|--------|--------------|
| U4 Runtime Integrations (MCP bridge) | ✅ Done (post-v1) | Client-side MCP bridge exists |
| v1.4 MCP Governance Beta | ☐ Planned (post-v1) | Server-side MCP server target |
| Q4 MCP Governance Beta (roadmap v2) | ☐ Planned (post-v1) | Same as v1.4 |

---

## 2. MCP Protocol Requirements Summary

### 2.1 Core Protocol Elements

An MCP server must implement the following:

| Element | Description | Required |
|---------|-------------|----------|
| **JSON-RPC 2.0** | All messages use JSON-RPC 2.0 format | Yes |
| **Initialization** | `initialize` handshake: server info, capabilities, protocol version | Yes |
| **Protocol version** | MCP uses `2024-11-05` (or similar dated version) | Yes |
| **tools/list** | List all available tools with names, descriptions, input schemas | Yes |
| **tools/call** | Execute a named tool with JSON arguments | Yes |
| **resources/** | Resource handlers (optional but common) | No |
| **prompts/** | Prompt templates (optional) | No |
| **Sampling** | Server-initiated sampling (optional) | No |

### 2.2 Transport Options

| Transport | Description | FerrumGate Status |
|-----------|-------------|-------------------|
| **stdio** | JSON-RPC over stdin/stdout | Not implemented; primary choice for local AI agent integration |
| **Streamable HTTP** | HTTP POST + SSE for notifications | Not implemented; suitable for remote AI agent integration |
| **HTTP** | Simple request/response (deprecated in MCP) | Not applicable |

### 2.3 Security Requirements

| Requirement | Description |
|-------------|-------------|
| **Authentication** | Bearer token or similar; agents must authenticate |
| **Authorization** | Every tool call must go through policy evaluation |
| **Capability scoping** | Tool calls require single-use capabilities with TTL ≤ 300s |
| **Provenance** | All tool executions emit provenance events |
| **Rollback** | Tool executions must have rollback prepare before commit |
| **Input validation** | Tool arguments validated against JSON Schema |
| **Output sanitization** | Tool outputs sanitized before return |
| **Rate limiting** | Per-agent rate limits to prevent spam |

---

## 3. Existing Pieces Available for MCP Server

| Piece | Location | Utility for MCP Server |
|-------|----------|------------------------|
| `RuntimeBridge` trait | `ferrum-sync/src/mcp_bridge.rs:43` | Can be adapted for inbound tool registration |
| `McpBridge` struct | `ferrum-sync/src/mcp_bridge.rs:61` | Reference implementation for bridge patterns |
| `BridgeToolInfo` struct | `ferrum-sync/src/mcp_bridge.rs:16` | Tool metadata format (reusable for MCP tool schema) |
| `GatewayRuntime.bridges` | `ferrum-gateway/src/state.rs:15` | Runtime bridge registry (source model for tool registry) |
| `ActionType::McpToolMutation` | `ferrum-proto` | Already defined for MCP tool actions |
| `RollbackTarget::Generic` | `ferrum-rollback` | Generic rollback for bridge operations |
| `ferrum-gateway` REST API | `ferrum-gateway/src/server.rs` | Governance pipeline reference |
| Provenance chain | `ferrum-proto` | Existing lineage infrastructure |
| Policy engine (PDP) | `ferrum-pdp` | Existing policy evaluation |
| Capability system | `ferrum-cap` | Existing single-use capability issuance |
| Rollback system | `ferrum-rollback` | Existing compensation/rollback infrastructure |

---

## 4. Missing Pieces for MCP Server

| Missing Piece | Description | Complexity |
|--------------|-------------|------------|
| **JSON-RPC 2.0 server** | HTTP or stdio server that parses JSON-RPC requests and returns JSON-RPC responses | High |
| **`initialize` handler** | Protocol handshake: receive client info, return server capabilities and protocol version | Medium |
| **`tools/list` handler** | Return all FerrumGate tools with name, description, input_schema matching MCP schema | Medium |
| **`tools/call` handler** | Map MCP tool call to governance pipeline (intent → capability → execute → verify → compensate) | High |
| **Resources handlers** | If supporting MCP resources (file templates, etc.) | Low |
| **Prompts handlers** | If supporting MCP prompts | Low |
| **stdio transport** | Read JSON-RPC from stdin, write to stdout; for local AI agent integration | Medium |
| **Streamable HTTP transport** | HTTP POST endpoint + SSE for server-initiated notifications | Medium |
| **MCP SDK dependency** | Currently no MCP SDK in `Cargo.toml`; would need `mcp` crate | Medium |
| **Auth metadata in MCP** | Map MCP auth (bearer token, etc.) to FerrumGate `ActorRef` | Medium |
| **Actor identity mapping** | Map MCP client identity to FerrumGate `ActorRef` for provenance | Medium |
| **MCP tool call → governance pipeline mapper** | Convert MCP tool name + args → `ActionProposal` → policy evaluation → execution | High |
| **Tool output → JSON-RPC response serializer** | Convert FerrumGate tool output to MCP `CallToolResult` schema | Medium |
| **Rollback integration** | MCP tool calls must prepare rollback before execution; compensate on failure | High |

---

## 5. Architecture Options

### 5.1 Option A — MCP-to-REST Adapter (Recommended for MVP)

```
AI Agent (MCP Client)
    │
    │ stdio or Streamable HTTP
    │
    ▼
┌─────────────────────────────────────────────────┐
│  ferrum-mcp-server (NEW binary)                  │
│  - JSON-RPC 2.0 server (stdio or HTTP+SSE)      │
│  - maps MCP protocol → FerrumGate REST API       │
│  - handles initialize, tools/list, tools/call    │
│  - validates auth, maps ActorRef                 │
│  - converts tool output → CallToolResult         │
└─────────────────────────────────────────────────┘
    │
    │ REST API (internal network or UDS)
    │
    ▼
┌─────────────────────────────────────────────────┐
│  ferrumd (existing REST API server)             │
│  - full governance pipeline (intent→cap→prov→rollback) │
│  - policy engine, capability system              │
│  - adapters (fs, git, http, etc.)               │
└─────────────────────────────────────────────────┘
```

**Pros:**
- Clear separation: MCP protocol handling separate from governance kernel
- Existing REST API already has full governance pipeline
- Can iterate on MCP protocol without touching core
- Single binary for MCP server, simple deployment
- Works with existing `ferrumd` without forking
- `ferrum-mcp-server` can be independently deployed/scaled

**Cons:**
- Two processes instead of one
- Internal REST latency overhead
- Need to maintain auth pass-through

**Recommended naming:** `crates/ferrum-mcp-server` → binary `ferrum-mcp-server`

### 5.2 Option B — Embedded MCP Server (Not Recommended for MVP)

```
┌─────────────────────────────────────────────────┐
│  ferrumd (MODIFIED)                             │
│  - existing REST API server                     │
│  + new MCP transport (stdio or HTTP+SSE)        │
│  + new MCP handlers (initialize, tools/list,    │
│    tools/call)                                  │
│  + MCP-to-governance pipeline mapper            │
└─────────────────────────────────────────────────┘
```

**Pros:**
- Single process
- No inter-process communication

**Cons:**
- Bloats `ferrumd` with MCP-specific code
- Mixed concerns: REST API + MCP protocol in same binary
- Harder to test MCP in isolation
- Couples MCP evolution to core release cycle
- Violates separation of concerns principle

**Verdict:** Not recommended for MVP. Option A is preferred.

### 5.3 Option C — Direct Standalone Tool Runner (Rejected)

```
AI Agent → ferrum-mcp-standalone → Tool execution
```

**Verdict:** Rejected. Bypasses FerrumGate governance entirely. Defeats the purpose of FerrumGate as an intent-scoped reversible execution plane.

---

## 6. Recommended Crate/Binary Naming

### 6.1 Naming Options

| Option | Crate | Binary | Notes |
|--------|-------|--------|-------|
| A | `ferrum-mcp-server` | `ferrum-mcp-server` | Clear, focused, matches crate naming conventions |
| B | `ferrum-integrations-mcp` | `ferrum-integrations-mcp` | Aligns with roadmap v1 `90-upgrade-and-integration-plan.md` which proposes `ferrum-integrations-mcp` |
| C | `ferrum-mcp` | `ferrum-mcp` | Shorter but less descriptive |

### 6.2 Recommendation

**Option B (`ferrum-integrations-mcp`)** is preferred because:

1. **Roadmap alignment:** The existing `90-upgrade-and-integration-plan.md` §5.1 already proposes `ferrum-integrations-mcp` as the integration crate name
2. **Future-proofing:** Allows adding `ferrum-integrations-local`, `ferrum-integrations-nemoclaw` under the same umbrella
3. **Consistency:** Matches the adapter crate naming pattern (`ferrum-adapter-fs`, `ferrum-adapter-git`, etc.)
4. **Clear intent:** Signals this is an integration layer, not core functionality

However, **Option A (`ferrum-mcp-server`)** is acceptable if:
- MCP server is the only planned integration
- Simpler naming is preferred over future-proofing
- No plans for `ferrum-integrations-*` family

**This document assumes Option B (`ferrum-integrations-mcp`) per roadmap alignment.**

---

## 7. Tool Surface Phases

### Phase 1 — Read-Only Tools (MVP 0)

Start with tools that have no side effects (or minimal, reversible ones):

| Tool | Description | Risk |
|------|-------------|------|
| `ferrum__list_policies` | List available policy bundles | Low |
| `ferrum__list_intents` | List intents matching filters | Low |
| `ferrum__get_intent` | Get intent details by ID | Low |
| `ferrum__get_execution` | Get execution status | Low |
| `ferrum__list_capabilities` | List capabilities for an intent | Low |
| `ferrum__probe` | Health/readiness probe | Low |
| `ferrum__list_bridges` | List registered runtime bridges | Low |

### Phase 2 — Governed Mutating Tools (MVP 1)

Tools that modify state but go through full governance pipeline:

| Tool | Description | Risk |
|------|-------------|------|
| `ferrum__submit_intent` | Submit a new intent | Medium |
| `ferrum__evaluate_intent` | Evaluate intent against policies | Medium |
| `ferrum__prepare_execution` | Prepare execution with rollback | Medium |
| `ferrum__execute_prepared` | Execute prepared action | High |
| `ferrum__compensate` | Rollback an execution | High |

### Phase 3 — Adapter-Backed Tools (MVP 2)

Tools backed by FerrumGate adapters:

| Tool | Adapter | Risk |
|------|---------|------|
| `ferrum__fs__read_file` | fs adapter | Medium |
| `ferrum__fs__write_file` | fs adapter | High |
| `ferrum__git__create_branch` | git adapter | High |
| `ferrum__git__commit` | git adapter | High |
| `ferrum__http__request` | http adapter | High |
| `ferrum__sqlite__query` | sqlite adapter | High |

### Phase 4 — Adapter-Specific Tools (MVP 3)

Narrow, specific tools per adapter with tight scope:

| Tool | Adapter | Risk |
|------|---------|------|
| `ferrum__fs__safe_read` | fs adapter | Low |
| `ferrum__git__safe_branch` | git adapter | Medium |
| `ferrum__http__bounded_post` | http adapter | Medium |

---

## 8. Security Gates

Every MCP tool call must pass through the following gates before execution:

| Gate | Description | FerrumGate Component |
|------|-------------|----------------------|
| **1. Authentication** | Verify MCP client credentials (bearer token) | `ferrum-mcp-server` auth middleware |
| **2. Actor Identity Mapping** | Map MCP client → `ActorRef` for provenance | `ferrum-mcp-server` identity mapper |
| **3. Tool Discovery Validation** | Only expose tools explicitly registered in tool registry | `ferrum-mcp-server` tool registry |
| **4. Policy Evaluation** | Evaluate tool call against policy bundle | `ferrum-pdp` |
| **5. Capability Issuance** | Mint single-use capability with TTL ≤ 300s | `ferrum-cap` |
| **6. Scope Validation** | Verify tool call scope matches capability scope | `ferrum-pdp` scope checker |
| **7. Rollback Prepare** | Prepare rollback contract before execution | `ferrum-rollback` |
| **8. Provenance Emission** | Emit `ActionProposalSubmitted`, `ToolCallPrepared` events | `ferrum-gateway` |
| **9. Output Sanitization** | Sanitize tool output before return to agent | `ferrum-firewall` |
| **10. Default-Deny Unknown** | Unknown/noop tools return error, not success | `ferrum-mcp-server` default handler |

### 8.1 Per-Agent Rate Limits

| Limit | Value | Enforcement |
|-------|-------|-------------|
| Requests per second | 2 req/s (default, configurable) | `tower_governor` |
| Burst | 50 (default, configurable) | `tower_governor` |
| Concurrent tool calls | Bounded by governance pipeline | `ferrum-gateway` semaphore |

### 8.2 Auth Metadata

| Field | Description |
|-------|-------------|
| `actor_type` | `Agent` for MCP clients |
| `actor_id` | MCP client identity (from bearer token subject) |
| `display_name` | Optional human-readable agent name |
| `auth_method` | `Bearer` for MCP stdio/HTTP |

---

## 9. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Agent calls unknown/noop tools** | Medium | High | Default-deny unknown tools; explicit tool registry |
| **Agent bypasses governance** | Low | Critical | All tool calls go through `ferrum-mcp-server` → REST API → governance pipeline |
| **Auth weakness** | Medium | High | Require bearer token; validate on every request |
| **Scope too broad** | Medium | High | Policy evaluation with scope mismatch → deny |
| **Agent spam** | Medium | Medium | Rate limiting; per-agent limits |
| **Protocol churn** | Medium | Low | Pin MCP protocol version; document version requirements |
| **Latency** | Medium | Low | Option A adapter pattern; internal REST call overhead |
| **Rollback not prepared** | Low | High | Enforce rollback prepare before execute in governance pipeline |

---

## 10. Todo-List Roadmap

### Phase A — Crate and Binary Setup

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| A.1 | Create `crates/ferrum-integrations-mcp` crate | Engineering | ☐ TODO | New crate; follow existing crate patterns |
| A.2 | Add `ferrum-mcp-server` binary | Engineering | ☐ TODO | New binary in `bins/` or `src/bin/` |
| A.3 | Add MCP SDK dependency (`mcp` crate) to `Cargo.toml` | Engineering | ☐ TODO | Do not add to workspace unless needed |
| A.4 | Set up `serde_json` + `tokio` for async JSON-RPC | Engineering | ☐ TODO | Reuse existing deps |
| A.5 | Create initial module structure | Engineering | ☐ TODO | `src/server.rs`, `src/handlers.rs`, `src/transport.rs` |

### Phase B — JSON-RPC Server Skeleton

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| B.1 | Implement stdio transport (read stdin, write stdout) | Engineering | ☐ TODO | Primary choice for local AI agent integration |
| B.2 | Implement JSON-RPC 2.0 request parser | Engineering | ☐ TODO | Reuse or wrap existing `serde_json` |
| B.3 | Implement JSON-RPC 2.0 response serializer | Engineering | ☐ TODO | Must produce valid JSON-RPC 2.0 responses |
| B.4 | Implement `handle_batch` for batch requests | Engineering | ☐ TODO | MCP allows batch requests |
| B.5 | Implement error response format per JSON-RPC 2.0 | Engineering | ☐ TODO | `-32600` (Invalid Request), `-32601` (Method not found), etc. |

### Phase C — MCP Protocol Handlers

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| C.1 | Implement `initialize` handler | Engineering | ☐ TODO | Receive client info, return server capabilities, protocol version |
| C.2 | Implement `tools/list` handler | Engineering | ☐ TODO | Return FerrumGate tool registry as MCP tool list |
| C.3 | Implement `tools/call` handler | Engineering | ☐ TODO | Map to governance pipeline; return `CallToolResult` |
| C.4 | Implement `ping` handler | Engineering | ☐ TODO | Simple liveness check |
| C.5 | Implement `resources/list` handler (optional) | Engineering | ☐ TODO | If supporting MCP resources |
| C.6 | Implement `prompts/list` handler (optional) | Engineering | ☐ TODO | If supporting MCP prompts |

### Phase D — Governance Pipeline Integration

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| D.1 | Add MCP auth middleware to `ferrum-mcp-server` | Engineering | ☐ TODO | Validate bearer token on every request |
| D.2 | Implement ActorRef mapping (MCP client → FerrumGate actor) | Engineering | ☐ TODO | Map MCP identity to `ActorRef` for provenance |
| D.3 | Implement tool call → `ActionProposal` mapper | Engineering | ☐ TODO | Convert MCP tool name + args to `ActionProposal` |
| D.4 | Integrate with REST API for governance pipeline | Engineering | ☐ TODO | `ferrum-mcp-server` → `ferrumd` REST API |
| D.5 | Implement `CallToolResult` → JSON-RPC response serializer | Engineering | ☐ TODO | Convert FerrumGate tool output to MCP schema |
| D.6 | Implement rollback prepare on tool call | Engineering | ☐ TODO | Enforce rollback prepare before execute |
| D.7 | Implement output sanitization | Engineering | ☐ TODO | Sanitize tool output before return |
| D.8 | Implement provenance event emission | Engineering | ☐ TODO | Emit `ToolCallPrepared`, `ToolCallExecuted` events |
| D.9 | Implement default-deny for unknown tools | Engineering | ☐ TODO | Return error for tools not in registry |
| D.10 | Implement per-agent rate limiting | Engineering | ☐ TODO | Reuse `tower_governor` |

---

### MVP Milestones

#### MVP 0 — Read-Only Tool Surface

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| M0.1 | `ferrum__list_policies` | Engineering | ☐ TODO | |
| M0.2 | `ferrum__list_intents` | Engineering | ☐ TODO | |
| M0.3 | `ferrum__get_intent` | Engineering | ☐ TODO | |
| M0.4 | `ferrum__probe` | Engineering | ☐ TODO | |

**MVP 0 gate:** AI agent can discover and query FerrumGate state without making changes.

#### MVP 1 — Governed Intent Submission

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| M1.1 | `ferrum__submit_intent` | Engineering | ☐ TODO | Full governance pipeline |
| M1.2 | `ferrum__evaluate_intent` | Engineering | ☐ TODO | Policy evaluation only |
| M1.3 | `ferrum__prepare_execution` | Engineering | ☐ TODO | Rollback prepare |
| M1.4 | `ferrum__execute_prepared` | Engineering | ☐ TODO | Execute with rollback |

**MVP 1 gate:** AI agent can submit intents and execute governed actions with full rollback support.

#### MVP 2 — Adapter-Backed Tools

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| M2.1 | `ferrum__fs__read_file` | Engineering | ☐ TODO | fs adapter |
| M2.2 | `ferrum__fs__write_file` | Engineering | ☐ TODO | fs adapter |
| M2.3 | `ferrum__git__create_branch` | Engineering | ☐ TODO | git adapter |
| M2.4 | `ferrum__http__bounded_post` | Engineering | ☐ TODO | http adapter |

**MVP 2 gate:** AI agent can perform real file, git, and HTTP operations through FerrumGate with rollback.

#### MVP 3 — Production Hardening

| # | Item | Owner | Status | Notes |
|---|------|-------|--------|-------|
| M3.1 | Full auth integration | Engineering | ☐ TODO | Bearer token validation |
| M3.2 | Full provenance emission | Engineering | ☐ TODO | All tool calls emit lineage |
| M3.3 | Per-agent rate limits | Engineering | ☐ TODO | Rate limit enforcement |
| M3.4 | Integration tests | Engineering | ☐ TODO | End-to-end MCP → governance → rollback |
| M3.5 | Load testing | Engineering | ☐ TODO | Performance under MCP agent load |

**MVP 3 gate:** FerrumGate MCP server is production-hardened for AI agent use.

---

## 11. Do and Don't Recommendations

### Do

| Recommendation | Reason |
|----------------|--------|
| **Do use Option A (MCP-to-REST adapter)** | Clear separation; reuses existing governance pipeline |
| **Do start with stdio transport** | Simplest for local AI agent integration |
| **Do start with read-only tools (MVP 0)** | Lower risk; validate protocol handling first |
| **Do enforce all security gates** | Auth, capability, scope, rollback, provenance are non-negotiable |
| **Do use `ferrum-integrations-mcp` naming** | Aligns with roadmap v1 `90-upgrade-and-integration-plan.md` |
| **Do emit provenance for every tool call** | Core FerrumGate invariant |
| **Do prepare rollback before every execution** | Core FerrumGate invariant |
| **Do validate tool arguments against JSON Schema** | Input validation prevents injection |
| **Do sanitize tool outputs** | Output sanitization prevents data leakage |
| **Do default-deny unknown tools** | Fail-closed behavior |

### Don't

| Recommendation | Reason |
|----------------|--------|
| **Don't implement MCP server in `ferrumd`** | Violates separation of concerns; bloats core |
| **Don't bypass the governance pipeline** | Defeats FerrumGate purpose as execution governance layer |
| **Don't expose all internal tools** | Only expose explicitly registered, policy-approved tools |
| **Don't skip rollback prepare** | Core invariant; required for reversible execution |
| **Don't skip provenance emission** | Core invariant; required for lineage |
| **Don't trust tool outputs without sanitization** | Tool outputs may contain sensitive data |
| **Don't allow unbounded rate limits** | Prevents agent spam |
| **Don't skip auth validation** | Every MCP client must authenticate |
| **Don't use Option C (direct tool runner)** | Bypasses governance entirely |
| **Don't claim production-ready** | MCP server is post-v1; v1 is RC-ready/conditional only |

---

## 12. Final Verdict

### Feasibility: YES

Building FerrumGate into an MCP server is **feasible and recommended** as a governed MCP server. The existing codebase provides:
- Strong governance kernel (intent → capability → provenance → rollback)
- Policy engine for authorization
- Provenance infrastructure for lineage
- Rollback infrastructure for compensation
- Adapter layer for real tool execution

### Recommended Approach

**Option A (MCP-to-REST adapter)** with **ferrum-integrations-mcp** crate and **ferrum-mcp-server** binary. This:
- Preserves separation of concerns
- Reuses existing governance pipeline
- Allows independent iteration on MCP protocol
- Aligns with roadmap v1 naming conventions

### Preconditions

Before MCP server implementation begins:
1. v1 single-node RC must be stable (current priority)
2. G2 pilot readiness gates should be passed (operator-owned)
3. MCP Governance Beta (v1.4) should be formally scoped in release plan

### Next Steps

1. Update `../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/02-release-plan.md` §Release 4 (v1.4) with concrete MCP server scope when ready
2. Create `crates/ferrum-integrations-mcp` crate following Phase A checklist
3. Implement stdio JSON-RPC server skeleton (Phase B)
4. Implement `initialize`, `tools/list`, `tools/call` handlers (Phase C)
5. Integrate with governance pipeline (Phase D)
6. Progress through MVP 0 → 1 → 2 → 3

---

## 13. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope; MCP Governance Beta (v1.4) |
| This doc | [`../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md`](../ferrumgate-roadmap-v1/90-upgrade-and-integration-plan.md) | MCP integration naming and approach alignment |
| This doc | [`../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/02-release-plan.md`](../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/02-release-plan.md) | v1.4 MCP Governance Beta scope |
| This doc | [`01-current-state.md`](01-current-state.md) | U4 (MCP bridge) done; MCP server is next step |
| This doc | [`09-phase-checklists.md`](09-phase-checklists.md) | Phase D adapter status (fs, git, http, sqlite) |
| This doc | [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) | v1 boundary; MCP server is out-of-scope for v1 |
| This doc | [`70-security-hardening-local-only-plan.md`](70-security-hardening-local-only-plan.md) | Security hardening reference for MCP server |

---

*Document created: 2026-05-06. Design documentation only. No implementation. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
