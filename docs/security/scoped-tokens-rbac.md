# FerrumGate Scoped Tokens and RBAC

> **Parent Plan:** [`guides/README.md`](../guides/README.md)

This document explains FerrumGate's scoped token model, role mapping, enforced route scopes, and request authorization flow. It documents current implementation behavior.

## 1. Purpose and Boundary

Scoped tokens limit what an actor can do against FerrumGate's HTTP API when `AuthMode::Scoped` is enabled. They are intended to support least-privilege access for humans, operators, service accounts, and agents.

Scoped tokens are **not**:

- OIDC / SSO federation.
- Multi-tenant isolation.
- SOC 2 / compliance certification.

## 2. Auth Modes

| Mode | Intended use | Notes |
|------|--------------|-------|
| `Disabled` | Local development only | Must not be exposed on non-loopback deployed endpoints. |
| `Bearer` | Transitional/operator bootstrap mode | One shared bearer token gates non-public routes; no per-scope authorization. |
| `Scoped` | Preferred least-privilege mode | Token lookup, hash verification, revocation, expiry, and scope checks are enforced. |

Operational bootstrap can use an existing safe auth mode to create the first admin scoped token, then move the deployment to `AuthMode::Scoped`.

## 3. Token Model

The token model is implemented in `crates/ferrum-proto/src/token.rs` and backed by store repositories in `crates/ferrum-store`.

| Field / concept | Purpose |
|-----------------|---------|
| `actor_id` | Human, agent, service account, or operator identity label. |
| `role` | One of `Admin`, `Operator`, `PolicyAuthor`, `Auditor`, `Agent`, `ReadOnly`. |
| `scopes` | Explicit API capabilities granted to the token. |
| `description` | Human-readable purpose / ownership note. |
| `expires_at` | Expiration timestamp; expired tokens fail authentication. |
| `last_used_at` | Best-effort timestamp updated after successful scoped authentication. |
| `revoked_at` / `revoked_reason` | Revocation state and operator reason. |
| `rotated_from` | Links a rotated token to its predecessor. |
| lookup hash / salted hash / salt | Secret-safe lookup and verification material. Secret token values are not recoverable from list responses. |

Token values are printed only at create/rotate time. List operations return metadata only.

## 4. Roles and Default Scope Families

Implemented defaults are defined in `TokenRole::default_scopes()`.

| Role | Intended use | Default scope family |
|------|--------------|----------------------|
| `Admin` | Full administrative control | `*` wildcard. |
| `Operator` | Operational execution and approvals | Approval, execution, provenance, and operational scopes. |
| `PolicyAuthor` | Policy bundle management | Policy read/write and read-only visibility scopes. |
| `Auditor` | Read-only audit and provenance review | Provenance and audit visibility scopes. |
| `Agent` | Agent/tool execution under governance | Intent/proposal/capability/execution scopes, excluding approval/admin capabilities. |
| `ReadOnly` | Safe observation | Read-only visibility scopes. |

When exact defaults matter, inspect `TokenRole::default_scopes()` in code rather than copying stale scope lists.

## 5. Current Enforced Route Scopes

The following scopes are currently enforced by `required_scope_for_path()` for HTTP routes.

| Scope | Example route family | Purpose |
|-------|----------------------|---------|
| `intent:submit` | `POST /v1/intents/compile`, `GET /v1/intents` | Submit or inspect intents. |
| `proposal:evaluate` | `POST /v1/proposals/{proposal_id}/evaluate` | Evaluate proposals. |
| `capability:mint` | `POST /v1/capabilities/mint`, revoke capability routes | Mint/revoke capabilities. |
| `execution:authorize` | `POST /v1/executions/authorize` | Authorize execution. |
| `execution:prepare` | `POST /v1/executions/{id}/prepare` | Prepare side effects. |
| `execution:execute` | `POST /v1/executions/{id}/execute`, cancel | Execute/cancel actions. |
| `execution:verify` | `POST /v1/executions/{id}/verify`, evaluate outcome | Verify/evaluate outcomes. |
| `execution:commit` | `POST /v1/executions/{id}/commit` | Commit verified side effects. |
| `execution:compensate` | `POST /v1/executions/{id}/compensate` | Run compensation/rollback flow. |
| `approval:read` | Approval list/get routes | Inspect pending approvals without resolving them. |
| `approval:resolve` | Approval resolve route | Approve or deny pending approvals. |
| `policy:read` | Policy bundle read/simulate/diff/version routes | Read policy state and simulations. |
| `policy:write` | Policy bundle create/update/delete/activate/rollback | Mutate policy state. |
| `provenance:read` | Provenance, lineage, bridges, execution detail | Read provenance and lineage. |
| `provenance:write` | `POST /v1/provenance/ingest` | Ingest trusted external provenance events. |
| `admin:tokens` | Admin token lifecycle routes | Create/list/revoke/rotate scoped tokens. |
| `admin:agents` | Admin agent registry routes | Create/list/revoke agent identities. |
| `admin:lifecycle-outbox:read` | Lifecycle outbox list/get routes | Inspect reconciliation records. |
| `admin:lifecycle-outbox:write` | Lifecycle outbox retry/resolve routes | Retry reconciliation or mark operator review resolved. |
| `admin:audit` | Admin audit-log routes | Read audit logs and audit checkpoints. |
| `admin:mfa` | Admin agent MFA routes | Enroll, verify, rotate, or disable agent MFA. |
| `admin:mfa:breakglass` | Admin agent MFA break-glass bypass | Disable or rotate an active MFA factor without TOTP re-verification (requires non-empty reason, auditable). |
| `*` | Admin wildcard | Matches any required scope. |

### Reserved scopes

- `admin:config` appears in planning language but has no currently enforced route unless a config endpoint exists.
- `backup:run` can appear in role defaults / operational planning but has no current route-enforced endpoint in this scope map.

These names must not be presented as active route-enforced controls until corresponding endpoints exist.

## 6. Middleware Flow

When `AuthMode::Scoped` is enabled, the request flow is:

1. Exact public whitelist check for liveness and shallow readiness endpoints.
2. Bearer token extraction from the request.
3. Lookup-hash computation and token lookup.
4. Salted hash verification.
5. Revocation check.
6. Expiration check.
7. Required-scope lookup for `(method, path)`.
8. Exact or wildcard (`*`) scope match.
9. Best-effort `last_used_at` update after successful auth.

## 7. Deny-by-Default Behavior

Only exact liveness and shallow-readiness routes are public. Deep readiness, metrics, workload, and admin routes require auth/scopes when authentication is enabled. Unknown or unmapped routes are not silently allowed: the fallback requires `admin:tokens`.

This can make a non-existent route return an auth error for non-admin scoped tokens before routing reaches a 404. That behavior is safe, though it should be documented for operators.

## 8. Token Lifecycle and CLI Examples

Use placeholders only; never paste real token values into docs, tickets, or logs.

```bash
# List metadata only
ferrumctl admin tokens list --active-only --format text

# Create a token; secret value is printed once
ferrumctl admin tokens create \
  --actor-id <ACTOR_ID> \
  --role <ROLE> \
  --scope <SCOPE> \
  --expires-in-days <N>

# Revoke a token
ferrumctl admin tokens revoke <TOKEN_ID> --reason <TEXT>

# Rotate a token
ferrumctl admin tokens rotate <TOKEN_ID> --reason <TEXT> --expires-in-days <N>
```

Supported CLI lifecycle commands are implemented in `bins/ferrumctl/src/main.rs`.

## 9. Operational Guidance

- Prefer short-lived tokens and least-privilege scopes.
- Use role defaults only when they match the actor's actual responsibility.
- Use explicit scopes for service accounts and agents.
- Rotate tokens during personnel, CI, or environment changes.
- Revoke unused or compromised tokens immediately.
- Treat create/rotate output as secret material; it is shown once.
- Use list output for metadata review only; it is intentionally redacted.
- Review audit logs for token create/revoke/rotate actions.

## 10. Notes

- OIDC / SSO is not covered in this document.
- Multi-tenancy is not provided; scoped tokens are single-deployment controls.
- Exhaustive RBAC integration tests are outside this document's scope.
- Dedicated CLI parsing tests for token subcommands are outside this document's scope.
- `last_used_at` / `rotated_from` integration coverage is outside this document's scope.
- This document records current route-enforced scopes and flags reserved names.

## 11. Related Auth Documents

- OIDC / JWT federation: see [`docs/security/oidc-jwt-federation.md`](./oidc-jwt-federation.md).
- Agent identity (Ed25519): see [`docs/security/agent-identity-ed25519.md`](./agent-identity-ed25519.md).

## 12. Evidence Links

- [`security-model.md`](../guides/security-model.md) — Scoped token store and RBAC middleware implementation

## 12. Notes

- This document records current behavior and boundaries.
