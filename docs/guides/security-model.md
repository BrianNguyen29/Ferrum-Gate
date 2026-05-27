# Security Model Guide

> **Status**: Scoped token/RBAC/SEC-6 implementation signed for current T1 scope; not full production security.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Current security posture

- **Auth mode**: `Disabled` (dev), `Bearer` (pilot), or scoped-token mode when explicitly enabled.
- **Bearer token**: Single global token remains supported for conditional pilot compatibility.
- **Scoped tokens/RBAC**: Implemented for the current T1 scope with deny-by-default middleware and admin token lifecycle APIs/CLI.
- **No multi-tenancy**: One deployment, one implicit tenant.
- **Audit log**: SEC-6 minimal append-only audit log implemented for admin/policy/approval/token actions; not compliance-grade WORM/signed storage.

## Threat model (summary)

| Threat | Current mitigation | Future mitigation |
|--------|--------------------|--------------------|
| Token theft | Token stored on VM only; rotation procedure exists; scoped tokens support expiry + revocation | OIDC/SSO and stronger operational policy |
| Insider abuse | Operator trust plus RBAC + minimal audit log | Compliance-grade audit logging |
| Tenant data leak | N/A (single tenant) | tenant_id filtering + PG RLS |
| Auth bypass | Constant-time token comparison plus scoped-token RBAC middleware | External identity integration |
| Secret leak in logs | Output redaction in MCP; minimal audit log avoids secret material | Structured compliance-grade audit controls |

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

### Roles

| Role | Permissions |
|------|-------------|
| admin | Full system control |
| operator | Approve/reject, run drills, view health |
| policy_author | Create/update/simulate policies |
| auditor | Read-only lineage/executions/provenance |
| agent | Submit intent / use MCP within scope |
| read_only | Health and status only |

### Token scopes

- `intent:submit`
- `proposal:evaluate`
- `capability:mint`
- `execution:authorize`, `prepare`, `execute`, `verify`, `compensate`
- `approval:resolve`
- `policy:read`, `policy:write`
- `provenance:read`
- `admin:tokens`, `admin:config`
- `backup:run`

### Acceptance targets

- [x] Read-only token cannot call mutating endpoints.
- [x] Agent token cannot approve proposals.
- [x] Auditor token cannot execute actions.
- [x] Revoked token returns 401.
- [x] Expired token returns 401.
- [x] Audit log records actor, action, and result for current scope.
- [ ] Tenant A cannot read tenant B data — deferred by single-tenant T1 decision; no multi-tenant claim.

## Hardening checklist

### Immediate (pilot)

- [ ] Use `Bearer` auth mode in production pilot config.
- [ ] Generate token with `openssl rand -hex 32`.
- [ ] Store token with `chmod 640` and restricted ownership.
- [ ] Rotate token after initial setup and periodically.
- [ ] Deploy behind TLS-terminating reverse proxy.
- [ ] Do not print tokens in logs or command history.

### Near-term (post-pilot)

- [x] Implement scoped tokens with metadata in store.
- [x] Implement RBAC middleware (endpoint → required scope mapping).
- [x] Add minimal audit log separate from provenance.
- [x] Design tenant model ADR for T1 single-tenant scope.

### Long-term

- [ ] Implement tenant_id in schema and store filters.
- [ ] Add PostgreSQL RLS as defense-in-depth.
- [ ] Evaluate OIDC/JWT/SSO integration.
- [ ] Evaluate mTLS for service-to-service auth.

## Status caveat

> **production-ready = NO**. Scoped auth/RBAC/SEC-6 are implemented and signed for the current T1 scope, but this does not complete full G2, Block A, multi-tenant security, OIDC/SSO, or compliance-grade audit logging. See [`docs/production-readiness-v2/04-security-tenant-model-adr.md`](../../production-readiness-v2/04-security-tenant-model-adr.md) and [`docs/implementation-path/artifacts/2026-05-27-phase4-security-operator-signoff.md`](../../implementation-path/artifacts/2026-05-27-phase4-security-operator-signoff.md).

## Related docs

- [`operator.md`](./operator.md) — Token rotation and incident response.
- [`docs/production-readiness-v2/04-security-tenant-model-adr.md`](../../production-readiness-v2/04-security-tenant-model-adr.md) — Full ADR.
- [`docs/implementation-path/70-security-hardening-local-only-plan.md`](../../implementation-path/70-security-hardening-local-only-plan.md) — Local security audit commands.
