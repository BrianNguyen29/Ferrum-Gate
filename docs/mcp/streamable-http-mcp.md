# Streamable HTTP MCP Transport — Skeleton

## What is implemented

This provides a bounded Streamable HTTP transport skeleton for `ferrum-mcp-server`.

- CLI args `--transport stdio|http` and `--bind ADDR`
  - Default: `stdio` transport, bind `127.0.0.1:3000`
- `GET /health` — basic JSON health probe
- `GET /ready` — shallow readiness with tool count and transport metadata
- `POST /mcp` — accepts a single JSON-RPC request body, dispatches through the existing `dispatch_with_client()`, and returns a synchronous JSON-RPC response as `application/json`
- `GET /mcp` — returns `405 Method Not Allowed` with a JSON body explaining that SSE streaming is not provided by this endpoint

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
- **Origin validation**: Not provided. If exposing the MCP HTTP endpoint beyond localhost, place a reverse proxy (e.g., nginx, Caddy, Cloudflare Tunnel) in front and enforce origin/Host validation there.
- **No OAuth / auth middleware**: MCP HTTP transport does not add new auth. Existing gateway bearer-token behavior continues to apply inside `dispatch_with_client()`.
- **No TLS termination**: The skeleton does not terminate TLS. Use a reverse proxy or secure tunnel for TLS.

## Spec compliance and caveats

This is a **skeleton**, not a full Streamable HTTP implementation.

| Spec feature | Status | Notes |
|--------------|--------|-------|
| Single MCP endpoint (`/mcp`) | Partial | `POST` supported; `GET` returns 405 |
| Synchronous JSON response (`POST`) | Implemented | Returns `application/json` |
| SSE streaming (`GET`) | Not provided | Returns 405; full SSE/multiplexing not provided by this endpoint |
| `MCP-Protocol-Version` header | Not provided | Skeleton does not enforce strict headers |
| Session management | Not provided | No session store, no `Mcp-Session-Id` |
| Resumability | Not provided | No event ID store, no replay buffer |
| DELETE session termination | Not provided | No session concept yet |
| Strict SEP-2243 headers | Not provided | Not enforced in skeleton |

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

- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — recommended deployment behind a tunnel
