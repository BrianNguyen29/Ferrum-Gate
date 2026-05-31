# Secure MCP Tunnel Security Review

> **Parent**: [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md)

---

## 1. Scope

This document is a security review checklist for operators who deploy FerrumGate behind a secure tunnel (Cloudflare Tunnel, Tailscale Funnel/Serve, or generic TLS reverse proxy). It does not replace a formal penetration test or security audit.

---

## 2. Checklist

### 2.1 Network binding

- [ ] **Bind FerrumGate to a local interface** (`127.0.0.1` or a tailnet-only address) so that it is not directly reachable from the public internet or untrusted network.
- [ ] Verify with `ss -tlnp | grep ferrumd` that no listener is on `0.0.0.0`.

### 2.2 Authentication

- [ ] **Authorization header is required on every request**. The tunnel provides transport encryption; it does not authenticate the MCP client to FerrumGate.
- [ ] **Do not disable auth** (`auth_mode = "Disabled"`) when running behind a tunnel in any non-local environment.
- [ ] Use scoped tokens with minimal necessary scopes instead of the global bearer token where possible.

### 2.3 Logging and secrets hygiene

- [ ] **Do not log the `Authorization` header** in reverse proxies, tunnel daemons, or FerrumGate logs.
- [ ] **Do not log `Mcp-Session-Id`** or other session identifiers at `INFO` or higher levels.
- [ ] Redact token material in any debug or trace output.
- [ ] Rotate tokens after initial setup and periodically thereafter.

### 2.4 TLS

- [ ] **TLS must be terminated at the public endpoint** (tunnel edge, reverse proxy, or ingress). Unencrypted HTTP must not be exposed to untrusted networks.
- [ ] Use valid certificates (not self-signed) for public internet endpoints.
- [ ] For tailnet-only (Tailscale Serve), Tailscale's internal WireGuard encryption satisfies the transport security requirement; no additional TLS is required inside the tailnet, though terminating TLS at the edge is still recommended.

### 2.5 Cloudflare Tunnel specifics

- [ ] Enforce **Cloudflare Access** with JWT validation at the edge.
- [ ] Restrict Access policies to approved email domains or identity providers.
- [ ] Do not rely on the tunnel alone; FerrumGate bearer/scoped token auth must still be enforced.
- [ ] Review `originRequest` settings; `noTLSVerify: true` is acceptable only because FerrumGate does not terminate TLS.

### 2.6 Tailscale specifics

- [ ] Restrict Funnel usage via **ACL node attributes** (`"funnel"` attr).
- [ ] Prefer **Tailscale Serve** (tailnet-only) over Funnel (public internet) unless public access is explicitly required.
- [ ] Tag FerrumGate nodes with `tag:ferrumgate` and enforce least-privilege ACLs.

### 2.7 Token passthrough

- [ ] **No token passthrough**: The tunnel or reverse proxy must not inject, append, or pass through FerrumGate bearer tokens on behalf of clients.
- [ ] Each MCP client must present its own `Authorization` header.

### 2.8 Origin validation

- [ ] Validate the `Origin` header on HTTP/SSE MCP requests where feasible.
- [ ] Reject requests with unexpected `Origin` values at the reverse proxy or application layer.

### 2.9 Policy and capability invariants

- [ ] Tunnel integration does not relax FerrumGate policy evaluation.
- [ ] Capabilities remain **single-use** and **TTL-bound** (max 300 s).
- [ ] Rollback-by-default and provenance tracking remain active.

---

## 3. Notes

| Note | Value |
|------|-------|
| **Formal security audit** | **NO** — this is a checklist, not a penetration test |
| **Tunnel service provided by FerrumGate** | **NO** |
| **Cloudflare / Tailscale / OpenAI liability** | **NO** — operator-owned configuration |

## 4. Related docs

- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — Deployment examples and topology.
- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — STRIDE trust boundaries and controls.
