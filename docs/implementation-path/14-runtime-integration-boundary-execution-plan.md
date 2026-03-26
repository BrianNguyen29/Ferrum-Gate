# 14 — Runtime Integration Boundary Execution Plan

Commit-by-commit plan for the first runtime integration boundary slice.
Grounded in existing repo reality: `ExternalEventIngestRequest` exists at
`crates/ferrum-proto/src/provenance.rs:146-179`, the endpoint handler lives at
`crates/ferrum-gateway/src/server.rs:2506-2636`, and `ferrumctl` already POSTs
to `/v1/provenance/events/external` at `bins/ferrumctl/src/main.rs:636`.

ASCII only.

---

## Slice Goals

- Prove the integration boundary by bridging external runtime events into the
  FerrumGate provenance graph via the existing external-event ingest API.
- Keep the core crates (ferrum-proto, ferrum-gateway, ferrum-store) vendor-neutral.
- Establish a minimal observation-only scaffold that future slices can extend
  with full MCP transport loops.

---

## Guardrails (This Slice Only)

1. **No auto anchor resolution** — callers must supply explicit
   `execution_id` AND `parent_event_id`; no lookup or inference.
2. **No retries** — one-shot POST; caller handles transient failures.
3. **No auto-correlation** — no background thread, no event-loop, no replay worker.
4. **No arbitrary per-call source_system override** — source_system is
   bridge-owned config set at construction time.
5. **No full MCP transport loop** — only a thin HTTP sink that wraps the
   existing external ingest API; no LSP protocol parsing, no server
   implementation, no client event-loop.
6. **Observation-only** — this bridge records what was observed; it does not
   command or mutateFerrumGate state.

---

## Commit 1: Add execution-plan doc and update README

**Files:**
- `docs/implementation-path/14-runtime-integration-boundary-execution-plan.md` (new)
- `docs/implementation-path/README.md` (update line 19-20 to add entry 14)

**Scope:**
- Add this doc with guardrails, commit plan, and slice status.
- Update README to include the new entry in order.

**Validation:**
- Doc exists at the expected path; README refers to it.

---

## Commit 2: Add ferrum-integrations-mcp crate scaffold

**Files:**
- `crates/ferrum-integrations-mcp/Cargo.toml` (new)
- `crates/ferrum-integrations-mcp/src/lib.rs` (new)
- `crates/ferrum-integrations-mcp/src/types.rs` (new)
- `crates/ferrum-integrations-mcp/src/error.rs` (new)
- `crates/ferrum-integrations-mcp/src/client.rs` (new)
- `crates/ferrum-integrations-mcp/src/bridge.rs` (new)
- `crates/ferrum-integrations-mcp/src/mapping.rs` (new)
- `crates/ferrum-integrations-mcp/src/vendors/mod.rs` (new)

**Root Cargo.toml changes:**
- Add `crates/ferrum-integrations-mcp` to workspace members
- Add `reqwest` and `url` to workspace.dependencies

**Crate dependencies:**
- `ferrum-proto` (for `ExternalEventIngestRequest`, `ExternalEventIngestResponse`,
  `ProvenanceEvent`, `ExecutionId`, `EventId`, `Timestamp`)
- `reqwest` (HTTP client)
- `url` (URL parsing for gateway endpoint)
- `anyhow`, `thiserror` (error handling)
- `serde`, `serde_json` (serialization)

**Validation:**
- `cargo check -p ferrum-integrations-mcp` passes with zero warnings.

---

## Commit 3: Implement minimal public API

**Module responsibilities:**

| Module | Responsibility |
|--------|---------------|
| `types.rs` | Local Rust types for MCP runtime events (one variant per event type). |
| `error.rs` | Crate-local `Error` type; `From<reqwest::Error>` and `From<serde_json::Error>`. |
| `client.rs` | Thin HTTP client wrapping reqwest; one `post_external_event` method. |
| `mapping.rs` | Strict `map_to_ingest_request` converting local types to `ExternalEventIngestRequest`. |
| `bridge.rs` | `McpBridge` struct holding gateway URL + source_system; single `ingest` method. |
| `vendors/mod.rs` | Vendor-specific mapping helpers (placeholder for this slice). |
| `lib.rs` | Re-exports and docs. |

**Key API shape (lib.rs exports):**
```rust
pub use bridge::McpBridge;
pub use error::Error;
pub use types::McpRuntimeEvent;
```

**McpBridge::ingest signature:**
```rust
pub async fn ingest(
    &self,
    execution_id: ExecutionId,
    parent_event_id: EventId,
    event: McpRuntimeEvent,
) -> Result<ProvenanceEvent, Error>
```

**Guardrail enforcement:**
- `source_system` is stored in `McpBridge`, not passed per-call.
- Both `execution_id` and `parent_event_id` are required arguments (no Option).

**Validation:**
- Unit tests for `mapping.rs` covering:
  - all required fields present in mapped request
  - optional fields omitted when None
  - source_system copied from bridge config
  - payload_digest computed from event JSON when present

---

## Commit 4: Add reqwest and url to workspace.dependencies

**Scope:**
- Add to `[workspace.dependencies]` in root `Cargo.toml`:
  - `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }`
  - `url = "2"`

**Validation:**
- Workspace `cargo metadata` resolves without errors.

---

## Slice Status: IN PROGRESS

Work in progress. This plan is the living record for this slice.

---

## Future Backlog (Out of Scope for This Slice)

- Full MCP server transport loop (MCP handshake, protocol negotiation)
- Auto anchor resolution (lookup execution_id from context)
- Persistent dedupe / idempotency layer
- Background replay worker
- Multiple simultaneous vendor bridges
- MCP client capability negotiation

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-proto/src/provenance.rs:146-179` | `ExternalEventIngestRequest` proto type |
| `crates/ferrum-gateway/src/server.rs:2506-2636` | External event ingest endpoint handler |
| `bins/ferrumctl/src/main.rs:636` | Example POST to `/v1/provenance/events/external` |
| `crates/ferrum-integrations-mcp/src/lib.rs` | New crate public API entry |
| `crates/ferrum-integrations-mcp/src/bridge.rs` | `McpBridge` type holding config |
| `crates/ferrum-integrations-mcp/src/mapping.rs` | Strict request mapping |
| `crates/ferrum-integrations-mcp/src/client.rs` | Thin reqwest HTTP wrapper |

---

## Recommended Next Slice

Prove end-to-end: spin up a local ferrumd, send an MCP runtime event through
the bridge, and verify the event appears in the provenance graph via the query API.

Source: this doc, Future Backlog section above.
