# Security Model Guide

> **Parent**: [`guides/README.md`](./README.md)

---

## Current security controls

- **Auth mode**: `Disabled` (dev), `Bearer` (pilot), `Scoped`, `OIDC`, and `Agent` (all implemented).
- **Bearer token**: Single global token remains supported for pilot compatibility.
- **Scoped tokens/RBAC**: Implemented with deny-by-default middleware and admin token lifecycle APIs/CLI.
- **No multi-tenancy**: One deployment, one implicit tenant.
- **Audit log**: Minimal append-only audit log implemented for admin/policy/approval/token actions; not compliance-grade WORM/signed storage.
- **Token TTL enforcement**: Server-side rejection of tokens with expiry > 90 days.

## Threat model (summary)

| Threat | Current mitigation | Additional mitigation |
|--------|--------------------|-----------------------|
| Token theft | Token stored on VM only; rotation procedure exists; scoped tokens support expiry + revocation | OIDC/SSO and stronger operational policy |
| Insider abuse | Operator trust plus RBAC + minimal audit log | Compliance-grade audit logging |
| Tenant data leak | N/A (single tenant) | tenant_id filtering + PG RLS |
| Auth bypass | Constant-time token comparison plus scoped-token RBAC middleware | External identity integration |
| Secret leak in logs | Output redaction in MCP; minimal audit log avoids secret material | Structured compliance-grade audit controls |

## Scoped token and RBAC implementation

### What is implemented

| Feature | Evidence |
|---------|----------|
| Scoped token store (SQLite + PostgreSQL) | Implemented |
| RBAC middleware (endpoint → required scope) | `crates/ferrum-gateway/src/server.rs` |
| Admin token lifecycle API (`POST/GET/DELETE/rotate`) | [`docs/security/scoped-tokens-rbac.md`](../security/scoped-tokens-rbac.md) |
| `ferrumctl admin tokens` CLI | [`docs/guides/operator.md`](./operator.md) |
| TTL enforcement (>90d rejected) | `test_create_token_rejects_excessive_ttl` |
| Durable revocation (`revoked_at` in store) | `crates/ferrum-store/src/sqlite/tokens.rs` and `postgres/tokens.rs` |

### Acceptance targets

- Read-only token cannot call mutating endpoints (SEC-1).
- Agent token cannot approve proposals (SEC-2).
- Auditor token cannot execute actions (SEC-3).
- Revoked token returns 401 (SEC-4).
- Expired token returns 401 (SEC-5).
- Audit log records actor, action, and result for current scope (SEC-6).
- Tenant A cannot read tenant B data — not available by single-tenant design decision; no multi-tenant claim.

## Bearer auth hardening

1. Generate token on the target host (never print to logs):
   ```bash
   openssl rand -hex 32
   ```
2. Store token with `chmod 640` and restricted ownership (`root:ferrumgate`).
3. Set `FERRUMD_AUTH_MODE=Bearer` in the environment file.
4. Deploy behind a TLS-terminating reverse proxy.
5. Do not print tokens in logs or command history.
6. Rotate token after initial setup and periodically:
   - Update env/config with new token.
   - Restart ferrumd.
   - Verify new token returns 200 and old token returns 401.
   - Record rotation in audit log.

> **Rotation validated on target host**: Token rotation procedures are documented above.

## Audit log (SEC-6)

### What is logged

| Action | Trigger endpoint |
|--------|------------------|
| `token_create` | `POST /v1/admin/tokens` |
| `token_revoke` | `DELETE /v1/admin/tokens/{id}` |
| `token_rotate` | `POST /v1/admin/tokens/{id}/rotate` |
| `policy_bundle_create` | `POST /v1/policy-bundles` |
| `policy_bundle_activate` | `PUT /v1/policy-bundles/{id}/active` |
| `policy_bundle_rollback` | `POST /v1/policy-bundles/{id}/rollback` |
| `approval_resolve` | `POST /v1/approvals/{id}/resolve` |
| `execution_cancel` | `POST /v1/executions/{id}/cancel` |

### How to query

```bash
# API (requires admin:audit scope)
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/admin/audit-logs?action=token_create&limit=10"

# CLI
ferrumctl admin audit list --limit 20
```

### Limitations

- Append is **best-effort**; store errors do not fail the primary action.
- No cryptographic signing or WORM storage.
- Not compliance-grade forensic audit.

## Secret handling

- Token material is **hashed** in the store; plaintext is never persisted.
- DSN credentials in env files should use `chmod 640` and `root:ferrumgate` ownership.
- TLS client keys: `chmod 600` and `ferrumgate:ferrumgate` ownership.
- Certificate rotation requires a ferrumd restart because DSN is parsed once at startup.

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

## Hardening checklist

### Immediate (pilot)

- [ ] Use `Bearer` auth mode in deployed pilot config.
- [ ] Generate token with `openssl rand -hex 32`.
- [ ] Store token with `chmod 640` and restricted ownership.
- [ ] Rotate token after initial setup and periodically.
- [ ] Deploy behind TLS-terminating reverse proxy.
- [ ] Do not print tokens in logs or command history.

### Long-term

- Implement tenant_id in schema and store filters.
- Add PostgreSQL RLS as defense-in-depth.
- Evaluate OIDC/JWT/SSO integration.
- Evaluate mTLS for service-to-service auth.

## Related docs

- [`operator.md`](./operator.md) — Token rotation and incident response.
