# 04 — Security and Tenant Model ADR

> **Status**: Planning artifact. Design-only; not implemented.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Design the security control plane to move from bearer-only single-operator auth to scoped identity, RBAC, audit logging, and a tenant model that can evolve from single-tenant to multi-tenant.

## Current state

- Auth mode: `Disabled` or `Bearer`.
- Bearer token is sufficient for pilot.
- No multi-tenancy.
- No RBAC.
- No scoped API tokens.
- No OIDC/JWT/SSO.
- No tenant isolation.
- No token lifecycle API.
- No actor-level authorization beyond bearer possession.

## Gaps

| Gap | Severity |
|-----|----------|
| Single bearer token global power | High |
| No per-actor identity | High |
| No roles/RBAC | High |
| No tenant/org/workspace model | High |
| No scoped tokens | High |
| No admin audit log separate from provenance | Medium/High |
| Capability revocation durability concerns | Medium |
| No token rotation API | Medium |
| No OIDC/JWT/SSO | Medium |
| No mTLS option | Low/Medium |

## Proposed model

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

### RBAC (minimum viable)

| Role | Permissions |
|------|-------------|
| admin | manage config, policy, tokens, backups, users |
| operator | approve/reject, run restore/drill, view health |
| policy_author | create/update/simulate policy bundles |
| auditor | read-only lineage/executions/provenance |
| agent | submit intent/use MCP within scope |
| read_only | health, lineage, execution status |

### Token model

```
token_id
tenant_id
actor_id
role
scopes[]
expires_at
created_at
last_used_at
revoked_at
```

### Scopes

- `intent:submit`
- `proposal:evaluate`
- `capability:mint`
- `execution:authorize`
- `execution:prepare`
- `execution:execute`
- `execution:verify`
- `execution:compensate`
- `approval:resolve`
- `policy:read`
- `policy:write`
- `provenance:read`
- `admin:tokens`
- `admin:config`
- `backup:run`

## Tenant model options

### Option 1 — Single-tenant production (recommended first)

One deployment = one tenant.

**Pros:**
- Minimal code change.
- Fits self-hosted.
- Simple security.
- Faster to production posture.

**Cons:**
- Not SaaS multi-tenant.
- One deployment per customer.

### Option 2 — Row-level tenant_id

Every table has `tenant_id`; every query filters `tenant_id`.

**Pros:**
- Fits SaaS.
- Better scale.

**Cons:**
- Large store change.
- Cross-tenant test required for every endpoint.
- Easy to leak if filter forgotten.

### Option 3 — PostgreSQL RLS

`tenant_id` + PG Row-Level Security policies.

**Pros:**
- DB-level guard.
- Defense-in-depth.

**Cons:**
- Complex.
- Requires accurate session tenant context.
- Strict RLS migration/tests required.

### Recommended phasing

| Phase | Action |
|-------|--------|
| T1 | Single-tenant production hardening |
| T2 | Tenant model ADR |
| T3 | tenant_id in schema + store filters |
| T4 | PostgreSQL RLS as defense-in-depth |
| T5 | Multi-tenant production claim |

## Implementation tasks (design phase)

- [x] Write scoped token ADR/contract (`13-token-api-contract.md`).
- [x] Write RBAC endpoint mapping ADR (`12-endpoint-to-scope-mapping.md`).
- [x] Write audit log schema/evidence (`2026-05-21-sec6-audit-log-implementation-evidence.md`).
- [x] Write tenant model ADR (this doc; T1 single-tenant selected in `2026-05-20-security-model-operator-decisions.md`).
- [x] Define token revocation durability strategy (`15-revocation-durability-tradeoff.md`; durable revocation selected).
- [x] Define token rotation API contract (`13-token-api-contract.md`; implemented in admin token API/CLI).

## Acceptance criteria (design)

- [x] ADR approved by engineering and operator for T1 single-tenant scoped-token scope.
- [x] Token read-only cannot mutate (implemented/tested as SEC-1).
- [x] Agent token cannot approve (implemented/tested as SEC-2).
- [x] Auditor cannot execute (implemented/tested as SEC-3).
- [x] Tenant A cannot read tenant B (deferred by T1 single-tenant decision; no multi-tenant claim).
- [x] Revoked token returns 401 (implemented/tested as SEC-4).
- [x] Expired token returns 401 (implemented/tested as SEC-5).
- [x] Audit log records actor/action/result for current scope (implemented/tested as SEC-6 minimal audit log).

## Evidence required

- `security-model-adr.md`
- `tenant-model-adr.md`
- Design review signoff

## Non-claims

- **T1 scoped-token implementation complete/signed**: Scoped token store, RBAC middleware, admin token APIs/CLI, TTL enforcement, and SEC-6 minimal audit log are implemented and operator-reviewed for the current scope.
- **NOT full production security**: Tier 2 production security still requires real-domain revalidation, full G2 re-signoff, and final production posture signoff.
- **NOT multi-tenant**: T1 is single-tenant only.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.4, §3.5, §4 Phase 4
- [`docs/implementation-path/70-security-hardening-local-only-plan.md`](../../implementation-path/70-security-hardening-local-only-plan.md)
