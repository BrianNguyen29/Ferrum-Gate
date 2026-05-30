# Zero-Downtime Upgrade Guide

> **Status**: Guide added 2026-05-30.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md) §3.10 Phase 8 P2; [`docs/guides/hosted-deployment.md`](./hosted-deployment.md)
> **Scope**: Bounded to documentation of upgrade patterns and tradeoffs. Does not change code, contracts, or runtime behavior.

---

## When zero-downtime is possible vs maintenance window required

True zero-downtime upgrades (no dropped requests, no connection errors) require:

- Multiple ferrumd instances behind a load balancer
- Healthcheck-gated rolling restart
- PostgreSQL backend (shared state survives single-node restarts)
- HA-4 automated failover already operational

If any of the above are not in place, a **maintenance window** is required. State the expected interruption class honestly — a brief SQLite restart is not the same as a multi-host HA upgrade.

---

## Upgrade mode matrix

| Mode | Zero-downtime possible? | Interruption class | Notes |
|------|------------------------|--------------------|-------|
| SQLite / single-node | **No** | Brief outage (~5–15s) | Maintenance window required. WAL commit is atomic but the process restarts. |
| PostgreSQL / single-node | **Conditional** | Brief outage (~5–15s) if no LB; near-zero if drain-and-restart is used | HA-4 not complete; no automated failover. Manual operator-controlled restart needed. |
| PostgreSQL / multi-instance behind LB | **Yes (near-zero)** | One instance out at a time; LB routes around it | Requires ≥2 ferrumd instances, healthcheck endpoint, and operator-controlled drain/restart sequence. |
| HA-4 automated unattended | **Yes (true)** | No operator action needed | **NOT COMPLETE** — HA-4 is deferred. Do not claim this is available. |

> **HA-4 caveat**: Automated unattended failover is NOT COMPLETE. All multi-instance upgrade procedures below require an operator to manually orchestrate drain-and-restart. Until HA-4 is implemented, there is no truly zero-touch zero-downtime path.

---

## Pre-upgrade checklist

Regardless of mode, complete before any binary or config upgrade:

- [ ] Read the current [`docs/ROADMAP.md`](../../ROADMAP.md) §3.10 and confirm upgrade is within scope of current deployment mode
- [ ] Review [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) for current operational baseline
- [ ] Capture a fresh backup (see [`docs/guides/operator.md`](./operator.md) §Backup and restore)
- [ ] For PostgreSQL: run `pg_dump` to a timestamped file
- [ ] Verify backup is listable/restorable before proceeding
- [ ] Notify operators if a maintenance window is needed
- [ ] **Do not perform upgrades during an active SLO observation window** — see §Active SLO window warning below
- [ ] Ensure you have rollback instructions (see §Rollback procedure)

---

## systemd binary upgrade — single-node (SQLite or PostgreSQL)

> **Limitation**: This pattern reduces interruption but does not achieve true zero-downtime on single-node. A brief outage (~5–15s) occurs while the process stops and restarts. For SQLite, this is unavoidable. For PostgreSQL/single-node, the interruption is brief but still requires a maintenance window.

### Procedure

```bash
# 1. Capture backup
# SQLite:
ferrumctl backup create --db-path /var/lib/ferrumgate/ferrumgate.db --output-dir /var/backups/ferrumgate

# PostgreSQL:
sudo -u postgres pg_dump -Fc ferrumgate \
  -f /var/backups/ferrumgate-postgres/ferrumgate-$(date +%Y%m%d-%H%M%S).dump

# 2. Verify backup
pg_restore -l /var/backups/ferrumgate-postgres/ferrumgate-*.dump > /dev/null \
  && echo "LISTABLE=PASS"

# 3. Stop the service (brief outage begins)
sudo systemctl stop ferrumgate

# 4. Replace the binary
sudo cp /opt/ferrumgate/ferrumd /opt/ferrumgate/ferrumd.old
sudo cp /path/to/new/ferrumd /opt/ferrumgate/ferrumd

# 5. Start the service
sudo systemctl start ferrumgate

# 6. Wait for readiness
sleep 3

# 7. Verify smoke checks (see §Post-upgrade smoke checklist)
```

> **Note**: The binary replacement is not atomic. The stop/start sequence causes a brief gap in availability. This is expected for single-node deployments.

### Drain-and-restart (PostgreSQL/single-node, reduces but does not eliminate gap)

If running PostgreSQL and willing to accept a manual operator sequence:

```bash
# 1. Backup (as above)

# 2. Verify current readiness
curl -s http://localhost:8080/v1/readyz/deep | jq .
# Confirm status is "ok"

# 3. Stop
sudo systemctl stop ferrumgate

# 4. Replace binary

# 5. Start
sudo systemctl start ferrumgate

# 6. Verify
curl -s http://localhost:8080/v1/readyz/deep | jq .
```

---

## Docker Compose upgrade pattern with healthcheck gating

For Docker Compose deployments (both SQLite and PostgreSQL):

```bash
# 1. Capture backup
# SQLite:
docker compose exec ferrumd ferrumctl backup create \
  --db-path /var/lib/ferrumgate/ferrumgate.db --output-dir /backups

# PostgreSQL:
docker compose exec postgres pg_dump -Fc ferrumgate \
  -f /backups/ferrumgate-postgres-$(date +%Y%m%d-%H%M%S).dump

# 2. Pull new image
docker compose pull

# 3. Stop
docker compose stop ferrumd

# 4. Recreate with new image
docker compose up -d ferrumd

# 5. Wait for healthcheck to pass
# Healthcheck is defined in docker-compose.yml as:
# healthcheck:
#   test: ["CMD", "curl", "-f", "http://localhost:8080/v1/readyz"]
#   interval: 10s
#   timeout: 5s
#   retries: 5

# 6. Verify
docker compose exec ferrumd curl -s http://localhost:8080/v1/readyz/deep | jq .
```

> **Healthcheck gate**: The `healthcheck` in docker-compose.yml ensures the container is not marked healthy until `/v1/readyz` returns 200. Do not route traffic to this instance until the healthcheck passes.

---

## PostgreSQL schema migration discipline

FerrumGate uses embedded schema migration. Observe the following discipline:

### Forward-only principle

- Migrations are **forward-only**. Down migrations are not implemented.
- Each migration must be idempotent (safe to run twice).
- The migration version is recorded in the `schema_migrations` table.

### Before any migration

1. Capture a `pg_dump -Fc` backup (see §Pre-upgrade checklist).
2. Verify the backup is listable: `pg_restore -l <dumpfile> > /dev/null && echo OK`.
3. Test restore to a drill database (do not overwrite production):
   ```bash
   sudo -u postgres pg_restore -d ferrumgate_restore_drill <dumpfile>
   ```
4. Verify row counts match the production database.

### Rollback via restore if migration fails

If a migration step fails or the new binary behaves incorrectly:

1. **Do not attempt a manual down-migration** — none exists.
2. Restore from the pre-upgrade `pg_dump` backup:
   ```bash
   sudo systemctl stop ferrumgate
   sudo -u postgres pg_restore -d ferrumgate <dumpfile>
   sudo systemctl start ferrumgate
   ```
3. Verify `/v1/readyz/deep` returns 200.
4. Do not re-attempt the upgrade until the failure is understood.

> **Forward-only caveat**: If the new schema is incompatible with the old binary, you must restore the backup AND use the old binary. Do not assume the old binary works with a partially-migrated schema.

### Schema migration and HA-4

HA-4 automated failover is **NOT COMPLETE**. During a rolling upgrade with multiple ferrumd instances, each instance must run the same migration before receiving traffic. This requires manual sequencing:

1. Drain instance 1 (remove from LB pool or stop).
2. Run migration (if ferrumd does it automatically on startup, the first instance to start will apply it).
3. Restart instance 1.
4. Wait for `/v1/readyz/deep` to pass.
5. Put instance 1 back in service.
6. Repeat for instance 2.

Until HA-4 is complete, this is an operator-manual process.

---

## SQLite maintenance-window upgrade procedure

SQLite does not support concurrent writers and the ferrumd process must restart to load a new binary. True zero-downtime is not possible.

### Procedure

```bash
# 1. Notify operators / schedule maintenance window

# 2. Capture backup
ferrumctl backup create --db-path /var/lib/ferrumgate/ferrumgate.db --output-dir /var/backups/ferrumgate

# 3. Verify backup
ferrumctl backup verify --db-path /var/backups/ferrumgate-YYYYMMDD-HHMMSS.db

# 4. Stop ferrumd
sudo systemctl stop ferrumgate

# 5. Replace binary
sudo cp /opt/ferrumgate/ferrumd /opt/ferrumgate/ferrumd.old
sudo cp /path/to/new/ferrumd /opt/ferrumgate/ferrumd

# 6. Start ferrumd
sudo systemctl start ferrumgate

# 7. Wait for startup
sleep 3

# 8. Verify — see §Post-upgrade smoke checklist
```

Expected interruption: ~5–15s.

---

## Config-only changes

FerrumGate config precedence is:

```
CLI args > env vars > config file > defaults
```

### What requires restart

- Any change to `FERRUMD_CONFIG` file path or contents (config file is read once at startup)
- `FERRUMD_STORE_DSN` (pool is initialized at startup)
- `FERRUMD_AUTH_MODE` (auth layer is initialized at startup)
- `FERRUMD_BEARER_TOKEN` (read at startup; token rotation requires restart)
- `FERRUMD_BIND_ADDR` (TCP listener is bound at startup)
- `FERRUMD_PG_MAX_CONNECTIONS`, `FERRUMD_PG_MIN_IDLE`, `FERRUMD_PG_ACQUIRE_TIMEOUT_SECS` (pool params read at startup)

### What does NOT require restart

> **Note**: FerrumGate does not currently implement a config reload mechanism. Do not assume any hot-reload capability that is not documented here. If you change an env var or config value, you must restart the process for it to take effect.

- `FERRUMD_LOG_FILTER` — logged but not hot-reloaded; requires restart
- `FERRUMD_LOG_FORMAT` — requires restart
- `FERRUMD_RATE_LIMIT_PER_SECOND`, `FERRUMD_RATE_LIMIT_BURST` — rate limit is checked per-request with current config values; no reload signal exists today

### Config-only upgrade (no binary change)

If only the config file changed:

```bash
# 1. Backup current config
sudo cp /etc/ferrumgate/ferrumd.toml /etc/ferrumgate/ferrumd.toml.backup-$(date +%Y%m%d-%H%M%S)

# 2. Update config file

# 3. Restart
sudo systemctl restart ferrumgate

# 4. Verify smoke checks
```

---

## Post-upgrade smoke checklist

Run after every upgrade (binary or config):

```bash
# 1. Liveness
curl -s http://localhost:8080/v1/healthz
# Expected: {"status":"ok"} HTTP 200

# 2. Readiness
curl -s http://localhost:8080/v1/readyz
# Expected: {"status":"ready"} HTTP 200

# 3. Deep readiness (store, write_queue, pool)
curl -s http://localhost:8080/v1/readyz/deep
# Expected: HTTP 200, all checks "ok"

# 4. If metrics are configured:
curl -s http://localhost:8080/v1/metrics | grep -E "ferrumgate_store_health_up|ferrumgate_write_queue_depth"
# Expected: store_health_up=1, write_queue_depth within normal threshold

# 5. For PostgreSQL:
# - pool idle > 0:  ferrumgate_store_pg_pool_idle > 0
# - no acquire timeout spikes: ferrumgate_store_pg_acquire_timeouts_total not incrementing
```

If any smoke check fails, see §Rollback procedure.

---

## Rollback procedure

### Binary rollback

```bash
# 1. Stop
sudo systemctl stop ferrumgate

# 2. Restore old binary
sudo cp /opt/ferrumgate/ferrumd.old /opt/ferrumgate/ferrumd

# 3. Start
sudo systemctl start ferrumgate

# 4. Verify
curl -s http://localhost:8080/v1/readyz/deep | jq .
```

### Config rollback

```bash
# 1. Stop
sudo systemctl stop ferrumgate

# 2. Restore config
sudo cp /etc/ferrumgate/ferrumd.toml.backup-YYYYMMDD-HHMMSS /etc/ferrumgate/ferrumd.toml

# 3. Start
sudo systemctl start ferrumgate

# 4. Verify
```

### Database restore rollback

> **Warning**: Database restore overwrites the current store. This is a last resort when both binary rollback and config rollback fail to restore readiness.

```bash
# 1. Stop
sudo systemctl stop ferrumgate

# 2. Restore from pre-upgrade backup
# SQLite:
sudo cp /var/lib/ferrumgate/ferrumgate.db /var/lib/ferrumgate/ferrumgate.db.failed-upgrade
ferrumctl backup restore --db-path /var/lib/ferrumgate/ferrumgate.db \
  --from /var/backups/ferrumgate-YYYYMMDD-HHMMSS.db --confirm

# PostgreSQL:
sudo -u postgres pg_restore -d ferrumgate /var/backups/ferrumgate-postgres/ferrumgate-YYYYMMDD-HHMMSS.dump

# 3. Start
sudo systemctl start ferrumgate

# 4. Verify deep readiness
curl -s http://localhost:8080/v1/readyz/deep | jq .

# 5. For PostgreSQL: verify row counts match expected values
```

> **DB restore caveat**: Restoring a PostgreSQL backup may lose writes that occurred between the backup and the failed upgrade. The RPO is determined by your backup frequency. Test restores against a drill database before production.

---

## Active SLO window warning

> **WARNING — Do not upgrade during an active SLO observation window.**

If an SLO monitoring window is actively running on a deployment target:

- **Do not** perform binary upgrades, config changes, or restarts unless you are intentionally invalidating or annotating the run.
- Upgrades and restarts cause brief outages that will invalidate availability SLO measurements.
- If an upgrade is required during an active window, you must:
  1. Annotate the SLO tool with the intentional maintenance window.
  2. Document the annotation in the evidence artifact.
  3. Resume the window after the upgrade is verified.

This applies to all deployment modes. The active SLO target is operator-defined; FerrumGate does not have a built-in mechanism to detect or protect an active SLO window.

---

## Non-claims

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** |
| **full G2** | **NOT COMPLETE** |
| **Tier 2** | **NOT COMPLETE** |
| **HA-4 unattended automated failover** | **NOT COMPLETE** |
| **sustained SLO window** | **NO** |
| **True zero-downtime upgrade** | **Only possible with PostgreSQL + multi-instance + LB + operator-manual drain sequence; HA-4 not complete** |
| **SQLite zero-downtime** | **Not possible — maintenance window required** |
| **Hot config reload** | **Not implemented — restart required for any config change** |
| **Multi-host production HA** | **NO** |

---

## Related docs

- [`docs/guides/hosted-deployment.md`](./hosted-deployment.md) — Deployment modes, systemd, Docker Compose, PostgreSQL.
- [`docs/guides/operator.md`](./operator.md) — Config, health checks, backup/restore, incident response.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../production-readiness-v2/02-postgres-production-plan.md) — PostgreSQL hardening and migration discipline.
- [`docs/production-readiness-v2/08-hosted-deployment-plan.md`](../production-readiness-v2/08-hosted-deployment-plan.md) — Hosted deployment plan (zero-downtime upgrade guide P2 item checked).
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../production-readiness-v2/09-ha-roadmap.md) — HA roadmap; HA-4 automated failover NOT COMPLETE.
- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.10 Phase 8 P2 — zero-downtime upgrade guide deliverable.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime configuration baseline.
