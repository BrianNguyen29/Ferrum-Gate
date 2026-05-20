# Scoped Token Implementation Evidence

> **Date**: 2026-05-20
> **Scope**: Phase 4 security and tenant model — scoped opaque bearer tokens, RBAC enforcement, admin token API, ferrumctl CLI, and TTL enforcement
> **Owner**: Engineering
> **Status**: IMPLEMENTATION COMPLETE — not production-ready claim

---

## Summary

This artifact records the implementation and automated-test validation of the Phase 4 scoped token system. All operator decisions from `2026-05-20-security-model-operator-decisions.md` were implemented under the signed defaults (single-tenant T1, opaque scoped tokens, durable revocation, 90-day max TTL, full RBAC role set).

---

## What was implemented

### 1. Token types and contracts

- **`ferrum-proto/src/token.rs`**: Added `ScopedToken`, `TokenRole` (Admin, Operator, PolicyAuthor, Auditor, Agent, ReadOnly), `AuthMode::Scoped`.
- **`ferrum-proto/src/api.rs`**: Added `ApiErrorCode::Forbidden` for RBAC denial.
- Two-hash architecture:
  - `token_lookup_hash`: deterministic `blake3(token_value)` for fast DB lookup
  - `token_hash`: secure `blake3(salt || token_value)` for authentication verification
  - `token_salt`: random 16-byte hex salt per token

### 2. Store layer

- **`ferrum-store/src/repos.rs`**: `TokenRepo` trait with insert/get/get_by_lookup_hash/list/revoke/touch.
- **`ferrum-store/src/sqlite/tokens.rs`**: Full SQLite implementation.
- **`ferrum-store/src/postgres/tokens.rs`**: Compile-time skeleton (returns `StoreError::Other("not yet implemented")`).
- **Migration `007_add_tokens.sql`**: SQLite `scoped_tokens` table with `token_lookup_hash`, `token_hash`, `token_salt` columns.
- **Migration `postgres/001_initial.sql`**: PostgreSQL `scoped_tokens` table updated with same columns.
- `CURRENT_SCHEMA_VERSION` bumped to 7.

### 3. Gateway auth middleware

- **`crates/ferrum-gateway/src/server.rs`**: Replaced `bearer_auth_middleware` with `auth_middleware` supporting three modes:
  - `Disabled`: no auth
  - `Bearer`: global single token (backward-compatible)
  - `Scoped`: lookup by `token_lookup_hash`, verify with salted `token_hash`, check scope, check revocation, check expiration
- Endpoint-to-scope mapping covers 15 scopes across public, lifecycle, approvals, policy, provenance, bridge, and admin endpoints.
- Best-effort `last_used_at` update via `tokio::spawn` (fire-and-forget).

### 4. Admin token API

- `POST /v1/admin/tokens` — create token with TTL validation (max 90 days)
- `GET /v1/admin/tokens` — list tokens with actor/role/active filters
- `DELETE /v1/admin/tokens/{id}` — revoke token
- `POST /v1/admin/tokens/{id}/rotate` — rotate token with TTL validation (max 90 days)

### 5. ferrumctl CLI

- `ferrumctl admin tokens list` — list tokens
- `ferrumctl admin tokens create` — create token with `--expires-in-days` / `--expires-at`
- `ferrumctl admin tokens revoke` — revoke token
- `ferrumctl admin tokens rotate` — rotate token
- Client-side TTL validation (max 90 days) for create and rotate.

### 6. TTL enforcement

- Server-side: both `create_token` and `rotate_token` handlers reject `expires_at > now + 90 days` with `400 Bad Request`.
- Client-side: `ferrumctl` rejects the same condition before sending the request.
- Default TTL on rotate (when not specified): 90 days.

---

## Automated test evidence

All tests pass in `cargo test --package ferrum-gateway`:

| Test | What it validates |
|------|-------------------|
| `test_scoped_token_create_and_list` | Admin can create and list tokens via API |
| `test_scoped_token_revoked_fails` | Revoked token returns 401 |
| `test_scoped_token_expired_fails` | Expired token returns 401 |
| `test_sec1_read_only_token_cannot_mutate` | Read-only token cannot call mutating endpoints (403) |
| `test_sec2_agent_token_cannot_approve` | Agent token cannot resolve approvals (403) |
| `test_sec3_auditor_token_cannot_execute` | Auditor token cannot authorize executions (403) |
| `test_sec4_revoked_token_returns_401` | Revoked token rejected by middleware |
| `test_sec5_expired_token_returns_401` | Expired token rejected by middleware |
| `test_token_admin_api_create_and_revoke` | Full create + revoke lifecycle via API |
| `test_create_token_rejects_excessive_ttl` | Create with 91-day expiry returns 400 |
| `test_rotate_token_rejects_excessive_ttl` | Rotate with 91-day expiry returns 400 |

**Result**: 87 tests passed; 0 failed; 0 ignored.

---

## What is NOT implemented / deferred

- **SEC-6 (audit log)**: Dedicated audit log schema and provenance integration deferred to later phase.
- **PostgreSQL token repo**: Skeleton only; full implementation deferred until PG production prioritization.
- **OIDC/JWT**: Deferred per operator decision (Q2).
- **Multi-tenant**: Single-tenant only per operator decision (Q1).

---

## Non-claims

- **NOT production-ready**: Implementation completeness does not constitute a production-ready claim.
- **NOT target-host validated**: All tests run locally against in-memory SQLite.
- **NOT audited**: No third-party security audit performed.
- **NOT full G2**: Full G2 closure still requires Block A (real domain), target-host SLO evidence, and operator re-signoff.

---

## Related artifacts

- `docs/implementation-path/artifacts/2026-05-20-security-model-operator-decisions.md` — Operator decisions that authorized this implementation
- `docs/production-readiness-v2/12-endpoint-to-scope-mapping.md` — Endpoint-to-scope mapping
- `docs/production-readiness-v2/13-token-api-contract.md` — Token API contract
- `docs/production-readiness-v2/14-ferrumctl-admin-tokens-cli-spec.md` — CLI spec
- `docs/production-readiness-v2/15-revocation-durability-tradeoff.md` — Revocation tradeoff note
- `docs/production-readiness-v2/16-operator-shortcut-decision-packet.md` — Condensed decision packet
- `docs/production-readiness-v2/10-evidence-checklist.md` — Phase 4 checklist
- `docs/production-readiness-v2/11-blockers-and-unblock-plan.md` — Blocker status

---

*End of artifact — implementation evidence only; no production-ready claim.*
