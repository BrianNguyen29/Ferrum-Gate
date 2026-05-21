# SEC-6 Minimal Append-Only Audit Log — Implementation Evidence

> **Date**: 2026-05-21  
> **Status**: IMPLEMENTED  
> **Scope**: SEC-6 acceptance gate — minimal append-only audit log for admin/policy/approval/token actions  
> **Parent**: [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) Phase 4  
> **Non-claims**: Best-effort only; not a full compliance-grade audit system; not production-ready

---

## What was implemented

1. **Audit proto/domain types** (`crates/ferrum-proto/src/audit_log.rs`)  
   - `AuditLogEntry` domain type with fields: `id`, `created_at`, `actor_id`, `action`, `resource_type`, `resource_id`, `result`, `metadata`.
   - Action enum covers: `token_create`, `token_revoke`, `token_rotate`, `policy_bundle_create`, `policy_bundle_activate`, `policy_bundle_rollback`, `approval_resolve`, `execution_cancel`.

2. **Store repository** (`crates/ferrum-store/src/audit_log_repo.rs`)  
   - `AuditLogRepo` trait with `append(entry) -> Result<()>` and `list(action, resource_type, resource_id, cursor, limit) -> Result<(Vec<AuditLogEntry>, Option<String>)>` (cursor-based pagination).
   - **SQLite implementation** using the existing async write queue for non-blocking append.
   - **PostgreSQL implementation** using direct `INSERT` via `sqlx`.

3. **Schema migrations**  
   - SQLite: migration `008_audit_log.sql` adds `audit_log` table with indexed `timestamp` and `actor_token_id`.
   - PostgreSQL: schema updated in `migrations/postgres/001_initial.sql` with equivalent `audit_log` table and indexes.

4. **Gateway endpoint** (`crates/ferrum-gateway/src/server.rs`)  
   - `GET /v1/admin/audit-logs` — requires `admin:audit` scope.
   - Supports `?action=`, `?resource_type=`, `?resource_id=`, `?cursor=`, and `?limit=` query parameters (cursor-based pagination).
   - Returns `403` if the caller lacks `admin:audit`.

5. **Best-effort audit append points**  
   - `token_create` — appended on successful `POST /v1/admin/tokens`.
   - `token_revoke` — appended on successful `DELETE /v1/admin/tokens/{id}`.
   - `token_rotate` — appended on successful `POST /v1/admin/tokens/{id}/rotate`.
   - `policy_create` — appended on successful `POST /v1/policy-bundles`.
   - `policy_activate` — appended on successful `PUT /v1/policy-bundles/{bundle_id}/active`.
   - `policy_rollback` — appended on successful `POST /v1/policy-bundles/{bundle_id}/rollback`.
   - `approval_resolve` — appended on successful `POST /v1/approvals/{id}/resolve`.
   - `execution_cancel` — appended on successful `POST /v1/executions/{id}/cancel`.
   - Append is **best-effort**; store errors are logged but do not fail the primary action.

6. **CLI support** (`bins/ferrumctl/src/main.rs`)  
   - `ferrumctl admin audit list` — lists recent audit entries with optional `--limit`.
   - Outputs table or JSON (`--json`).

---

## Verification summary

| Check | Command / Test | Result |
|-------|----------------|--------|
| Formatting | `cargo fmt --all -- --check` | ✅ PASS |
| Type check | `cargo check --workspace` | ✅ PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ PASS |
| Workspace tests | `cargo test --workspace` | ✅ PASS (all packages) |
| Store focused tests | `cargo test --package ferrum-store` | ✅ PASS (SQLite + Postgres) |
| Gateway focused tests | `cargo test --package ferrum-gateway` | ✅ PASS (scoped-auth + audit endpoint) |
| ferrumctl focused tests | `cargo test --package ferrumctl` | ✅ PASS (CLI parse + integration) |
| Postgres feature check | `cargo check --package ferrum-store --features postgres` | ✅ PASS |
| Postgres tests | `cargo test --package ferrum-store --features postgres` | ✅ PASS (72 tests) |

---

## Non-claims

- **Best-effort append**: If the audit store is unreachable or the write queue is saturated, the primary action still succeeds. Audit loss is possible under extreme store pressure.
- **Not a compliance-grade audit**: This is an operator accountability aid, not a tamper-proof forensic log. No cryptographic signing or WORM storage is implemented.
- **Not production-ready**: SEC-6 implementation does not change the overall `production-ready = NO` posture.
- **Block A remains WAIVED/CONDITIONAL**: Real owned domain still required for production-ready or full G2 closure.
- **No secrets in this artifact**: No tokens, passwords, or keys are recorded here.

---

*End of evidence — SEC-6 implementation artifact (2026-05-21).*
