# Security Model Guide

> **Status**: Scaffold. Security model is design-only; not yet implemented.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Current security posture

- **Auth mode**: `Disabled` (dev) or `Bearer` (pilot).
- **Bearer token**: Single global token with full power.
- **No RBAC**: Anyone with the token can do anything.
- **No multi-tenancy**: One deployment, one implicit tenant.
- **No audit log separate from provenance**: Provenance exists but is not a security audit log.

## Threat model (summary)

| Threat | Current mitigation | Future mitigation |
|--------|--------------------|--------------------|
| Token theft | Token stored on VM only; rotation procedure exists | Scoped tokens with expiry + revocation |
| Insider abuse | Operator trust | RBAC + audit log |
| Tenant data leak | N/A (single tenant) | tenant_id filtering + PG RLS |
| Auth bypass | Constant-time token comparison | RBAC middleware + deny-by-default |
| Secret leak in logs | Output redaction in MCP | Structured audit log with redaction |

## Target security model

### Identity hierarchy

```
Tenant
  └── Workspace/Project
        └── Actor
              ├── Human Operator
              ├── Auditor
              ├── Agent
              └── Service Account
```

### Roles (planned)

| Role | Permissions |
|------|-------------|
| admin | Full system control |
| operator | Approve/reject, run drills, view health |
| policy_author | Create/update/simulate policies |
| auditor | Read-only lineage/executions/provenance |
| agent | Submit intent / use MCP within scope |
| read_only | Health and status only |

### Token scopes (planned)

- `intent:submit`
- `proposal:evaluate`
- `capability:mint`
- `execution:authorize`, `prepare`, `execute`, `verify`, `compensate`
- `approval:resolve`
- `policy:read`, `policy:write`
- `provenance:read`
- `admin:tokens`, `admin:config`
- `backup:run`

### Acceptance targets (planned)

- Read-only token cannot call mutating endpoints.
- Agent token cannot approve proposals.
- Auditor token cannot execute actions.
- Tenant A cannot read tenant B data.
- Revoked token returns 401.
- Expired token returns 401.
- Audit log records actor, action, and result.

## Hardening checklist

### Immediate (pilot)

- [ ] Use `Bearer` auth mode in production pilot config.
- [ ] Generate token with `openssl rand -hex 32`.
- [ ] Store token with `chmod 640` and restricted ownership.
- [ ] Rotate token after initial setup and periodically.
- [ ] Deploy behind TLS-terminating reverse proxy.
- [ ] Do not print tokens in logs or command history.

### Near-term (post-pilot)

- [ ] Implement scoped tokens with metadata in store.
- [ ] Implement RBAC middleware (endpoint → required scope mapping).
- [ ] Add audit log separate from provenance.
- [ ] Design tenant model ADR.

### Long-term

- [ ] Implement tenant_id in schema and store filters.
- [ ] Add PostgreSQL RLS as defense-in-depth.
- [ ] Evaluate OIDC/JWT/SSO integration.
- [ ] Evaluate mTLS for service-to-service auth.

## Status caveat

> **production-ready = NO**. The current auth model (single global bearer token) is acceptable for conditional pilot only. Scoped auth/RBAC is planned but not implemented. See [`docs/production-readiness-v2/04-security-tenant-model-adr.md`](../../production-readiness-v2/04-security-tenant-model-adr.md).

## Related docs

- [`operator.md`](./operator.md) — Token rotation and incident response.
- [`docs/production-readiness-v2/04-security-tenant-model-adr.md`](../../production-readiness-v2/04-security-tenant-model-adr.md) — Full ADR.
- [`docs/implementation-path/70-security-hardening-local-only-plan.md`](../../implementation-path/70-security-hardening-local-only-plan.md) — Local security audit commands.
