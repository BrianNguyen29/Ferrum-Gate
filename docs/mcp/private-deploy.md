# Private MCP Deployment Guide

> **Parent**: [`guides/README.md`](../guides/README.md)

---

## 1. Scope

This guide documents how to deploy FerrumGate's MCP server privately behind a tunnel or reverse proxy so that remote MCP clients can reach it over HTTPS without exposing it to the public internet.

This guide is a **reference** for integrating FerrumGate's MCP server with external clients.

---

## 2. Topology

```
┌─────────────────┐     ┌──────────────────────────┐     ┌─────────────────┐
│  MCP Client     │────▶│  Private tunnel /        │────▶│  ferrum-mcp-    │
│  (remote agent, │     │  reverse proxy           │     │  server         │
│  ChatGPT, etc.) │     │  (TLS termination)       │     │  127.0.0.1:3000 │
└─────────────────┘     └──────────────────────────┘     └─────────────────┘
                                                                  │
                                                                  ▼
                                                         ┌─────────────────┐
                                                         │  FerrumGate     │
                                                         │  Gateway REST   │
                                                         └─────────────────┘
```

1. `ferrum-mcp-server` binds to a **local interface only** (`127.0.0.1:3000`).
2. A reverse proxy or secure tunnel (Caddy, Cloudflare Tunnel, Tailscale) terminates TLS and forwards to `localhost:3000`.
3. The MCP client connects to the **private HTTPS endpoint** provided by the tunnel.
4. The gateway REST API enforces bearer/scoped token auth and the full execution-governance lifecycle.

> **Rule**: The tunnel replaces the network path; it **does not replace** FerrumGate auth or policy.

---

## 3. CLI & binding

Run `ferrum-mcp-server` in HTTP mode:

```bash
# Default: stdio transport, bind 127.0.0.1:3000
./ferrum-mcp-server

# HTTP mode on default localhost:3000
./ferrum-mcp-server --transport http

# HTTP mode on custom local bind
./ferrum-mcp-server --transport http --bind 127.0.0.1:8080
```

**Always bind to a local interface** (`127.0.0.1` or a tailnet address). Do not bind to `0.0.0.0` unless a reverse proxy or firewall strictly limits access, and never bind to `0.0.0.0` without TLS.

See [`docs/mcp/streamable-http-mcp.md`](./streamable-http-mcp.md) for the full HTTP skeleton details.

---

## 4. Client configuration

Remote MCP clients that speak HTTP should point to the private HTTPS endpoint and include the FerrumGate token in the `Authorization` header:

```json
{
  "mcpServers": {
    "ferrumgate-private": {
      "url": "https://<private-host>/mcp",
      "headers": {
        "Authorization": "Bearer <FERRUMGATE_TOKEN>"
      }
    }
  }
}
```

> **Security warning**: Do not commit tokens to version control. Use environment variables or a secrets manager. Rotate tokens periodically.

See [`docs/guides/mcp-integration.md`](../guides/mcp-integration.md) for stdio client config and the token warning pattern.

---

## 5. Reverse proxy / tunnel setup

FerrumGate **does not provide a tunnel service**. Use one of the following operator-owned options.

### 5.1 Caddy (simple reverse proxy)

```caddy
ferrumgate.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

> **Note**: A real owned domain is recommended for deployed instances. Temporary domain (e.g., nip.io) may be used for rehearsal only.

### 5.2 Cloudflare Tunnel

Outbound-only; no open inbound firewall ports.

```yaml
tunnel: <TUNNEL_ID>
credentials-file: /etc/cloudflared/<TUNNEL_ID>.json

ingress:
  - hostname: ferrumgate.example.com
    service: http://localhost:3000
    originRequest:
      noTLSVerify: true   # ferrum-mcp-server does not terminate TLS
  - service: http_status:404
```

Enforce **Cloudflare Access** with JWT validation at the edge. FerrumGate still requires its own `Authorization` token on every request.

See [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) §4 for the full Cloudflare example.

### 5.3 Tailscale Funnel / Serve

**Tailscale Serve** (tailnet-only, recommended for private MCP):

```bash
tailscale serve --bg --https 443 localhost:3000
```

**Tailscale Funnel** (public internet, only if required):

```bash
tailscale funnel --bg --https 443 localhost:3000
```

Restrict Funnel usage via ACL node attributes. See [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) §5 for the full Tailscale example.

### 5.4 TLS at the edge

- Terminate TLS at the tunnel edge or reverse proxy.
- `ferrum-mcp-server` does not terminate TLS.
- Use valid certificates (Let's Encrypt, managed provider, or Tailscale's internal certs) for public or tailnet endpoints.

---

## 6. Auth / token handling

### Gateway token configuration

The underlying gateway (`ferrumd`) enforces auth, not the MCP server itself. Configure the gateway with one of the following:

```bash
# Environment variable
FERRUMD_AUTH_MODE=Bearer
FERRUMD_BEARER_TOKEN=<generate-with-openssl-rand-hex-32>
```

Or in `ferrumd.toml`:

```toml
[server]
auth_mode = "Bearer"
bearer_token = "<generate-with-openssl-rand-hex-32>"
```

### Scoped tokens

Where possible, use **scoped tokens** instead of the global bearer token:

| Scope | Use case |
|-------|----------|
| `intent:submit` | MCP client submitting intents |
| `proposal:evaluate` | Policy evaluation before capability mint |
| `capability:mint` | Capability generation |
| `execution:execute` | Running prepared actions |
| `provenance:read` | Query lineage and execution status |

See [`docs/guides/security-model.md`](../guides/security-model.md) for the full scope list and token lifecycle commands (`ferrumctl admin tokens`).

### Rotation and storage

1. Generate token on the target host (never print to logs):
   ```bash
   openssl rand -hex 32
   ```
2. Store token with `chmod 640` and restricted ownership.
3. Rotate after initial setup and periodically.
4. Verify old token returns 401 after rotation.
5. Do not print tokens in logs, command history, or MCP client configs committed to version control.

See [`docs/guides/operator.md`](../guides/operator.md) §"Token rotation" for the full procedure.

---

## 7. Security boundaries and checklist

### Trust boundary

This guide operates at **B2: Agent/MCP Client → FerrumGate MCP Server** per [`docs/security/threat-model-stride.md`](../security/threat-model-stride.md).

### Security checklist

| # | Check |
|---|-------|
| 1 | Bind `ferrum-mcp-server` to `127.0.0.1` (or a tailnet/local interface). |
| 2 | Require `Authorization` header on every request; tunnel is not auth. |
| 3 | Do not log `Authorization` or `Mcp-Session-Id` headers. |
| 4 | Terminate TLS at the tunnel edge or reverse proxy. |
| 5 | Do not expose an unauthenticated MCP endpoint to the public internet. |
| 6 | Use scoped tokens with minimal necessary scopes. |
| 7 | Rotate tokens periodically and record rotation in the audit log. |
| 8 | Validate JSON-RPC request structure at the gateway layer before execution. |
| 9 | Keep `ferrum-mcp-server` and `ferrumd` on the same host or trusted network; B3 is same-process/internal bridge today. |

See [`docs/security/secure-mcp-tunnel-review.md`](../security/secure-mcp-tunnel-review.md) for the full security review checklist.

---

## 8. Limitations

The following are explicitly **not** covered by this guide:

| Item | Reason |
|------|--------|
| SSE streaming (`GET /mcp`) | Returns 405 today; full SSE/multiplexing not provided |
| Session management / `Mcp-Session-Id` | No session store |
| Resumability with event ID tracking | No replay buffer |
| `MCP-Protocol-Version` header enforcement | Not enforced in skeleton |
| DELETE `/mcp` session termination | No session concept yet |
| OAuth / auth middleware specifically for MCP HTTP transport | Gateway bearer/scoped token auth is used instead |
| mTLS service-to-service | Transport hardening; tunnel integration covers baseline |

---

## 9. Notes

| Note | Value |
|------|-------|
| Certified compatible with any external MCP client | **NO** |
| SSE / session / resumability support | **Not provided** |
| OAuth / mTLS for MCP transport | **Not provided** |
| `MCP-Protocol-Version` header enforcement | **Not provided** |

---

## 10. Related docs

- [`docs/mcp/streamable-http-mcp.md`](./streamable-http-mcp.md) — HTTP skeleton, routes, CLI.
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — Reverse proxy/tunnel patterns, Caddy/Cloudflare/Tailscale examples, tunnel != auth.
- [`docs/guides/mcp-integration.md`](../guides/mcp-integration.md) — MCP client config and token warning pattern.
- [`docs/security/threat-model-stride.md`](../security/threat-model-stride.md) — MCP Client → MCP Server trust boundary (B2).
- [`docs/guides/security-model.md`](../guides/security-model.md) — Auth/token config and scope list.
- [`docs/guides/operator.md`](../guides/operator.md) — Token rotation, deployment checklist, and incident response.
- [`docs/guides/hosted-deployment.md`](../guides/hosted-deployment.md) — systemd, Docker, and deployment modes.
- [`docs/security/secure-mcp-tunnel-review.md`](../security/secure-mcp-tunnel-review.md) — Tunnel security checklist.

---

*End of private MCP deployment guide.*
