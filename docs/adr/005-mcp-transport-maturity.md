# ADR 005 — MCP Transport Maturity Boundary

## Status
Accepted

## Context

The MCP server binary (`ferrum-mcp-server`) supports two transports:
- **Stdio** — line-based JSON-RPC over stdin/stdout (stable, default)
- **HTTP** — Streamable HTTP skeleton with POST/GET endpoints (experimental)

The HTTP transport has incomplete SSE support, no session management, and no OAuth/auth integration. Enabling it by default would expose unhardened surface area.

## Decision

1. **Stdio is the default and stable transport**. It is always compiled and requires no extra dependencies.
2. **HTTP is experimental and feature-gated** behind the `http` feature in `ferrum-integrations-mcp`. Without this feature:
   - The `Transport::Http` enum variant is still parsed by clap but the binary exits with a clear error at runtime.
   - HTTP handlers, `run_http`, and HTTP tests are not compiled.
   - `axum` and `tower` are optional dependencies.
3. **Before HTTP can become default**, a secure-code-review checklist must be completed:
   - SSE streaming/resumability implemented
   - Session state management
   - OAuth or bearer auth integration for MCP HTTP endpoints
   - Rate limiting per HTTP connection
   - Audit logging for all HTTP requests
   - Penetration test against the HTTP surface
4. **HTTP hardening (P1-2)** is implemented behind the `http` feature but does not promote HTTP to stable:
   - Mandatory bearer auth by default with explicit `--allow-insecure-no-auth` opt-out
   - Host validation (localhost/loopback defaults, configurable via `--allowed-host` / `FERRUM_MCP_ALLOWED_HOSTS`)
   - Origin validation (reject any Origin by default, allowlist via `--allowed-origin` / `FERRUM_MCP_ALLOWED_ORIGINS`)
   - Per-IP HTTP-layer token-bucket rate limiting (default 5 req/s, burst 20)
   - Structured security logging via `tracing` (no gateway audit API integration)
   - `Accept: text/event-stream` rejected with 406 (SSE not implemented)
   - `MCP-Protocol-Version` header validated on `initialize` when present
   - Non-loopback bind blocked unless `--allow-insecure-nonlocal-bind` is set
5. **Documentation** marks HTTP as experimental and directs users to stdio for production use.

## Consequences

- **Positive**: Default build is minimal and safe; HTTP surface is opt-in.
- **Positive**: Clear boundary between stable (stdio) and experimental (HTTP) transports.
- **Negative**: Users wanting HTTP must build with `--features http` and accept experimental status.
- **Negative**: Dual maintenance of stdio and HTTP paths until HTTP is promoted or removed.
