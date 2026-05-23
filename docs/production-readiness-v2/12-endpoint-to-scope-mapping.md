# 12 — Endpoint-to-Scope Mapping

> **Status**: Implemented — scoped token endpoints (create/list/revoke/rotate) implemented 2026-05-21. Operator signoff and Phase 4 full signoff remaining. See [`10-evidence-checklist.md`](./10-evidence-checklist.md) §Phase 4.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **Depends on**: [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md)

---

## Goal

Map every current and planned API endpoint to the minimum scoped-token scope required to access it. This is a prerequisite for RBAC middleware design and for the operator to review and approve the scope list.

## Current state

- Auth is `Disabled` or `Bearer` (global single token).
- No per-endpoint authorization beyond "bearer token present."
- All workload endpoints share the same auth gate.
- Monitoring endpoints are intentionally unauthenticated.

## Scope definitions

These scopes are drawn from [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) §Scopes. This document assigns them to concrete routes.

| Scope | Meaning |
|-------|---------|
| `intent:submit` | Compile and list intents |
| `proposal:evaluate` | Evaluate proposals |
| `capability:mint` | Mint and revoke capabilities |
| `execution:authorize` | Authorize executions |
| `execution:prepare` | Prepare execution side effects |
| `execution:execute` | Execute side effects |
| `execution:verify` | Verify execution outcomes |
| `execution:compensate` | Compensate / rollback executions |
| `approval:resolve` | Resolve (approve/reject) approvals |
| `policy:read` | Read policy bundles |
| `policy:write` | Create, update, delete, activate, rollback policy bundles |
| `provenance:read` | Query provenance and lineage |
| `admin:tokens` | Create, list, revoke, rotate scoped tokens |
| `admin:config` | View and mutate server configuration (future) |
| `backup:run` | Trigger backup operations (future) |

## Endpoint-to-scope mapping

### Public endpoints (no authentication required)

| Method | Route | Scope required | Notes |
|--------|-------|----------------|-------|
| `GET` | `/v1/healthz` | *(none)* | Always public; used by load balancers |
| `GET` | `/v1/readyz` | *(none)* | Always public; shallow readiness |
| `GET` | `/v1/readyz/deep` | *(none)* | Always public; deep readiness with store check |
| `GET` | `/v1/metrics` | *(none)* | Always public; Prometheus scrape endpoint |

### Intent and proposal endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/intents/compile` | `intent:submit` | Creates a new intent |
| `GET` | `/v1/intents` | `intent:submit` | Lists intents |
| `POST` | `/v1/proposals/{proposal_id}/evaluate` | `proposal:evaluate` | Evaluates a proposal against active policy |

### Capability endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/capabilities/mint` | `capability:mint` | Mints a capability lease |
| `POST` | `/v1/capabilities/{capability_id}/revoke` | `capability:mint` | Revokes a capability lease |

### Execution endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/executions/authorize` | `execution:authorize` | Authorizes an execution |
| `POST` | `/v1/executions/{execution_id}/prepare` | `execution:prepare` | Prepares side effects |
| `POST` | `/v1/executions/{execution_id}/execute` | `execution:execute` | Executes side effects |
| `POST` | `/v1/executions/{execution_id}/verify` | `execution:verify` | Verifies outcome |
| `POST` | `/v1/executions/{execution_id}/compensate` | `execution:compensate` | Compensates / rolls back |
| `POST` | `/v1/executions/{execution_id}/cancel` | `execution:execute` | Cancels a running execution |
| `POST` | `/v1/executions/{execution_id}/evaluate-outcome` | `execution:verify` | Evaluates outcome report |
| `GET`  | `/v1/executions/{execution_id}` | `provenance:read` | Inspection is read-only |

### Approval endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `GET` | `/v1/approvals` | `approval:resolve` | List pending approvals |
| `GET` | `/v1/approvals/{approval_id}` | `approval:resolve` | Inspect a single approval |
| `POST` | `/v1/approvals/{approval_id}/resolve` | `approval:resolve` | Approve or reject |

### Policy bundle endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/policy-bundles` | `policy:write` | Create a new bundle |
| `GET` | `/v1/policy-bundles` | `policy:read` | List bundles |
| `GET` | `/v1/policy-bundles/{bundle_id}` | `policy:read` | Get a single bundle |
| `PUT` | `/v1/policy-bundles/{bundle_id}` | `policy:write` | Update a bundle |
| `DELETE` | `/v1/policy-bundles/{bundle_id}` | `policy:write` | Delete a bundle |
| `PUT` | `/v1/policy-bundles/{bundle_id}/active` | `policy:write` | Activate a bundle |
| `POST` | `/v1/policy-bundles/simulate` | `policy:read` | Simulation is read-only side-effect-free |
| `GET` | `/v1/policy-bundles/{bundle_id}/versions` | `policy:read` | List version history |
| `GET` | `/v1/policy-bundles/{bundle_id}/diff` | `policy:read` | Diff two versions |
| `POST` | `/v1/policy-bundles/{bundle_id}/rollback` | `policy:write` | Rollback to a previous version |

### Provenance and lineage endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/provenance/query` | `provenance:read` | Query provenance events |
| `POST` | `/v1/provenance/lineage` | `provenance:read` | Multi-hop lineage query |
| `GET` | `/v1/provenance/lineage/{execution_id}` | `provenance:read` | Execution lineage graph |
| `POST` | `/v1/provenance/ingest` | `provenance:read` | Ingest external provenance (write to provenance stream; read scope because it feeds audit) |

### Bridge endpoints

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `GET` | `/v1/bridges` | `provenance:read` | List configured bridges |
| `GET` | `/v1/bridges/{bridge_id}/tools` | `provenance:read` | List tools exposed by a bridge |

### Admin/token endpoints (implemented 2026-05-21)

| Method | Route | Minimum scope | Notes |
|--------|-------|---------------|-------|
| `POST` | `/v1/admin/tokens` | `admin:tokens` | Create a scoped token |
| `GET` | `/v1/admin/tokens` | `admin:tokens` | List tokens (excluding raw token values) |
| `DELETE` | `/v1/admin/tokens/{token_id}` | `admin:tokens` | Revoke a token |
| `POST` | `/v1/admin/tokens/{token_id}/rotate` | `admin:tokens` | Rotate a token (revoke old, create new) |

## Role-to-scope mappings (minimum viable)

Derived from [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) §RBAC.

| Role | Scopes granted |
|------|----------------|
| `admin` | `*` (all scopes) |
| `operator` | `approval:resolve`, `provenance:read`, `policy:read`, `execution:verify`, `backup:run` |
| `policy_author` | `policy:read`, `policy:write`, `provenance:read` |
| `auditor` | `provenance:read` only |
| `agent` | `intent:submit`, `proposal:evaluate`, `capability:mint`, `execution:authorize`, `execution:prepare`, `execution:execute`, `execution:verify`, `execution:compensate` |
| `read_only` | `policy:read`, `provenance:read` |

## Design invariants

1. **Deny-by-default**: Any endpoint not listed above returns `403 Forbidden` for scoped-token auth.
2. **Bearer backward compatibility**: When auth mode is `Bearer`, the global bearer token retains full access (all scopes) until the operator explicitly enables scoped-token enforcement.
3. **Monitoring is always public**: `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics` never require authentication.
4. **Admin endpoints require `admin:tokens`**: Token lifecycle management is restricted to the `admin:tokens` scope; no other role can create or revoke tokens.
5. **Agent cannot approve**: The `agent` role does not include `approval:resolve`.
6. **Auditor cannot execute**: The `auditor` role does not include any execution or capability scope.

## Non-claims

- **NOT production-ready**: Scoped-token enforcement requires explicit operator enablement; bearer-only remains the production pilot auth mode until then.
- **NOT multi-tenant**: This mapping assumes single-tenant (T1) from [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md).
- **Partial implementation**: RBAC middleware and scope enforcement implemented 2026-05-21 for token lifecycle endpoints; operator signoff and Phase 4 full signoff remaining.

## Related docs

- [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) — Source of scope and role definitions
- [`13-token-api-contract.md`](13-token-api-contract.md) — Token API contract
- [`14-ferrumctl-admin-tokens-cli-spec.md`](14-ferrumctl-admin-tokens-cli-spec.md) — CLI surface spec

---

*End of file — Endpoint-to-Scope Mapping (planning artifact only).*
