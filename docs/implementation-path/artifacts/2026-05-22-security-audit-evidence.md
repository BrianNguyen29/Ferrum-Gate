# Consolidated Security Audit Evidence — 2026-05-22

> **Artifact ID**: 2026-05-22-security-audit-evidence
> **Date**: 2026-05-22
> **Owner**: Engineering
> **Scope**: Compilation of existing SEC-1–SEC-6, scoped-token, audit-log, and security-invariant evidence. No new implementation.
> **Status**: EVIDENCE COMPILATION — not a security audit pass; not production-ready

---

## 1. Scope and Explicit Non-Claims

### What this artifact covers
- SEC-1 through SEC-5 automated test evidence (RBAC denial, revocation, expiry)
- SEC-6 audit log implementation evidence
- Scoped token RBAC model and middleware evidence
- Token lifecycle API and CLI evidence
- Security invariant enforcement (capability TTL, single-use, lineage)
- Local dependency audit snapshot (`cargo-deny`, `cargo-audit`)
- Gaps and deferred controls

### What this artifact does NOT claim
- **NOT production-ready**: Compilation of evidence does not constitute a production-ready claim.
- **NOT full G2 closure**: Full G2 requires Block A (real domain), target-host SLO evidence, and operator re-signoff.
- **NOT Block A closed**: Block A remains WAIVED/CONDITIONAL (DuckDNS only); real owned domain still required.
- **NOT a third-party security audit**: No external penetration test or code audit performed.
- **NOT target-host validated**: All automated tests run locally against in-memory SQLite.
- **NOT PostgreSQL production validated**: PostgreSQL token and audit log repos are implemented and pass tests, but no production PostgreSQL deployment validation claimed.
- **NOT a compliance-grade audit system**: SEC-6 is best-effort append-only; no cryptographic signing or WORM storage.
- **NOT HA/multi-node**: Single-node only.
- **NOT multi-tenant**: Single-tenant per operator decision Q1.
- **No secrets recorded**: No tokens, passwords, keys, or credentials appear in this artifact.

---

## 2. SEC-1 Through SEC-6 Evidence Table

| Gate | Description | Evidence Location | Status |
|------|-------------|-------------------|--------|
| SEC-1 | Read-only token cannot mutate | `test_sec1_read_only_token_cannot_mutate` in `crates/ferrum-gateway/src/server.rs:10856` | ✅ IMPLEMENTED — `POST /v1/policy-bundles` returns 403 for `read_only` token |
| SEC-2 | Agent token cannot approve | `test_sec2_agent_token_cannot_approve` in `crates/ferrum-gateway/src/server.rs:10899` | ✅ IMPLEMENTED — `POST /v1/approvals/{id}/resolve` returns 403 for `agent` token |
| SEC-3 | Auditor token cannot execute | `test_sec3_auditor_token_cannot_execute` in `crates/ferrum-gateway/src/server.rs:10946` | ✅ IMPLEMENTED — `POST /v1/executions/authorize` returns 403 for `auditor` token |
| SEC-4 | Revoked token fails | `test_sec4_revoked_token_returns_401` in `crates/ferrum-gateway/src/server.rs:10988` | ✅ IMPLEMENTED — revoked token returns 401 via `auth_middleware` |
| SEC-5 | Expired token fails | `test_sec5_expired_token_returns_401` in `crates/ferrum-gateway/src/server.rs:11030` | ✅ IMPLEMENTED — expired token returns 401 via `auth_middleware` |
| SEC-6 | Audit log records admin/policy/approval/token actions | `docs/implementation-path/artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md` | ✅ IMPLEMENTED — minimal append-only audit log with best-effort store append; SQLite migration 008 + Postgres schema; `GET /v1/admin/audit-logs` with `admin:audit` scope; `ferrumctl admin audit list` |

### Test execution evidence
- `cargo test --package ferrum-gateway` passes with all SEC-1–SEC-5 tests included (87 tests passed, 0 failed, 0 ignored as of 2026-05-20 artifact).
- `cargo test --workspace` passes all packages (verified 2026-05-17 and 2026-05-21).

---

## 3. Scoped Token RBAC Evidence

### 3.1 Token types and roles
- **File**: `crates/ferrum-proto/src/token.rs`
- **Roles**: `Admin`, `Operator`, `PolicyAuthor`, `Auditor`, `Agent`, `ReadOnly`
- **Scope derivation**: Each role has a `default_scopes()` method returning permitted scopes.
- **Admin scope**: `"*"` (wildcard — all scopes).

### 3.2 Two-hash architecture
- `token_lookup_hash`: deterministic `blake3(token_value)` for fast DB lookup.
- `token_hash`: secure `blake3(salt || token_value)` for authentication verification.
- `token_salt`: random 16-byte hex salt per token.
- Plaintext token value is returned exactly once at creation and never stored.

### 3.3 Store layer
- **Trait**: `TokenRepo` in `crates/ferrum-store/src/repos.rs` (lines 202–231)
- **SQLite implementation**: `crates/ferrum-store/src/sqlite/tokens.rs` — full implementation.
- **PostgreSQL implementation**: `crates/ferrum-store/src/postgres/tokens.rs` — **full implementation** (insert/get/get_by_lookup_hash/list/revoke/touch). *Note: 2026-05-20 scoped-token artifact previously noted this as "skeleton"; it has since been fully implemented and tested (72 postgres-feature tests pass).*
- **Migrations**: SQLite `007_add_tokens.sql`; PostgreSQL `001_initial.sql` updated.

### 3.4 Gateway middleware
- **File**: `crates/ferrum-gateway/src/server.rs`
- **Modes**: `Disabled`, `Bearer`, `Scoped`
- **Scoped flow**:
  1. Extract `Authorization: Bearer <token>` header.
  2. Lookup by `token_lookup_hash`.
  3. Verify with salted `token_hash`.
  4. Check scope against endpoint-to-scope mapping.
  5. Check revocation (`revoked_at`).
  6. Check expiration (`expires_at`).
  7. Best-effort `last_used_at` update via `tokio::spawn`.
- **Endpoint mapping**: 15 scopes across public, lifecycle, approvals, policy, provenance, bridge, and admin endpoints (see `docs/production-readiness-v2/12-endpoint-to-scope-mapping.md`).

### 3.5 Admin token API
- `POST /v1/admin/tokens` — create token with TTL validation (max 90 days).
- `GET /v1/admin/tokens` — list tokens with actor/role/active filters.
- `DELETE /v1/admin/tokens/{id}` — revoke token.
- `POST /v1/admin/tokens/{id}/rotate` — rotate token with TTL validation.

### 3.6 ferrumctl CLI
- `ferrumctl admin tokens list/create/revoke/rotate`
- Client-side TTL validation (max 90 days) for create and rotate.

### 3.7 TTL enforcement
- Server-side: `create_token` and `rotate_token` handlers reject `expires_at > now + 90 days` with 400 Bad Request.
- Client-side: `ferrumctl` rejects the same condition before sending the request.
- Default TTL on rotate (when not specified): 90 days.

---

## 4. Audit Log Evidence

### 4.1 Domain types
- **File**: `crates/ferrum-proto/src/audit_log.rs`
- **Entry fields**: `id`, `created_at`, `actor_id`, `action`, `resource_type`, `resource_id`, `result`, `metadata`.
- **Actions covered**: `token_create`, `token_revoke`, `token_rotate`, `policy_bundle_create`, `policy_bundle_activate`, `policy_bundle_rollback`, `approval_resolve`, `execution_cancel`.

### 4.2 Store layer
- **Trait**: `AuditLogRepo` in `crates/ferrum-store/src/repos.rs` (lines 181–195)
- **SQLite implementation**: `crates/ferrum-store/src/sqlite/audit_log.rs` — full implementation with async write queue for non-blocking append.
- **PostgreSQL implementation**: `crates/ferrum-store/src/postgres/audit_log.rs` — full implementation using direct `INSERT` via `sqlx`.

### 4.3 Schema migrations
- SQLite: migration `008_add_audit_log.sql` adds `audit_log` table with indexed `timestamp` and `actor_token_id`.
- PostgreSQL: `migrations/postgres/001_initial.sql` updated with equivalent `audit_log` table and indexes.

### 4.4 Gateway endpoint
- `GET /v1/admin/audit-logs` — requires `admin:audit` scope.
- Supports `?action=`, `?resource_type=`, `?resource_id=`, `?cursor=`, `?limit=` (cursor-based pagination).
- Returns 403 if caller lacks `admin:audit`.

### 4.5 Best-effort append points
Append is **best-effort**; store errors are logged but do not fail the primary action. Audit loss is possible under extreme store pressure.

| Action | Trigger Endpoint |
|--------|------------------|
| `token_create` | `POST /v1/admin/tokens` |
| `token_revoke` | `DELETE /v1/admin/tokens/{id}` |
| `token_rotate` | `POST /v1/admin/tokens/{id}/rotate` |
| `policy_bundle_create` | `POST /v1/policy-bundles` |
| `policy_bundle_activate` | `PUT /v1/policy-bundles/{bundle_id}/active` |
| `policy_bundle_rollback` | `POST /v1/policy-bundles/{bundle_id}/rollback` |
| `approval_resolve` | `POST /v1/approvals/{id}/resolve` |
| `execution_cancel` | `POST /v1/executions/{id}/cancel` |

### 4.6 CLI support
- `ferrumctl admin audit list` — lists recent audit entries with optional `--limit`.
- Outputs table or JSON (`--json`).

---

## 5. Security Invariant Enforcement

| Invariant | Enforcement | Evidence |
|-----------|-------------|----------|
| Intent-scoped execution | Gateway validates intent scope before side effects | `crates/ferrum-gateway/src/server.rs` lifecycle handlers |
| Single-use capability | Capabilities consumed once; status transitions `Active` → `Used`/`Expired`/`Revoked` | `crates/ferrum-cap` + store `update_status_if_active` |
| Capability TTL max 300s | Hardcoded max; expired capabilities return `CapabilityExpired` | `ferrum-cap` enforcement + tests |
| Provenance-first lineage | Minimum lineage chain required before side effect | `ActionProposalSubmitted → PolicyEvaluated → CapabilityMinted → ... → Terminal` |
| Rollback-by-default | Rollback contracts generated for R1/R2 executions | `crates/ferrum-gateway/src/rollback/` + store `rollback_contracts` table |
| No auto-commit (R3) | R3 executions require explicit operator approval | Approval flow in gateway |
| Output sanitization | Redaction/sanitization verified in MCP smoke | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §MCP-6 |
| Rate limiting | 2 req/s sustained, burst 50; per-IP via `tower_governor` | `crates/ferrum-gateway/src/server.rs` |
| Bearer auth constant-time comparison | `subtle::ConstantTimeEq` used for global bearer token | `auth_middleware` implementation |
| No plaintext token storage | Only `token_lookup_hash` + `token_hash` + `token_salt` stored | `crates/ferrum-proto/src/token.rs` |

---

## 6. Dependency Audit Snapshot

### 6.1 Local audit tools
- `cargo-deny v0.19.6` — installed and operational.
- `cargo-audit v0.22.1` — installed and operational.
- `make audit` runs `scripts/run_security_audit.sh`.

### 6.2 Last known clean snapshot (2026-05-17)
- **Command**: `make audit`
- **cargo-deny advisory DB**: ok; 1090 advisories loaded.
- **Dependencies scanned**: 384.
- **Actionable issues**: 0.
- **Ignored advisory**: `RUSTSEC-2023-0071` — uncompiled optional dependency (`rsa`); not present in active dependency tree.
- **Result**: `SECURITY AUDIT GATE: PASS`

### 6.3 Non-claims
- Dependency audit is **local-only**; not integrated into CI.
- Advisory database changes daily; snapshot above is from 2026-05-17.
- No guarantee that future dependency additions remain clean without re-running.

---

## 7. Verification Evidence

| Check | Command | Last Verified | Result |
|-------|---------|---------------|--------|
| Formatting | `cargo fmt --all -- --check` | 2026-05-21 | ✅ PASS |
| Type check | `cargo check --workspace` | 2026-05-21 | ✅ PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | 2026-05-21 | ✅ PASS |
| Workspace tests | `cargo test --workspace` | 2026-05-21 | ✅ PASS (all packages) |
| Store focused tests | `cargo test --package ferrum-store` | 2026-05-21 | ✅ PASS (SQLite + Postgres) |
| Gateway focused tests | `cargo test --package ferrum-gateway` | 2026-05-21 | ✅ PASS (scoped-auth + audit endpoint) |
| ferrumctl focused tests | `cargo test --package ferrumctl` | 2026-05-21 | ✅ PASS |
| Postgres feature check | `cargo check --package ferrum-store --features postgres` | 2026-05-21 | ✅ PASS |
| Postgres tests | `cargo test --package ferrum-store --features postgres` | 2026-05-21 | ✅ PASS (72 tests) |
| Local security audit | `make audit` | 2026-05-17 | ✅ PASS |
| Pre-target gate | `bash scripts/run_pre_target_gate.sh --full` | 2026-05-17 | ✅ PASS |

---

## 8. Gaps and Deferred Controls

| # | Gap / Deferred Control | Rationale | Priority |
|---|------------------------|-----------|----------|
| G.1 | OIDC/JWT/SSO | Deferred per operator decision Q2 (2026-05-20) | Later phase |
| G.2 | Multi-tenant row-level isolation | Single-tenant selected per operator decision Q1 | Later phase |
| G.3 | PostgreSQL RLS | Deferred with multi-tenant | Later phase |
| G.4 | Compliance-grade audit (cryptographic signing, WORM) | SEC-6 is operator accountability aid only | Post-v1 |
| G.5 | Durable capability persistence | In-memory revocation only; survives process restart deferred | Phase 3 |
| G.6 | Third-party security audit | No external pen-test or code review scheduled | Operator decision |
| G.7 | CI dependency scanning | Local-only due to CI cost constraints | Operator decision |
| G.8 | Application-level request body size limit | Proxy-owned per design | Operator config |
| G.9 | CORS implementation | Disabled by default; browser clients not supported in v1 | Operator config if needed |
| G.10 | Target-host security validation | All SEC tests local only | Blocked on operator deployment |
| G.11 | Real domain + HTTPS (Block A) | WAIVED/CONDITIONAL — DuckDNS accepted for pilot only | Operator action |
| G.12 | Default rate-limit SLO canonical pass | Failed (46.8% 429); max-valid config passed | Requires operator tuning |

---

## 9. Operator Signoff / Status

### Current signoff state
- **Phase 4 operator decisions**: Approved 2026-05-20 (`docs/implementation-path/artifacts/2026-05-20-security-model-operator-decisions.md`).
- **Conditional re-signoff**: BrianNguyen authorized conditional re-signoff for single-node SQLite pilot scope on 2026-05-21 (`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`).
- **Full G2 closure**: NOT COMPLETE — requires Block A resolution + operator final signoff.
- **Final production-ready signoff**: NOT COMPLETE — requires all prerequisites in `TEMPLATE-final-production-readiness-signoff.md`.

### Templates prepared for eventual closure
- `docs/implementation-path/artifacts/TEMPLATE-final-production-readiness-signoff.md` — reviewed and accepted as valid signoff form (planning signoff only; 2026-05-22).
- `docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md` — reviewed and accepted as valid signoff form (planning signoff only; 2026-05-22).

---

## 10. Related Artifacts

| Artifact | Purpose |
|----------|---------|
| `docs/implementation-path/artifacts/2026-05-20-scoped-token-implementation-evidence.md` | Scoped token implementation and test evidence |
| `docs/implementation-path/artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md` | SEC-6 audit log implementation evidence |
| `docs/implementation-path/artifacts/2026-05-20-security-model-operator-decisions.md` | Operator decisions Q1–Q6 |
| `docs/implementation-path/70-security-hardening-local-only-plan.md` | Local-only security hardening plan |
| `docs/production-readiness-v2/10-evidence-checklist.md` | Phase 4 checklist with SEC-1–SEC-6 status |
| `docs/production-readiness-v2/12-endpoint-to-scope-mapping.md` | Endpoint-to-scope mapping contract |
| `docs/production-readiness-v2/13-token-api-contract.md` | Token API contract |
| `docs/production-readiness-v2/15-revocation-durability-tradeoff.md` | Revocation tradeoff note |

---

*End of artifact — consolidated security audit evidence compilation. No production-ready claim. No secrets recorded.*
