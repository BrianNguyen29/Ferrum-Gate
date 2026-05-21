# 02 — PostgreSQL Production Plan

> **Status**: Planning artifact. Production PG not deployed.
> **Owner**: Engineering
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

> **Delegated signoff (planning-only)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: PostgreSQL production plan reviewed and accepted as engineering planning baseline.
> - **Nature**: Planning/decision document signoff only. This does not constitute evidence of production PostgreSQL deployment, target-host validation, or HA readiness. Does not substitute for missing evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

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
| ~~No PG-specific alert rules~~ | ~~Medium~~ | ~~Hard to operate without alerts~~ ✅ **CLOSED** — template rules added to `configs/monitoring/ferrumgate-alerts.yaml` (2026-05-21). Not deployed to live Prometheus. |
| ~~No TLS/SSL DSN guidance~~ | ~~Medium~~ | ~~Production PG connection not hardened~~ ✅ **CLOSED** — TLS/SSL DSN guidance documented in §PG-2.5 and `docs/guides/operator.md` (2026-05-21). Not deployed to live production PG. |
| No incremental up/down migration engine | Medium/High | Versioned runner exists; full incremental engine not built |
| No target-host PG drills | High | No evidence of production PG behavior |
| No PG restore drill evidence | High | Backup docs exist; evidence does not |
| No CI for postgres feature | Medium | Drift risk |
| ~~No PgBouncer/connection pooling story~~ | ~~Medium~~ | ~~Scaling is difficult~~ ✅ **CLOSED** — PgBouncer and pooling options documented in §PG-2.6 and `docs/guides/operator.md` (2026-05-21). No live deployment claimed. |
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

#### PG-2.4 — PostgreSQL alert rules template (COMPLETE)

- [x] Add `ferrumgate_postgres` alert group to `configs/monitoring/ferrumgate-alerts.yaml`.
- [x] `FerrumGatePostgresMetricsAbsent` — proxy for PG down / not emitting metrics (`absent(ferrumgate_store_pg_pool_max)`).
- [x] `FerrumGatePostgresPoolSaturation` — fires when `pool_idle == 0` and `pool_size >= pool_max`.
- [x] `FerrumGatePostgresSlowAcquire` — fires on `rate(acquire_timeouts_total[5m]) > 0`.
- [x] `FerrumGatePostgresBackupStale` — 2-hour threshold on generic backup timestamp metric.
- [x] `FerrumGatePostgresReplicationLag` — **placeholder / deferred**; uses fictional metric requiring `postgres_exporter`.
- [x] Document all alerts as **templates** in `configs/monitoring/README.md`.
- [x] Create evidence artifact: `docs/implementation-path/artifacts/2026-05-21-pg-alert-rules-evidence.md`.
- [x] Create alert deployment validation evidence template: `docs/implementation-path/artifacts/TEMPLATE-pg-alert-deployment-evidence.md`.
- [ ] Live Prometheus alert deployment validated — ☐ **PENDING** (requires operator environment).

> **Non-claim**: These are template rules only. They have **not** been validated against a live Prometheus instance or production PG backend. `promtool check rules` and live Prometheus evaluation are unavailable in this environment and remain operator/env-dependent. The `MetricsAbsent` alert is a heuristic, not definitive PG-down detection. Replication lag is a placeholder with a non-existent metric name. Operator must review thresholds, metric names, and validate with `promtool` before enabling.

#### PG-2.5 — TLS/SSL DSN guidance (RUNBOOK COMPLETE)

- [x] Document PostgreSQL TLS connection options for ferrumd.
- [x] Explain DSN parameters: `sslmode=require`, `sslmode=verify-ca`, `sslmode=verify-full`.
- [x] Document certificate file paths (`sslcert`, `sslkey`, `sslrootcert`) and permissions.
- [x] Provide example DSNs for common deployment patterns.
- [x] Cross-reference operator guide (`docs/guides/operator.md`) for TLS setup steps.
- [ ] Live TLS-encrypted PG connection validated on target host — ☐ **PENDING** (requires operator-provisioned PG with TLS).
- **Evidence template**: `docs/implementation-path/artifacts/TEMPLATE-pg-tls-dsn-evidence.md`

**DSN examples**:

```text
# Require TLS but do not verify server certificate (minimum for encrypted transport)
postgres://user:pass@host:5432/db?sslmode=require

# Verify server certificate against a CA bundle
postgres://user:pass@host:5432/db?sslmode=verify-ca&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt

# Verify server certificate and hostname (strongest)
postgres://user:pass@host:5432/db?sslmode=verify-full&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt

# Client certificate authentication (no password in DSN)
postgres://user@host:5432/db?sslmode=verify-full&sslcert=/etc/ferrumgate/certs/pg-client.crt&sslkey=/etc/ferrumgate/certs/pg-client.key&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt
```

**Operational notes**:
- `sqlx` passes TLS parameters through to the underlying `tokio-postgres` / `native-tls` or `rustls` stack.
- Client key files must be readable by the `ferrumd` process user (e.g., `ferrumgate:ferrumgate`) and **must not** be world-readable (`chmod 600`).
- In containerized deployments, mount certificates as secrets (Kubernetes Secret, Docker secret, or equivalent).
- Certificate rotation requires a `ferrumd` restart because the DSN and TLS config are loaded once at startup.

> **Non-claim**: TLS DSN guidance is documented as a runbook only. No live TLS-encrypted PostgreSQL connection has been validated. Operator must procure certificates and test connectivity independently.

#### PG-2.6 — PgBouncer / connection pooling story (RUNBOOK COMPLETE)

- [x] Document PgBouncer as an optional intermediary between ferrumd and PostgreSQL.
- [x] Explain when PgBouncer adds value vs. direct `sqlx::PgPool`.
- [x] Provide recommended `pool_mode` and session/transaction considerations.
- [x] Document connection count math (ferrumd pool max × ferrumd instances → PgBouncer pool size).
- [x] Cross-reference operator guide (`docs/guides/operator.md`) for setup steps.
- [ ] Live PgBouncer deployment validated — ☐ **PENDING** (requires operator environment).
- **Evidence template**: `docs/implementation-path/artifacts/TEMPLATE-pg-pgbouncer-evidence.md`

**When to use PgBouncer**:

| Scenario | Recommendation |
|----------|----------------|
| Single ferrumd instance, modest concurrency | Direct `sqlx::PgPool` is sufficient. PgBouncer optional. |
| Multiple ferrumd instances behind a load balancer | **PgBouncer recommended** — centralizes connection limit enforcement and prevents PG connection exhaustion. |
| Short-lived connections or high churn | **PgBouncer recommended** — `transaction` pooling mode reduces PG backend process count. |
| Prepared statements or session features required | Use `session` pool mode (or direct connections) because `transaction` mode does not preserve session state. |

**Recommended default for FerrumGate**:

- **Pool mode**: `transaction` (if no prepared statements or session features are used).
- **PgBouncer `max_client_conn`**: Sum of all ferrumd instance `pg_max_connections` + headroom (e.g., 20%).
- **PgBouncer `default_pool_size`**: PostgreSQL `max_connections` divided by number of PgBouncer instances minus overhead for admin/monitoring.
- **ferrumd DSN**: Point at PgBouncer instead of PostgreSQL directly:
  ```text
  postgres://user:pass@pgbouncer-host:6432/ferrumgate?sslmode=require
  ```

**Operational notes**:
- `sqlx::PgPool` already maintains an application-side connection pool. Adding PgBouncer creates a two-tier pool. Tune both layers to avoid over-provisioning.
- If PgBouncer is in `transaction` mode, `SET` commands (such as `statement_timeout` applied by ferrumd in `after_connect`) may behave differently. Test thoroughly.
- PgBouncer becomes a new single point of failure unless itself made HA (e.g., with HAProxy failover or multiple PgBouncer instances).

**Trigger for enabling PgBouncer**:
- More than 2 ferrumd instances connect to the same PostgreSQL.
- PostgreSQL `max_connections` is approached under normal load.
- Connection churn (frequent connect/disconnect) is observed in PG logs.

> **Non-claim**: PgBouncer guidance is documented as a runbook only. No live PgBouncer deployment has been validated with ferrumd. Operator must test in their environment before production use.

##### PG-2.3b — Reconnect/retry and circuit breaker (PARTIAL — B.1 docs complete; B.2–B.4 deferred)

> **Oracle verdict**: PG-2.3b code implementation is **deferred entirely**.
> No custom reconnect/retry policy or circuit breaker will be built for the
> single-node pilot.

**B.1 — Document `sqlx::PgPool` reconnect behavior** ✅ COMPLETE
- Operator runbook section added to `docs/guides/operator.md` §"PostgreSQL reconnect and recovery" (2026-05-21).
- Describes transparent reconnect on new acquisition, readiness degradation during outage, recovery checks (`readyz/deep`, metrics), and when restart is/is not required.
- Explicitly states no production-ready claim and no runtime recovery proof beyond local Docker.

**B.2 — Integration test: restart PG container → ferrumd recovers** ✅ SCRIPT PREPARED
- `scripts/run_pg_container_restart_drill.sh` created and locally validated on 2026-05-21.
- Recovery measured at 14s (target <= 30s).
- Script is manual/optional; not executed in CI.
- Evidence: `docs/implementation-path/artifacts/2026-05-21-pg-container-restart-drill-evidence.md`.

**B.3 — Circuit-breaker ADR for multi-node / load-balanced topology** ☐ **DEFERRED**

**Verdict**: DEFER now. No circuit breaker ADR will be written until HA design begins.

**Rationale**:
- PgPool reconnect + `/readyz/deep` pool saturation + acquire timeout metric + alert templates + operator runbook are sufficient for single-node fail-closed behavior.
- A circuit breaker adds value only with a load-balanced HA topology (PG-5); a premature state machine would add complexity and false positives.
- Revisit triggers: PG-5 HA design begins; load balancer introduced; RTO tightens beyond B.2 14 s recovery; real-world PG blips show failures a half-open state could mitigate.

**B.4 — Implement circuit breaker** ☐ **DEFERRED**

**Verdict**: DEFER until after HA ADR Phase 9 is approved and a multi-node/load-balanced topology is planned.

**Rationale**: Implementation depends on B.3 ADR. No code will be written before the ADR is approved.

**Combined rationale for PG-2.3b deferral**:
- `sqlx::PgPool` already performs transparent reconnect with exponential backoff.
  The pool recovers automatically after a PostgreSQL restart without requiring
  a ferrumd restart (local Docker drill measured 14 s recovery).
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
- Load balancer introduced in front of ferrumd or PostgreSQL.
- Real-world PostgreSQL blips show failure modes that a half-open state could mitigate.

### Phase PG-3 — Backup/restore evidence

#### PG-3 local backup/restore drill — COMPLETE (local Docker only)

- [x] Execute restore drill to clean DB — ✅ COMPLETE on 2026-05-18 (local Docker).
- [x] Verify row counts and content hashes — ✅ COMPLETE on 2026-05-18 (all counts matched; empty baseline).
- [x] Create evidence artifact — ✅ COMPLETE: `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md`.
- [x] Verify `/v1/readyz/deep` against restored DB — ✅ COMPLETE (HTTP 200, healthy true).

#### PG-3 scheduled backup/retention — RUNBOOK COMPLETE / EXECUTION PENDING

- [x] Document scheduled `pg_dump` procedure with cron and systemd timer examples — ✅ COMPLETE (see `docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md` §P5c.5).
- [x] Document retention pruning policy and examples — ✅ COMPLETE (see `docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md` §P5c.5).
- [x] Document offsite backup target considerations (GCS, S3, rsync) — ✅ COMPLETE (see below).
- [ ] Operator deploys and validates scheduled backup on live PostgreSQL — ☐ **PENDING**.
- [ ] Operator validates retention pruning on live backup target — ☐ **PENDING**.
- [ ] Operator validates offsite sync success and restore-from-offsite drill — ☐ **PENDING**.

**Offsite backup target considerations**:

| Target | Method | Pros | Cons |
|--------|--------|------|------|
| GCS / S3 | `gsutil rsync` or `aws s3 sync` after local `pg_dump` | Durable, geo-redundant, scalable | Requires service account / IAM credentials; egress cost |
| rsync / SFTP | `rsync -avz` to remote host | Simple, no cloud dependency | Remote host availability, bandwidth, retention management |
| Managed PG backup | Cloud provider automated backup (RDS, Cloud SQL) | Zero operator effort, point-in-time recovery | Vendor lock-in, cost, may not meet custom RPO |

**Recommended default for FerrumGate**:

1. **Local `pg_dump`** every 15 minutes (cron or systemd timer) to `/var/backups/ferrumgate-postgres/`.
2. **Retention pruning**: keep 4 days of local dumps (`find -mmin +$((15*4*24)) -delete`).
3. **Offsite sync**: hourly `rsync` or `gsutil rsync` of the latest dump to offsite storage.
4. **Restore drill**: monthly restore drill to a clean drill database; verify row counts and `/v1/readyz/deep`.

**Evidence required for execution completion**:
- `docs/implementation-path/artifacts/YYYY-MM-DD-pg-scheduled-backup-evidence.md` (use `TEMPLATE-pg-scheduled-backup-evidence.md`)
- `docs/implementation-path/artifacts/YYYY-MM-DD-pg-retention-pruning-evidence.md` (use `TEMPLATE-pg-retention-pruning-evidence.md`)
- `docs/implementation-path/artifacts/YYYY-MM-DD-pg-offsite-sync-evidence.md` (use `TEMPLATE-pg-offsite-sync-evidence.md`)

> **Non-claim**: PG-3 local drill evidence is complete. Scheduled backup, retention pruning, and offsite sync **runbooks** are complete (documented in `109-p5c-postgresql-backup-restore-runbook.md` and this section). **Execution on a live production PostgreSQL remains PENDING**. Do not cite this doc as evidence of production backup automation until operator-deployed evidence artifacts exist.

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
| PG-2.5 | TLS/SSL DSN guidance documented with examples and operator setup steps |
| PG-2.6 | PgBouncer/pooling options documented with recommendations, triggers, and caveats |
| PG-3 | `pg_dump exit 0`, `pg_restore exit 0`, restored row count matches, readiness after restore pass |
| PG-3.1 | Scheduled backup/retention/offsite runbook complete; execution pending operator environment |
| PG-4 | Migration can run twice safely; version recorded; parity/rollback strategy documented; CI drift check deferred |
| PG-5 | HA ADR approved; primary failure drill documented; RPO/RTO measured for manual failover |

## Evidence required

- `pg-target-deployment-evidence.md`
- `pg-restore-drill-evidence.md`
- `pg-migration-evidence.md`
- `pg-ha-adr.md`
- `docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md` — Phase B consolidated artifact (TLS, PgBouncer, scheduled backup, alert deployment)
- `pg-scheduled-backup-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-scheduled-backup-evidence.md`)
- `pg-retention-pruning-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-retention-pruning-evidence.md`)
- `pg-offsite-sync-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-offsite-sync-evidence.md`)
- `pg-tls-dsn-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-tls-dsn-evidence.md`)
- `pg-pgbouncer-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-pgbouncer-evidence.md`)
- `pg-alert-deployment-evidence.md` (operator-deployed; pending — use `TEMPLATE-pg-alert-deployment-evidence.md`)

## Non-claims

- **NOT production-ready**: PG hardening is a prerequisite, not a completion.
- **NOT HA**: HA design is ADR-only in this plan; implementation is later.
- **NOT validated on target**: Until PG-1 target drill is executed, claims are local-only.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.3, §4 Phase 1
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md)
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — SQLite vs PG scaling guidance
