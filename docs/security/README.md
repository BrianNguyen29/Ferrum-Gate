# Security Documentation

Security design documents, threat models, and control mappings for FerrumGate.

---

## Index

| Document | Summary |
|----------|---------|
| [Scoped Tokens and RBAC](scoped-tokens-rbac.md) | Least-privilege token model, roles, route scope enforcement, and lifecycle CLI examples. |
| [Secure MCP Tunnel Review](secure-mcp-tunnel-review.md) | Operator checklist for deploying FerrumGate behind secure tunnels (Cloudflare, Tailscale, TLS reverse proxy). |
| [Agent Identity with Ed25519](agent-identity-ed25519.md) | Cryptographic agent identity using Ed25519 signatures, nonce/timestamp replay protection, and registry lifecycle. |
| [STRIDE Threat Model](threat-model-stride.md) | Trust boundaries and STRIDE threat mapping across gateway, MCP server, store, provenance ledger, and adapters. |
| [mTLS Service-to-Service Design](mtls-service-mesh.md) | Edge-level mTLS recommendation for reverse proxies and PostgreSQL TLS; native mTLS is design-only. |
| [OWASP Agentic AI Mapping](owasp-agentic-ai-mapping.md) | Mapping of OWASP LLM Top 10 v2.0 categories to FerrumGate controls, gaps, and coverage. |
| [OIDC/JWT Federation Design](oidc-jwt-federation.md) | Stateless JWT validation, claim-to-role mapping, JWKS caching, and `AuthMode::Oidc` design. |

## Related documents

- Security model and operational hardening: [`docs/guides/security-model.md`](../guides/security-model.md)
- Threat model and trust boundaries: [`threat-model-stride.md`](./threat-model-stride.md)
