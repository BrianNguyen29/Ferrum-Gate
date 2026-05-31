# Secure MCP Tunnel Integration Guide

> **Parent**: [`guides/README.md`](../guides/README.md)

---

## 1. Principles

FerrumGate **does not provide a tunnel service**. Instead, it is designed to run **behind** existing secure transport layers:

- **Cloudflare Tunnel** — outbound-only, public-hostname or private-network.
- **Tailscale Funnel / Serve** — tailnet-only or public internet via WireGuard.
- **Generic HTTPS reverse proxy / tunnel** — any TLS-terminating edge that forwards to FerrumGate.

What the tunnel provides:

- Encrypted transport between the MCP client and the FerrumGate host.
- No open inbound firewall ports (when using outbound-only tunnels).
- Optional identity gating at the edge (e.g., Cloudflare Access JWT, Tailscale ACL).

What FerrumGate still provides:

- **Policy evaluation**, **capability minting**, **approval gating**, **rollback classification**, and **provenance tracking**.
- **Bearer / scoped token authentication** on every request.
- **Execution governance** independent of transport.

> **Rule**: A tunnel replaces the network path; it **does not replace** FerrumGate auth or policy.

---

## 2. Topology

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  MCP Client     │────▶│  Secure Tunnel  │────▶│  FerrumGate     │
│ (ChatGPT/       │     │ (Cloudflare /   │     │  MCP Server /   │
│  Claude /       │     │  Tailscale /    │     │  Gateway        │
│  OpenAI remote) │     │  Generic TLS)   │     │  localhost:8080 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

1. The MCP client connects to the **tunnel edge** (public hostname or tailnet address).
2. The tunnel forwards the request to FerrumGate bound to a **local interface** (e.g., `127.0.0.1:8080`).
3. FerrumGate validates the `Authorization` header and evaluates the request through its normal governance lifecycle.

---

## 3. OpenAI / Generic Remote MCP Example

> **Caveat**: No dedicated "OpenAI Secure MCP Tunnel" product documentation was found during research. OpenAI's documentation describes remote MCP servers over HTTPS/SSE with OAuth. The example below is therefore a **generic MCP-over-HTTPS remote server pattern** that works with any compatible client, including OpenAI's remote MCP server model, and is not specific to an OpenAI tunnel product.

### 3.1 Scenario

You want an external MCP client (e.g., ChatGPT, a hosted agent platform, or a remote Claude Desktop instance) to reach FerrumGate over the internet without exposing FerrumGate directly.

### 3.2 Requirements

- A public HTTPS endpoint with a valid TLS certificate (Let's Encrypt, managed provider, or Tailscale).
- FerrumGate bound to `127.0.0.1:8080` so it is not reachable directly from the network.
- A reverse proxy (Caddy, nginx, or the tunnel itself) that terminates TLS and forwards to `localhost:8080`.

### 3.3 Caddy reverse proxy example

```caddy
ferrumgate.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

> **Note**: A real owned domain is recommended for deployed instances. Temporary domain (e.g., nip.io) may be used for rehearsal only.

### 3.4 Client configuration

Remote MCP clients that speak HTTP/SSE should point to the public HTTPS endpoint and include the FerrumGate token in the `Authorization` header:

```json
{
  "mcpServers": {
    "ferrumgate-remote": {
      "url": "https://ferrumgate.example.com/mcp",
      "headers": {
        "Authorization": "Bearer <FERRUMGATE_TOKEN>"
      }
    }
  }
}
```

> **Security warning**: Do not commit tokens to version control. Rotate tokens periodically.

### 3.5 stdio-to-HTTP bridge note

FerrumGate's MCP server currently uses **stdio** transport. To expose it over HTTPS you may need an HTTP-to-stdio bridge (e.g., a small reverse gateway) or wait for **Streamable HTTP MCP** support. This guide documents the tunnel layer only; the MCP transport upgrade is tracked separately.

---

## 4. Cloudflare Tunnel Example

### 4.1 Why Cloudflare Tunnel

- **Outbound-only**: The `cloudflared` daemon initiates an outbound connection to Cloudflare; no inbound firewall rules are required.
- **Public hostname or private network**: You can expose a public DNS record or keep the tunnel private and require Cloudflare Access.
- **Edge security**: Cloudflare Access can enforce JWT-based identity before the request ever reaches FerrumGate.

### 4.2 Installation (operator-owned)

Install `cloudflared` on the FerrumGate host following [Cloudflare's official documentation](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/). FerrumGate does not distribute or manage `cloudflared`.

### 4.3 Tunnel configuration example

Create or edit `~/.cloudflared/config.yml`:

```yaml
tunnel: <TUNNEL_ID>
credentials-file: /etc/cloudflared/<TUNNEL_ID>.json

ingress:
  - hostname: ferrumgate.example.com
    service: http://localhost:8080
    originRequest:
      noTLSVerify: true   # FerrumGate does not terminate TLS
  - service: http_status:404
```

Run the tunnel:

```bash
cloudflared tunnel --no-autoupdate run --token <TUNNEL_TOKEN>
```

Or use a systemd service as described in Cloudflare docs.

### 4.4 Cloudflare Access (recommended)

Add an **Access application** for `ferrumgate.example.com`:

- Require **JWT validation** at the edge.
- Restrict by email domain or identity provider.
- Pass validated headers to FerrumGate if needed (FerrumGate still requires its own `Authorization` token).

> **Note**: FerrumGate does not validate Cloudflare Access JWTs natively. The edge must enforce identity; FerrumGate enforces its own bearer/scoped token on every request.

### 4.5 FerrumGate binding

Ensure `ferrumd` binds to the loopback interface only:

```bash
# In ferrumd.toml or env
bind_addr = "127.0.0.1:8080"
```

Or via environment variable:

```bash
FERRUMD_BIND_ADDR=127.0.0.1:8080
```

---

## 5. Tailscale Funnel / Serve Example

### 5.1 Why Tailscale

- **Funnel**: Exposes a service on the public internet over a WireGuard tunnel (ports 443, 8443, 10000).
- **Serve**: Exposes a service to your tailnet only (no public internet).
- **No open firewall ports**: Traffic arrives over the Tailscale mesh.

### 5.2 Installation (operator-owned)

Install Tailscale on the FerrumGate host and authenticate following [Tailscale's documentation](https://tailscale.com/kb/). FerrumGate does not distribute or manage Tailscale.

### 5.3 Tailscale Funnel (public internet)

Expose FerrumGate to the internet:

```bash
tailscale funnel --bg --https 443 localhost:8080
```

This creates a public HTTPS endpoint such as `https://node-name.tailnet-name.ts.net`.

### 5.4 Tailscale Serve (tailnet-only)

Expose FerrumGate only inside your tailnet:

```bash
tailscale serve --bg --https 443 localhost:8080
```

Clients must be on the tailnet to reach the service.

### 5.5 ACL restriction

Restrict which nodes can use Funnel via Tailscale ACL node attributes:

```json
{
  "nodeAttrs": [
    {
      "target": ["tag:ferrumgate"],
      "attr":   ["funnel"]
    }
  ]
}
```

Only nodes tagged `tag:ferrumgate` may open a Funnel.

> **Note**: FerrumGate does not manage Tailscale ACLs. The operator must configure Tailscale policy independently.

### 5.6 FerrumGate binding

Same as Cloudflare: bind to `127.0.0.1:8080` so FerrumGate is not reachable except through Tailscale.

---

## 6. Security checklist summary

See [`docs/security/secure-mcp-tunnel-review.md`](../security/secure-mcp-tunnel-review.md) for the full security review checklist.

Key points:

| # | Check |
|---|-------|
| 1 | Bind FerrumGate to `127.0.0.1` (or a local interface). |
| 2 | Require `Authorization` header on every request; tunnel is not auth. |
| 3 | Do not log `Authorization` or `Mcp-Session-Id` headers. |
| 4 | Terminate TLS at the tunnel edge or reverse proxy. |
| 5 | If using Cloudflare, enforce Access JWT at the edge. |
| 6 | If using Tailscale Funnel, restrict ACL node attributes. |
| 7 | Never pass through or inject FerrumGate bearer tokens at the tunnel layer. |

---

## 7. Notes

| Note | Value |
|------|-------|
| **FerrumGate provides a tunnel service** | **NO** — integration guide only |
| **Cloudflare / Tailscale / OpenAI configuration validated by FerrumGate** | **NO** — operator-owned; examples are illustrative |

## 8. Related docs

- [`docs/security/secure-mcp-tunnel-review.md`](../security/secure-mcp-tunnel-review.md) — Security review checklist.
- [`docs/guides/mcp-integration.md`](./mcp-integration.md) — Local MCP setup and stdio transport.
- [`docs/security/threat-model-stride.md`](../security/threat-model-stride.md) — Trust boundaries and STRIDE mapping.
- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes.
