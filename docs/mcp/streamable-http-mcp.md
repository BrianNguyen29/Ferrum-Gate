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

- **Bind address default**: `127.0.0.1:3000` (localhost only). Non-loopback binds are refused unless `--allow-insecure-nonlocal-bind` is passed.
- **Bearer auth mandatory by default**: `POST /mcp` requires a valid `Authorization: Bearer <token>` header. The token is read from `FERRUM_MCP_HTTP_BEARER_TOKEN` (falling back to `FERRUM_GATEWAY_BEARER_TOKEN`). Use `--allow-insecure-no-auth` only for local development.
- **Host validation**: The `Host` header is checked against an allowlist. Defaults allow `localhost`, `127.0.0.1`, and `[::1]`; customize with `--allowed-host` or `FERRUM_MCP_ALLOWED_HOSTS` (comma-separated).
- **Origin validation**: Any `Origin` header is rejected by default. Allow specific origins with `--allowed-origin` or `FERRUM_MCP_ALLOWED_ORIGINS` (comma-separated).
- **Per-IP rate limiting**: An in-memory token bucket limits each source IP (default 5 req/s sustained, burst 20). Configure with `--http-rate-per-sec` / `--http-rate-burst` or env vars `FERRUM_MCP_HTTP_RATE_PER_SEC` / `FERRUM_MCP_HTTP_RATE_BURST`. Excess requests receive HTTP 429 with `Retry-After: 1`.
- **No OAuth / auth middleware**: MCP HTTP transport uses Bearer header validation; it does not implement OAuth 2.1 or session auth.
- **No TLS termination**: The skeleton does not terminate TLS. Use a reverse proxy or secure tunnel for TLS.
- **Structured security logging**: Security events (rate limit, rejected Host/Origin/bearer) are emitted via `tracing::warn!`. The MCP server does not call the gateway audit API.

## Spec compliance and caveats

This is a **skeleton**, not a full Streamable HTTP implementation.

| Spec feature | Status | Notes |
|--------------|--------|-------|
| Single MCP endpoint (`/mcp`) | Partial | `POST` supported; `GET` returns 405 unless `Accept: text/event-stream`, which returns 406 |
| Synchronous JSON response (`POST`) | Implemented | Returns `application/json` |
| SSE streaming (`GET`) | Not provided | Returns 405; `Accept: text/event-stream` returns 406 |
| `MCP-Protocol-Version` header | Partial | Validated on `initialize` when present; must match `2024-11-05` |
| Host validation | Implemented | Default allow localhost/loopback; configurable allowlist |
| Origin validation | Implemented | Reject any Origin by default; configurable allowlist |
| Per-IP rate limiting | Implemented | Token bucket, default 5 req/s burst 20; returns 429 + `Retry-After` |
| Session management | Not provided | No session store, no `Mcp-Session-Id` |
| Resumability | Not provided | No event ID store, no replay buffer |
| DELETE session termination | Not provided | No session concept yet |
| Strict SEP-2243 headers | Not provided | Not enforced in skeleton |

## Running the HTTP transport

```bash
# Default stdio mode
./ferrum-mcp-server

# HTTP mode on default localhost:3000 (requires FERRUM_MCP_HTTP_BEARER_TOKEN)
./ferrum-mcp-server --transport http

# HTTP mode on custom bind
./ferrum-mcp-server --transport http --bind 127.0.0.1:8080

# Local development only: disable mandatory bearer auth
./ferrum-mcp-server --transport http --allow-insecure-no-auth
```

Example `POST /mcp` request:

```bash
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $FERRUM_MCP_HTTP_BEARER_TOKEN" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}'
```

## Related documents

- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — recommended deployment behind a tunnel
