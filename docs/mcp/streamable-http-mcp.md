# Streamable HTTP MCP Transport — Phase 6.1 Skeleton

## Status

**Phase 6.1: IMPLEMENTED** (skeleton only)  
**Phase 6.2+: NOT COMPLETE** (SSE streaming, sessions, resumability deferred)

## What is implemented

Phase 6.1 provides a bounded Streamable HTTP transport skeleton for `ferrum-mcp-server`.

- CLI args `--transport stdio|http` and `--bind ADDR`
  - Default: `stdio` transport, bind `127.0.0.1:3000`
- `GET /health` — basic JSON health probe
- `GET /ready` — shallow readiness with tool count and transport metadata
- `POST /mcp` — accepts a single JSON-RPC request body, dispatches through the existing `dispatch_with_client()`, and returns a synchronous JSON-RPC response as `application/json`
- `GET /mcp` — returns `405 Method Not Allowed` with a JSON body explaining that SSE streaming is deferred

## Architecture

```
MCP Client (HTTP)
   |
   POST /mcp  (application/json)
   |
ferrum-mcp-server (axum)
   |
   tokio::task::spawn_blocking
   |
   dispatch_with_client()  (existing sync dispatch)
   |
   FerrumGatewayClient  (reqwest::blocking)
   |
   FerrumGate Gateway REST API
```

The blocking `FerrumGatewayClient` is wrapped in `tokio::task::spawn_blocking` inside the async HTTP handler to avoid blocking the async executor. No governance or tool semantics were changed.

## Security notes

- **Bind address default**: `127.0.0.1:3000` (localhost only). Do not bind to `0.0.0.0` without a reverse proxy or tunnel in front.
- **Origin validation**: Not implemented in Phase 6.1. If exposing the MCP HTTP endpoint beyond localhost, place a reverse proxy (e.g., nginx, Caddy, Cloudflare Tunnel) in front and enforce origin/Host validation there.
- **No OAuth / auth middleware**: MCP HTTP transport does not add new auth. Existing gateway bearer-token behavior continues to apply inside `dispatch_with_client()`.
- **No TLS termination**: The skeleton does not terminate TLS. Use a reverse proxy or secure tunnel for TLS.

## Spec compliance and caveats

This is a **skeleton**, not a full Streamable HTTP implementation.

| Spec feature | Status | Notes |
|--------------|--------|-------|
| Single MCP endpoint (`/mcp`) | Partial | `POST` supported; `GET` returns 405 |
| Synchronous JSON response (`POST`) | Implemented | Returns `application/json` |
| SSE streaming (`GET`) | **Deferred** | Returns 405; full SSE/multiplexing deferred to Phase 6.2+ |
| `MCP-Protocol-Version` header | Not implemented | Phase 6.1 does not enforce strict headers |
| Session management | **Deferred** | No session store, no `Mcp-Session-Id` |
| Resumability | **Deferred** | No event ID store, no replay buffer |
| DELETE session termination | **Deferred** | No session concept yet |
| Strict SEP-2243 headers | **Deferred** | Not enforced in skeleton |

## Deferred items (Phase 6.2+)

- SSE streaming and multiplexing for `GET /mcp`
- Session state management (`Mcp-Session-Id`)
- Resumability with event ID tracking
- `MCP-Protocol-Version` header negotiation
- DELETE `/mcp` session termination
- Strict SEP-2243 header compliance
- OAuth / auth middleware specifically for MCP HTTP transport

## Non-claims

- This is **not** a production-ready MCP HTTP server.
- This is **not** certified compatible with any external MCP client.
- No SLO / latency guarantees are claimed for the HTTP transport path.
- No real domain / public endpoint claim is made.

## Running the HTTP transport

```bash
# Default stdio mode
./ferrum-mcp-server

# HTTP mode on default localhost:3000
./ferrum-mcp-server --transport http

# HTTP mode on custom bind
./ferrum-mcp-server --transport http --bind 127.0.0.1:8080
```

Example `POST /mcp` request:

```bash
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}'
```

## Related documents

- [`docs/plan.md`](../plan.md) — Phase 6 tracking
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — recommended deployment behind a tunnel
