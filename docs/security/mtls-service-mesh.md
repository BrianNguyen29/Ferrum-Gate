# mTLS Service-to-Service Design

> **Owner**: Security / Operator
> **Parent**: [`guides/README.md`](../guides/README.md)

---

## 1. Scope

This document is a **design-only artifact**. No native mTLS code has been added to `ferrumd`, `ferrum-mcp-server`, or any workspace crate. No new Rust dependencies (e.g., `rustls`, `native-tls`) have been introduced.

mTLS is treated as **optional transport-layer hardening**, not a replacement for bearer-token or scoped-token application-layer authentication. FerrumGate's execution-governance lifecycle (policy, capability, approval, rollback, provenance) remains independent of transport security.

---

## 2. Threat Model Mapping

mTLS benefits the following trust boundaries from [`docs/security/threat-model-stride.md`](./threat-model-stride.md):

| Boundary | Benefit | Rationale |
|----------|---------|---------|
| **B1** Human/Operator → Gateway | Yes | mTLS at edge proxy authenticates client certificates from operators/admin tools, adding transport-layer identity before bearer-token validation. |
| **B2** Agent/MCP Client → MCP Server | Yes | mTLS at tunnel edge authenticates the remote MCP client machine/agent identity before request reaches `ferrum-mcp-server`. |
| **B5** Gateway → Store | Yes | PostgreSQL native TLS/mTLS (operator-configured) encrypts and optionally authenticates the database connection. |
| **B3** MCP Server → Gateway | No | Local HTTP/REST via `FerrumGatewayClient` on localhost; same-trust-domain, same-host, no cross-host hop. |
| **B4** Gateway → PDP | No | Same-process PDP evaluation; no network hop. |
| **B6** Gateway → Provenance Ledger | No | Same-process write; no network hop. |
| **B7** Gateway → Adapters | No | Adapter calls are outbound from gateway to external systems; mTLS applies to those external systems, not FerrumGate internals. |

---

## 3. Architecture Decision / ADR

### Decision

**Select edge-level, operator-owned mTLS** for all inbound and store connectivity. **Reject native mTLS implementation inside `ferrumd` and `ferrum-mcp-server`.**

### Context

- FerrumGate currently runs as a single-node deployment: `ferrumd` and `ferrum-mcp-server` bind to `127.0.0.1` and are reached via reverse proxy or secure tunnel.
- Native mTLS in Rust would require adding TLS termination libraries, certificate reloading, revocation checking, and failure-mode handling to both binaries.
- The immediate deployment topology does not have service-to-service network hops that justify the added complexity.

### Options Considered

| Option | Verdict | Reason |
|--------|---------|--------|
| A. Native `rustls` in `ferrumd` / `ferrum-mcp-server` | **REJECTED** | Adds dependencies, config surface, cert lifecycle, and failure modes while the deployment topology remains single-node. |
| B. Native `native-tls` (OpenSSL/Schannel/SecureTransport) | **REJECTED** | Same as A; plus platform-specific behavior and linking complexity. |
| C. Edge-level operator-owned mTLS (Caddy, nginx, Cloudflare, Tailscale) | **ACCEPTED** | Leverages existing tunnel/proxy infrastructure; no code changes; operator controls CA and rotation. |
| D. PostgreSQL native TLS/mTLS | **ACCEPTED** as operator config | Documented separately; no FerrumGate code change required. |

### Reconsideration Condition

Native mTLS is relevant only for multi-node, cross-host deployment topologies (e.g., gateway and MCP server on separate hosts, or store on a separate network segment). Edge-level mTLS is sufficient for single-node deployments.

---

## 4. Edge Proxy mTLS Examples

### 4.1 Caddy (minimum example)

```caddy
{
    auto_https off
}

:443 {
    tls /etc/certs/server.crt /etc/certs/server.key {
        client_auth {
            mode require_and_verify
            trusted_ca_cert_file /etc/certs/ca.crt
        }
    }

    reverse_proxy 127.0.0.1:8080
}
```

- `mode require_and_verify` rejects requests without a valid client certificate signed by the configured CA.
- FerrumGate behind Caddy continues to enforce bearer/scoped-token auth; do not treat the client cert subject as a FerrumGate identity without a separate mapping design.

### 4.2 nginx (minimum example)

```nginx
server {
    listen 443 ssl;
    server_name ferrumgate.example;

    ssl_certificate     /etc/nginx/ssl/server.crt;
    ssl_certificate_key /etc/nginx/ssl/server.key;
    ssl_client_certificate /etc/nginx/ssl/ca.crt;
    ssl_verify_client on;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header X-SSL-Client-Subject $ssl_client_s_dn;
        proxy_set_header X-SSL-Client-Verify $ssl_client_verify;
    }
}
```

- `ssl_verify_client on` requires a valid client certificate.
- Headers are forwarded for logging/observability only; FerrumGate auth layer does not consume them as proof of identity.

### 4.3 Cloudflare Access mTLS Note

Cloudflare Access supports **mutual TLS authentication** at the edge:

- Upload a CA certificate to Cloudflare Access.
- Configure an mTLS policy on the hostname protecting FerrumGate.
- Only requests presenting a client certificate signed by that CA reach the origin tunnel.

Cloudflare handles certificate validation, revocation checking, and geo-blocking before traffic enters the tunnel.

### 4.4 Tailscale Mutual WireGuard Identity Note

Tailscale provides **machine-level identity** via WireGuard and Tailscale ACLs:

- Every node has a stable machine identity and key pair.
- Tailscale ACLs restrict which machines can reach the FerrumGate host.
- This is **not X.509 mTLS**, but achieves the same goal: mutual authentication of both ends of the connection.

If FerrumGate runs on a Tailscale node, the operator can restrict `ferrumd` port access to specific tag-based ACL rules without opening any public inbound ports.

---

## 5. PostgreSQL TLS/mTLS Guidance

When using PostgreSQL as the FerrumGate store, connection security is **operator-configured** via connection parameters and environment variables. No FerrumGate code changes are required.

### Recommended Parameters

| Parameter | Recommended Value | Purpose |
|-----------|-------------------|---------|
| `sslmode` | `verify-full` | Require TLS and verify server hostname against certificate. |
| `sslrootcert` | Path to CA bundle | Trust anchor for server certificate validation. |
| `sslcert` | Path to client certificate | Optional: enables mutual TLS authentication to PostgreSQL. |
| `sslkey` | Path to client private key | Required if `sslcert` is used. |

### Example Connection String

```
postgresql://user:pass@db.example/ferrumgate?sslmode=verify-full&sslrootcert=/etc/certs/ca.crt&sslcert=/etc/certs/client.crt&sslkey=/etc/certs/client.key
```

### Auth Options

- PostgreSQL can require `cert` authentication method (client cert subject mapped to database role).
- Alternatively, use `md5` or `scram-sha-256` **over** the TLS tunnel; the tunnel provides transport encryption and server identity, while database passwords provide application-layer auth.

This guidance is **operator-owned**: the operator manages CA distribution, certificate issuance, and rotation for the database tier.

---

## 6. Certificate Lifecycle Guidance

### 6.1 Certificate Authority (CA)

- Use a **dedicated internal CA** or a managed CA (e.g., HashiCorp Vault, AWS Private CA, Google Certificate Authority Service) for service certificates.
- Do not reuse a public Web PKI CA for internal mTLS; it complicates revocation and lacks machine identity semantics.

### 6.2 Issuance

- Issue certificates with **short lifetimes** (e.g., 30–90 days) to reduce the impact of compromise.
- Include clear Subject Alternative Names (SANs) for every service identity.
- Use automated issuance (ACME for internal CAs, Vault PKI engine, cert-manager) to avoid manual toil.

### 6.3 Rotation and Overlap

- Support **overlapping certificates**: a new certificate is deployed and trusted before the old one expires.
- Edge proxies (Caddy, nginx, Cloudflare) must reload or hot-swap certificates without dropping connections.
- FerrumGate itself does **not** reload certificates because it does not terminate TLS.

### 6.4 Revocation and Short-Lived Certs

- Prefer **short-lived certificates** over long-lived certificates with CRL/OCSP revocation.
- If using revocation, ensure edge proxies check CRL or OCSP stapling; document the latency trade-off.

### 6.5 Expiry Alerts

- Monitor certificate expiry (e.g., `openssl x509 -in cert.crt -noout -dates`, or Prometheus `ssl_certificate_expiry_seconds`).
- Alert at 30 days, 7 days, and 1 day before expiry.
- Define a rollback plan: if mTLS cert renewal fails, fall back to TLS-only (if acceptable) or stop serving traffic (fail closed).

---

## 7. Native mTLS Reference

If multi-node topology requires native mTLS, the following surface is anticipated:

| Concern | Detail |
|---------|-----------------|
| Dependencies | `rustls` + `rustls-pemfile` (or `native-tls` if platform integration is required). |
| Config Surface | `[tls]` TOML section: `cert_file`, `key_file`, `client_ca_file`, `client_auth_mode` (`optional` / `require` / `require_and_verify`). |
| Reload | File-watcher or SIGHUP-triggered certificate reload; or hot-reload via `notify` crate. |
| Failure Modes | Expired cert → fail closed (do not start listener). Missing client CA → fail closed. Mismatched SNI → TLS alert. |
| Scope | Would apply to `ferrumd` HTTP listener and optionally `ferrum-mcp-server` HTTP listener. Store connections already handled by PostgreSQL driver. |

**No native mTLS implementation is provided.**

---

## 8. Interaction with Existing Auth

mTLS operates at the **transport layer** (TLS). FerrumGate's existing auth operates at the **application layer**.

| Layer | Mechanism | Responsibility |
|-------|-----------|----------------|
| Transport | mTLS (edge proxy or tunnel) | Authenticates the network endpoint / machine identity. Encrypts data in transit. |
| Application | Bearer / scoped token | Authenticates the actor (operator, agent, service account) and authorizes actions (scopes, roles). |

### Rules

1. **Both layers are required** in a hardening scenario: mTLS does not eliminate the need for bearer-token auth inside FerrumGate.
2. **Do not use forwarded client certificate subject as application identity** unless a separate design maps X.509 Distinguished Names to FerrumGate `actor_id` / roles. That mapping is **not provided**.
3. If an edge proxy forwards `X-SSL-Client-*` headers, FerrumGate may log them for observability but must not treat them as authoritative auth signals.

---

## 9. Verification Strategy

| Step | Method | Owner |
|------|--------|-------|
| 1. Doc review | Review this document for completeness, consistency, and absence of overclaim. | Security |
| 2. Manual config validation (optional) | Operator deploys Caddy or nginx with mTLS in a test environment and confirms FerrumGate receives requests only from clients with valid certs. | Operator |
| 3. PostgreSQL TLS test (optional) | Connect `ferrumd` to a PostgreSQL instance with `sslmode=verify-full` and confirm connection succeeds. | Operator |

No `cargo` build or test execution is required because this is a documentation-only artifact.

---

## 10. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Operator misconfigures edge proxy mTLS (e.g., `verify_client optional` when `require_and_verify` intended). | Medium | High | Document minimum examples clearly; recommend `require_and_verify` as default. |
| Certificate expiry causes outage. | Medium | High | Expiry alerts at 30/7/1 days; automated renewal; overlapping certs. |
| Compromised CA leads to broad impersonation. | Low | Critical | Use dedicated internal CA; short-lived certs; rotate CA if compromise suspected. |
| mTLS perceived as replacing bearer auth, leading to auth bypass. | Medium | High | Document explicitly: mTLS is transport-layer; bearer/scoped token is still required. |
| Service-to-service encryption relies on edge-level mTLS. | Low | Medium | Configure edge proxy with client certificate verification. |

---

## 11. Go / No-Go Verdict

| Item | Verdict |
|------|---------|
| Design doc | **GO** — this document satisfies the design requirement. |
| Native mTLS implementation | **Not provided** — use edge-level mTLS for multi-node deployments. |
| Edge-level operator-owned mTLS | **GO** — recommended for deployed instances. |
| PostgreSQL TLS/mTLS | **GO** — operator-configured, no code changes. |

---

## 12. Related Docs

- [`docs/security/threat-model-stride.md`](./threat-model-stride.md) — Trust boundaries and STRIDE mapping.
- [`docs/guides/secure-mcp-tunnel-integration.md`](../guides/secure-mcp-tunnel-integration.md) — Tunnel topology and deployment examples.
- [`docs/mcp/private-deploy.md`](../mcp/private-deploy.md) — Private MCP deployment behind reverse proxy.
- [`docs/mcp/streamable-http-mcp.md`](../mcp/streamable-http-mcp.md) — HTTP transport skeleton and security notes.
- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes.

---

*End of mTLS design document.*
