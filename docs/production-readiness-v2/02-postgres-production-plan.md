# 02 — PostgreSQL Production Plan

> **Status**: Planning artifact. Production PG not deployed.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Transform PostgreSQL from "local feature-gated implementation" into a production-candidate backend with target-host deployment evidence, hardened connections, metrics, and backup/restore discipline.

## Current state

PostgreSQL local/runtime foundation is strong:

| Item | Status |
|------|--------|
| 9 repo implementations | ✅ Done |
| Embedded schema migration | ✅ Done |
| Feature-gated postgres build | ✅ Done |
| Runtime DSN switching | ✅ Done |
| Pool config cơ bản | ✅ Done |
| ferrum-migrate SQLite → PG | ✅ Done |
| Docker Compose PG local | ✅ Done |
| Config templates | ✅ Done |
| Integration tests local | ✅ Done |
| Backup/restore runbook | ✅ Done (docs) |
| Ops cadence docs | ✅ Done (docs) |

## Gaps

| Gap | Severity | Why |
|-----|----------|-----|
| No connection pool health/reconnect | High | DB restart may require ferrumd restart |
| No circuit breaker | High | DB failure can cascade to gateway |
| No statement timeout | Medium/High | Slow queries can hold connections indefinitely |
| No PG pool metrics | Medium | Cannot observe pool saturation |
| No PG-specific alert rules | Medium | Hard to operate without alerts |
| No TLS/SSL DSN guidance | Medium | Production PG connection not hardened |
| No incremental up/down migration engine | Medium/High | Versioned runner exists; full incremental engine not built |
| No target-host PG drills | High | No evidence of production PG behavior |
| No PG restore drill evidence | High | Backup docs exist; evidence does not |
| No CI for postgres feature | Medium | Drift risk |
| No PgBouncer/connection pooling story | Medium | Scaling is difficult |
| No HA/failover | Critical | No production HA |
| No replication configs | High | No standby/read replica |
| No failover runbook | High | No promote/reroute procedure |
| No split-brain prevention | High | HA claim impossible without this |

## Implementation tasks

### Phase PG-1 — PostgreSQL Target/Staging Baseline

**Objective**: Provision a PostgreSQL-backed target/staging environment, confirm ferrumd starts and reports healthy, migrate a SQLite snapshot, validate data integrity, and capture evidence without claiming production readiness.

**Prerequisites**:
- [x] PostgreSQL instance accessible from target/staging host (local Docker Compose fallback used on 2026-05-18).
- [x] Sanitized `FERRUMD_STORE_DSN` prepared (no secrets committed).
- [x] SQLite snapshot file available for `ferrum-migrate` source.
- [x] `ferrumd` binary built with `postgres` feature.

**Execution todo-list**:

| # | Task | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| PG-1.1 | Provision PostgreSQL target/staging instance and create application database/user. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.1 | ✅ COMPLETE — local Docker fallback |
| PG-1.2 | Start ferrumd with `FERRUMD_STORE_DSN=postgres://...` and confirm process stays up. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.2 | ✅ COMPLETE — local Docker fallback |
| PG-1.3 | Verify `/v1/readyz/deep` returns HTTP 200 with `store: healthy`. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.3 | ✅ COMPLETE — local Docker fallback |
| PG-1.4 | Run `ferrum-migrate` from SQLite snapshot to PostgreSQL target. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.4 | ✅ COMPLETE — local Docker fallback |
| PG-1.5 | Validate row counts match between SQLite source and PostgreSQL target. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.5 | ✅ COMPLETE — local Docker fallback |
| PG-1.6 | Validate content hash (e.g., `SHA-256` of ordered key columns or `pg_dump --data-only` hash) matches. | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.6 | ✅ COMPLETE — local Docker fallback |
| PG-1.7 | Create evidence artifact from template (`docs/implementation-path/artifacts/TEMPLATE-pg-target-deployment-evidence.md`). | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` | ✅ COMPLETE — local Docker fallback |
| PG-1.8 | Update tracking docs (`10-evidence-checklist.md`, `PRODUCTION_NOTES.md`) with PG-1 status. | Engineering | This doc + `10-evidence-checklist.md` + `PRODUCTION_NOTES.md` | ✅ COMPLETE — local Docker fallback |

**Acceptance gates**:

| Gate | Criteria | Verification Method |
|------|----------|---------------------|
| PG-1.1 | PostgreSQL target/staging provisioned and reachable from ferrumd host. | `psql "${FERRUMD_STORE_DSN}" -c "SELECT 1"` returns 1 row. |
| PG-1.2 | ferrumd starts with postgres DSN and does not panic or exit within 60s. | Process log shows `Server running` and no `store error` at startup. |
| PG-1.3 | `/v1/readyz/deep` returns HTTP 200 with `store: healthy`. | `curl -sf http://<bind>/v1/readyz/deep` exit 0 and JSON contains `"store": "healthy"`. |
| PG-1.4 | `ferrum-migrate` completes with exit code 0 and reports migrated tables. | Command stdout contains `Migration complete` or equivalent. |
| PG-1.5 | Row counts match per table between SQLite source and PostgreSQL target. | `SELECT COUNT(*)` diff shows zero differences for all tables. |
| PG-1.6 | Content hash validation passes (deterministic hash over ordered data). | Hash strings are identical for source and target. |
| PG-1.7 | Evidence artifact created from template, dated, and stored in `docs/implementation-path/artifacts/`. | File exists and follows template format. |
| PG-1.8 | Docs and checklists reference PG-1 as complete (evidence-linked), not as claimed. | `10-evidence-checklist.md` and `PRODUCTION_NOTES.md` updated. |

**Evidence required**:
- `docs/implementation-path/artifacts/YYYY-MM-DD-pg-target-deployment-evidence.md` (from `TEMPLATE-pg-target-deployment-evidence.md`).
- `pg-migration-evidence.md` (row counts + hash validation).
- `pg-readyz-evidence.md` (deep readiness output).

**Current PG-1 evidence**:
- `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` — ✅ local Docker staging fallback baseline PASS.
- Limitation: source SQLite snapshot was empty; target-host PostgreSQL, production data volume, backup/restore, PG hardening, and HA remain pending.

### Phase PG-2 — Connection hardening

#### PG-2.1 — Session timeout configuration (COMPLETE)

- [x] Add `statement_timeout` to PG connection config (default 5000 ms, `0` disables).
- [x] Add `idle_in_transaction_session_timeout` (default 10000 ms, `0` disables).
- [x] Apply both via `PgPoolOptions::after_connect` with session-level `SET` commands.
- [x] Add CLI flags (`--pg-statement-timeout-ms`, `--pg-idle-in-transaction-timeout-ms`).
- [x] Add env vars (`FERRUMD_PG_STATEMENT_TIMEOUT_MS`, `FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS`).
- [x] Add config file fields and validation (defaults documented; `0` = disabled).
- [x] Unit tests for defaults, precedence, and disabled behavior.

#### PG-2.2 — Pool metrics (COMPLETE)

- [x] Expose pool size, idle connections, and max connections via `StoreFacade::pool_status()`.
- [x] Emit `ferrumgate_store_pg_pool_size`, `ferrumgate_store_pg_pool_idle`, `ferrumgate_store_pg_pool_max` in `/v1/metrics`.
- [x] SQLite/non-PG stores return `None` and do not emit misleading PG metrics.
- [x] Unit tests for metrics presence (when `Some`) and omission (when `None`).

#### PG-2.3 — Acquire timeout metrics, pool saturation readiness, reconnect, circuit breaker

##### PG-2.3a — Acquire timeout counter + pool saturation readiness (COMPLETE)

- [x] Add `acquire_timeouts` counter to `PoolStatus` and `PostgresStore`.
- [x] Detect `sqlx::Error::PoolTimedOut` in `health_check()` and increment counter.
- [x] Emit `ferrumgate_store_pg_acquire_timeouts_total` in `/v1/metrics` when `pool_status` is `Some`.
- [x] Add pool saturation component to `/v1/readyz/deep`: reports degraded when `idle_connections == 0 && total_connections >= max_connections` for `max > 0`.
- [x] Tests for metric emission/omission and saturation behavior.

##### PG-2.3b — Reconnect/retry and circuit breaker (DEFERRED — docs-only rationale)

> **Oracle verdict**: PG-2.3b code implementation is **deferred entirely**.
> No custom reconnect/retry policy or circuit breaker will be built for the
> single-node pilot.

**Rationale**:
- `sqlx::PgPool` already performs transparent reconnect with exponential backoff.
  The pool recovers automatically after a PostgreSQL restart without requiring
  a ferrumd restart.
- PG-2.3a provides acquire-timeout metrics and pool-saturation readiness
  (`/v1/readyz/deep`), which gives fail-closed behavior when the pool is
  exhausted. This is sufficient for the current single-node pilot scope.
- A circuit breaker is only meaningful when there is a load-balancer or
  multi-node topology that can route around an unhealthy instance. That
  requirement belongs to PG-5 (HA/multi-node).

**Trigger for revisit**:
- PG-5 HA design (managed PG, Patroni, or load-balanced topology) where
  per-instance health state matters and concrete retry semantics / RTO need
  to be defined.

### Phase PG-3 — Backup/restore evidence

#### PG-3 local backup/restore drill — COMPLETE (local Docker only)

- [x] Execute restore drill to clean DB — ✅ COMPLETE on 2026-05-18 (local Docker).
- [x] Verify row counts and content hashes — ✅ COMPLETE on 2026-05-18 (all counts matched; empty baseline).
- [x] Create evidence artifact — ✅ COMPLETE: `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md`.
- [x] Verify `/v1/readyz/deep` against restored DB — ✅ COMPLETE (HTTP 200, healthy true).

#### PG-3 scheduled backup/retention — NOT STARTED / DEFERRED

- [ ] Implement scheduled `pg_dump` or WAL backup — ☐ NOT STARTED.
- [ ] Implement retention pruning — ☐ NOT STARTED.
- [ ] Offsite or production backup target validation — ☐ NOT STARTED.

> **Non-claim**: PG-3 local drill evidence is complete, but scheduled backup, retention pruning, and production backup targets remain NOT STARTED. Do not cite this artifact as evidence of production backup automation.

### Phase PG-4 — Schema migration discipline

#### PG-4a — Version table + idempotent runner (COMPLETE)

- [x] Add `_schema_version` table to both PG and SQLite.
- [x] Enhance `apply_embedded_migrations()` to check version before applying and skip when current >= target.
- [x] Handle bootstrap safely (create version table before querying).
- [x] Add tests for version tracking and idempotency.

#### PG-4b — Incremental migration files + CI drift check (DECOMPOSED)

> **Oracle design review conclusion**: PG-4b is decomposed. Do **not** implement a
> full incremental up/down engine or add `sqlx`/`refinery`-style dependencies.
> The bounded follow-up is docs + runner cleanup only.

##### PG-4b.1 — Migration parity decision + runner cleanup (COMPLETE)

- [x] Inspected PostgreSQL connect / bootstrap / `apply_embedded_migrations` path.
- [x] Confirmed the versioned runner (`apply_embedded_migrations`) is the **single**
  runtime path; no duplicate unversioned `bootstrap_schema` exists.
- [x] Documented that SQLite-only migrations are **not** ported to PostgreSQL:
  - `002_add_leader_tips.sql` → `leader_tips` (sync-only, SQLite)
  - `003_add_sync_state.sql` → `sync_state` (sync-only, SQLite)
  - `004_add_leader_allowlist.sql` → `leader_allowlist` (sync-only, SQLite)
  - `005_add_policy_bundles.sql` → `policy_bundles` **already present** in PG `001_initial.sql`
- [x] `CURRENT_SCHEMA_VERSION` remains `1` for PostgreSQL.
- [x] Added module-level docs in `crates/ferrum-store/src/postgres/migrations.rs` and
  doc comments in `mod.rs` recording the parity matrix.

##### PG-4b.2 — Incremental up/down engine + per-version SQL files (DEFERRED)

- [ ] Add per-migration up/down SQL files for future PG schema changes.
- [ ] Extend the runner to loop over versioned files when `CURRENT_SCHEMA_VERSION > 1`.
- [ ] Deferred until a real schema change requires it; no overengineering now.

##### PG-4b.3 — Rollback / forward strategy doc (COMPLETE)

- [x] Documented strategy in this plan (see below).
- [ ] CI schema-drift check remains **deferred** (no CI pipeline for PG feature).

##### PostgreSQL rollback / forward strategy

> **Scope**: This strategy applies to the embedded migration runner and the
> `migrations/postgres/` SQL files. It is **not** a general production DB ops
> runbook (see `109-p5c-postgresql-backup-restore-runbook.md` for backups).

1. **Forward-only by default**
   The runner only applies migrations when `current_version < target_version`.
   There is no automatic down-migration. If a deployment needs to revert schema,
   restore from a `pg_dump` snapshot taken before the migration.

2. **Idempotent baseline**
   `apply_embedded_migrations` creates `_schema_version` with `IF NOT EXISTS`
   and skips when the recorded version is already current. Re-running the same
   version is always safe.

3. **Adding a future migration**
   When a new schema change is required:
   - Add `migrations/postgres/002_<name>.sql` containing the new DDL.
   - Append the same DDL to `INIT_MIGRATION` (or refactor the runner to loop
     over files) so fresh databases receive the full schema in one transaction.
   - Increment `CURRENT_SCHEMA_VERSION` in `migrations.rs`.
   - The runner will then apply `002_*.sql` on the next startup.
   - Keep the migration within a single `BEGIN ... COMMIT` block if possible;
     the runner wraps everything in its own transaction, so individual files
     should avoid explicit `BEGIN`/`COMMIT`.

4. **Rollback procedure (manual)**
   - Stop `ferrumd`.
   - Restore the database from the most recent `pg_dump` (or snapshot) that
     predates the bad migration.
   - If the migration was partially applied, manually `DROP` any objects it
     created and delete its row from `_schema_version`.
   - Restart `ferrumd`; the runner will re-apply the correct baseline.

5. **SQLite-only tables stay SQLite-only**
   `leader_tips`, `sync_state`, and `leader_allowlist` are part of the SQLite
   sync subsystem and will not be added to PostgreSQL unless the sync design
   is later extended to PG. This is an intentional parity boundary, not a gap.

### Phase PG-5 — HA design (ADR first)

- [ ] Write HA ADR comparing managed PG, Patroni, repmgr, manual failover, read replicas.
- [ ] Select strategy: managed PG or manual failover runbook first.
- [ ] Document read replica plan for later.
- [ ] Defer automated failover until tenant/security model is stable.

## Acceptance criteria

| Gate | Criteria |
|------|----------|
| PG-1 | `FERRUMD_STORE_DSN=postgres://...` and `/v1/readyz/deep = 200` |
| PG-2 | Restart PG during test → ferrumd recovers or fails closed cleanly |
| PG-3 | `pg_dump exit 0`, `pg_restore exit 0`, restored row count matches, readiness after restore pass |
| PG-4 | Migration can run twice safely; version recorded; parity/rollback strategy documented; CI drift check deferred |
| PG-5 | HA ADR approved; primary failure drill documented; RPO/RTO measured for manual failover |

## Evidence required

- `pg-target-deployment-evidence.md`
- `pg-restore-drill-evidence.md`
- `pg-migration-evidence.md`
- `pg-ha-adr.md`

## Non-claims

- **NOT production-ready**: PG hardening is a prerequisite, not a completion.
- **NOT HA**: HA design is ADR-only in this plan; implementation is later.
- **NOT validated on target**: Until PG-1 target drill is executed, claims are local-only.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3, §4 Phase 1
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md)
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — SQLite vs PG scaling guidance
