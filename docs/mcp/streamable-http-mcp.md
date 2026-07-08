# Streamable HTTP MCP Transport — Skeleton

## What is implemented

This provides a bounded Streamable HTTP transport skeleton for `ferrum-mcp-server`.

- CLI args `--transport stdio|http` and `--bind ADDR`
  - Default: `stdio` transport, bind `127.0.0.1:3000`
- `GET /health` — basic JSON health probe
- `GET /ready` — shallow readiness with tool count and transport metadata
- `POST /mcp` — accepts a single JSON-RPC request body, dispatches through the existing `dispatch_with_client()`, and returns a synchronous JSON-RPC response as `application/json`
  - `initialize` returns a `Mcp-Session-Id` header on success
  - All other methods require a valid `Mcp-Session-Id` header bound to the same bearer token fingerprint
- `GET /mcp` — SSE redelivery-only replay endpoint; requires `Accept: text/event-stream`, a valid `Mcp-Session-Id`, and bearer auth
  - Replays retained outbound events after the optional `Last-Event-ID`
  - Never parses the request body or dispatches tool calls
- `DELETE /mcp` — terminate the session idempotently; requires bearer auth and `Mcp-Session-Id`

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

Outbound JSON-RPC responses are serialized and appended to an in-memory per-session replay buffer after `POST /mcp` dispatch completes. `GET /mcp` replays those retained events only. The replay buffer is bounded by event count, per-event payload bytes, and total bytes per session; if append fails after dispatch (e.g., the response exceeds the per-event byte limit), the completed dispatch response is still returned with a logged warning, and the client is never presented with a misleading 500.

## Security notes

- **Bind address default**: `127.0.0.1:3000` (localhost only). Non-loopback binds are refused unless `--allow-insecure-nonlocal-bind` is passed.
- **Bearer auth mandatory by default**: `POST /mcp`, `GET /mcp`, and `DELETE /mcp` require a valid `Authorization: Bearer <token>` header. The token is read from `FERRUM_MCP_HTTP_BEARER_TOKEN` (falling back to `FERRUM_GATEWAY_BEARER_TOKEN`). Use `--allow-insecure-no-auth` only for local development.
- **Host validation**: The `Host` header is checked against an allowlist. Defaults allow `localhost`, `127.0.0.1`, and `[::1]`; customize with `--allowed-host` or `FERRUM_MCP_ALLOWED_HOSTS` (comma-separated).
- **Origin validation**: Any `Origin` header is rejected by default. Allow specific origins with `--allowed-origin` or `FERRUM_MCP_ALLOWED_ORIGINS` (comma-separated).
- **Per-IP rate limiting**: An in-memory token bucket limits each source IP (default 5 req/s sustained, burst 20). Configure with `--http-rate-per-sec` / `--http-rate-burst` or env vars `FERRUM_MCP_HTTP_RATE_PER_SEC` / `FERRUM_MCP_HTTP_RATE_BURST`. Excess requests receive HTTP 429 with `Retry-After: 1`.
- **Session auth binding**: Sessions are bound to a SHA-256 fingerprint of the bearer token. The raw token is never stored in the session. A session ID alone is not sufficient for auth.
- **No OAuth / auth middleware**: MCP HTTP transport uses Bearer header validation; it does not implement OAuth 2.1 or session cookies.
- **No TLS termination**: The skeleton does not terminate TLS. Use a reverse proxy or secure tunnel for TLS.
- **Structured security logging**: Security events (rate limit, rejected Host/Origin/bearer, protocol version mismatch) are emitted via `tracing::warn!`. Session IDs are not logged. The MCP server does not call the gateway audit API.

## Session configuration

- `--mcp-session-ttl-secs` / `FERRUM_MCP_SESSION_TTL_SECS` — session TTL (default 300)
- `--mcp-session-max-events` / `FERRUM_MCP_SESSION_MAX_EVENTS` — max retained events per session (default 1000)
- `--mcp-session-max-sessions` / `FERRUM_MCP_SESSION_MAX_SESSIONS` — max concurrent in-memory sessions (default 1000)
- `--mcp-session-max-event-bytes` / `FERRUM_MCP_SESSION_MAX_EVENT_BYTES` — max bytes for a single replay event (default 1 MiB)
- `--mcp-session-max-total-bytes` / `FERRUM_MCP_SESSION_MAX_TOTAL_BYTES` — max total replay bytes retained per session (default 16 MiB)

Expired sessions are cleaned up periodically. Sessions are **in-memory only** and are lost on restart. Oldest events are evicted when a session exceeds its event count or total byte limit; an event larger than the per-event limit is not retained. If replay append fails after a completed dispatch, the dispatch response is still returned and a warning is logged. Replay loss after dispatch is never converted into a 500 response.

## Spec compliance and caveats

This is a **skeleton**, not a full Streamable HTTP implementation.

| Spec feature | Status | Notes |
|--------------|--------|-------|
| Single MCP endpoint (`/mcp`) | Partial | `POST`, `GET`, and `DELETE` supported |
| Synchronous JSON response (`POST`) | Implemented | Returns `application/json`; `initialize` returns `Mcp-Session-Id` |
| SSE streaming (`GET`) | Implemented | Redelivery-only replay; no request body parsing or dispatch |
| `MCP-Protocol-Version` header | Partial | Validated on `initialize` when present; must match `2024-11-05` |
| Host validation | Implemented | Default allow localhost/loopback; configurable allowlist |
| Origin validation | Implemented | Reject any Origin by default; configurable allowlist |
| Per-IP rate limiting | Implemented | Token bucket, default 5 req/s burst 20; returns 429 + `Retry-After` |
| Session management | Implemented | In-memory `Mcp-Session-Id`; bound to token fingerprint; TTL and bounds |
| Resumability | Skeleton | In-memory replay buffer; **not restart-resumable** |
| DELETE session termination | Implemented | Idempotent `204 No Content` |
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

# Tighten session bounds
./ferrum-mcp-server --transport http --mcp-session-ttl-secs 60 --mcp-session-max-events 100 --mcp-session-max-event-bytes 65536 --mcp-session-max-total-bytes 1048576
```

Example `POST /mcp` request:

```bash
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $FERRUM_MCP_HTTP_BEARER_TOKEN" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}'
# Response includes Mcp-Session-Id header
```

Example `GET /mcp` SSE replay:

```bash
curl -N http://127.0.0.1:3000/mcp \
  -H "Accept: text/event-stream" \
  -H "Authorization: Bearer $FERRUM_MCP_HTTP_BEARER_TOKEN" \
  -H "Mcp-Session-Id: $SESSION_ID"
```

Example `DELETE /mcp`:

```bash
curl -X DELETE http://127.0.0.1:3000/mcp \
  -H "Authorization: Bearer $FERRUM_MCP_HTTP_BEARER_TOKEN" \
  -H "Mcp-Session-Id: $SESSION_ID"
```

## Related documents

- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — recommended deployment behind a tunnel
