# PG Production Evidence Pack Runbook

> **Status**: PLANNING ARTIFACT — runbook for operator execution. No live production PostgreSQL deployed. No evidence claimed.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-25
> **Parent**: [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md)
> **Template**: [`docs/implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md)
> **Scope**: [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md)

> **Operator review**: pending
> This is a planning artifact. It does **not** constitute evidence of a production deployment, target-host validation, or production readiness. Does **not** substitute for missing evidence.

---

## 1. Purpose

This runbook tells an operator **exactly how to capture evidence** for a production PostgreSQL deployment signoff. It pairs with [`TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md). Every section below contains:

- Concrete shell commands or API calls.
- Expected output format.
- Pass/fail criteria.
- Redaction rules for secrets.
- Evidence file naming convention.

> **Non-claim**: This is a runbook only. No production PostgreSQL instance has been deployed. No evidence has been captured. Execution is operator-dependent and deferred until a live production PostgreSQL environment exists.

---

## 2. Evidence naming convention

All evidence artifacts must be date-stamped and stored in `docs/implementation-path/artifacts/`:

| Topic | Filename pattern |
|-------|-----------------|
| Target deployment | `YYYY-MM-DD-pg-target-deployment-evidence.md` |
| TLS DSN validation | `YYYY-MM-DD-pg-tls-dsn-evidence.md` |
| Scheduled backup | `YYYY-MM-DD-pg-scheduled-backup-evidence.md` |
| Retention pruning | `YYYY-MM-DD-pg-retention-pruning-evidence.md` |
| Offsite sync | `YYYY-MM-DD-pg-offsite-sync-evidence.md` |
| Alert deployment | `YYYY-MM-DD-pg-alert-deployment-evidence.md` |
| PgBouncer validation | `YYYY-MM-DD-pg-pgbouncer-evidence.md` |
| Restore drill | `YYYY-MM-DD-pg-restore-drill-evidence.md` |
| Consolidated signoff | `YYYY-MM-DD-pg-production-deployment-signoff.md` (from template) |

**File contents rule**: Every evidence file must contain the exact command, the exact (redacted) output, a pass/fail verdict, and the operator initials who ran it.

---

## 3. Redaction rules

Before pasting any output into an evidence artifact, apply these redactions **in this exact order**:

1. **Passwords in DSNs**: Replace `postgres://user:PASSWORD@host` with `postgres://user:__REDACTED__@host`.
2. **Bearer tokens**: Replace `Authorization: Bearer <token>` with `Authorization: Bearer __REDACTED__`.
3. **IP addresses / hostnames**: If the operator prefers, replace public IPs with `<TARGET_HOST>` or `<PG_HOST>`. Document the mapping in a separate operator-only sheet (not in version control).
4. **Cloud credentials**: Replace any AWS/GCS access keys, service account JSON, or API keys with `__REDACTED__`.
5. **Email addresses in alert outputs**: Replace with `<OPERATOR_EMAIL>`.

> **Sanity check**: After redaction, `grep -i -E '(pass|secret|key|token)'` on the artifact should return only the word `__REDACTED__` or innocuous words like "pass/fail".

---

## 4. Prerequisites capture commands (P.1–P.12)

These map directly to the Prerequisites Checklist in [`TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md).

### P.1 — PostgreSQL target/staging provisioned and reachable

**Command**:
```bash
pg_isready -h <PG_HOST> -p <PG_PORT> -U <monitor_user>
psql "${FERRUMD_STORE_DSN}" -c "SELECT 1 AS pg_reachable;"
```

**Expected output**:
```
<PG_HOST>:<PG_PORT> - accepting connections
 pg_reachable
--------------
            1
```

**Pass criteria**:
- `pg_isready` exit code `0`.
- `psql` returns exactly one row with value `1`.

**Evidence to record**: Screenshot or copy-paste of both commands and outputs (redacted).

---

### P.2 — ferrumd starts with PostgreSQL DSN and stays up

**Command**:
```bash
# Start or restart ferrumd with the PG DSN
sudo systemctl restart ferrumd
sleep 5
systemctl is-active ferrumd
journalctl -u ferrumd --since "1 minute ago" --no-pager
```

**Expected output**:
```
active
... Server running on <BIND_ADDR>
... store=postgres
```

**Pass criteria**:
- `systemctl is-active` returns `active`.
- Journal shows `Server running` and no `store error` or panic within 60 s of start.

**Evidence to record**: Last 20 lines of journal output (redact any DSN that appears in logs).

---

### P.3 — `/v1/readyz/deep` reports `store: healthy`

**Command**:
```bash
curl -sf http://<BIND_ADDR>/v1/readyz/deep | jq .
```

**Expected output**:
```json
{
  "status": "healthy",
  "store": "healthy",
  ...
}
```

**Pass criteria**:
- HTTP status `200`.
- JSON field `"store"` equals `"healthy"`.

**Evidence to record**: Full `curl` command and JSON output.

---

### P.4 — `ferrum-migrate` completes with row count + hash match

**Command**:
```bash
# Run migration
ferrum-migrate --source <SQLITE_SNAPSHOT> --target "${FERRUMD_STORE_DSN}"
MIGRATE_EXIT=$?
echo "ferrum-migrate exit code: ${MIGRATE_EXIT}"

# Row count diff
for table in intents approvals policy_bundles audit_logs tokens schema_version; do
  echo "=== ${table} ==="
  sqlite3 <SQLITE_SNAPSHOT> "SELECT COUNT(*) FROM ${table};"
  psql "${FERRUMD_STORE_DSN}" -c "SELECT COUNT(*) FROM ${table};"
done

# Content hash (ordered key columns; adapt per table)
psql "${FERRUMD_STORE_DSN}" -c "
SELECT md5(string_agg(id::text, ',' ORDER BY id)) FROM intents;
"
sqlite3 <SQLITE_SNAPSHOT> "
SELECT hex(md5(group_concat(id, ',' ORDER BY id)))
FROM intents;
"
```

**Pass criteria**:
- `ferrum-migrate` exit code `0`.
- Row counts match per table (±0).
- Content hashes match per table (or `pg_dump --data-only` hashes match).

**Evidence to record**: `ferrum-migrate` stdout, row count table, hash strings.

---

### P.5 — Connection hardening configured (timeout, metrics)

**Command**:
```bash
# Verify config is present
grep -E 'pg_statement_timeout_ms|pg_idle_in_transaction_timeout_ms' /etc/ferrumgate/ferrumd.toml

# Verify metrics are emitted
curl -sf http://<BIND_ADDR>/v1/metrics | grep -E 'ferrumgate_store_pg_pool_max|ferrumgate_store_pg_acquire_timeouts_total'
```

**Expected output**:
```
pg_statement_timeout_ms = 5000
pg_idle_in_transaction_timeout_ms = 10000
ferrumgate_store_pg_pool_max 20
ferrumgate_store_pg_acquire_timeouts_total 0
```

**Pass criteria**:
- Config fields present with non-negative values.
- Metrics endpoint returns `ferrumgate_store_pg_pool_max` and `ferrumgate_store_pg_acquire_timeouts_total`.

**Evidence to record**: Config snippet and metrics lines.

---

### P.6 — TLS/SSL DSN validated (if required)

**Command**:
```bash
# Verify SSL is active on the connection
psql "${FERRUMD_STORE_DSN}" -c "SHOW ssl;"

# Verify certificate files exist with correct permissions (if sslmode=verify-*)
ls -l /etc/ferrumgate/certs/pg-ca.crt /etc/ferrumgate/certs/pg-client.crt 2>/dev/null
```

**Expected output**:
```
 ssl
-----
 on
```

**Pass criteria**:
- `SHOW ssl` returns `on` (or `off` only if operator explicitly accepts non-TLS).
- Certificate files are readable by `ferrumd` user and **not** world-readable (`chmod 600` for keys).

**Evidence to record**: `SHOW ssl` output and `ls -l` of cert files (redact paths if sensitive).

---

### P.7 — Scheduled backup executing and verified

**Command**:
```bash
# List latest backup
ls -1lt /var/backups/ferrumgate-postgres/*.dump | head -n 5

# Verify latest backup is listable
LATEST=$(ls -1t /var/backups/ferrumgate-postgres/*.dump | head -n 1)
pg_restore -l "${LATEST}" > /dev/null && echo "LISTABLE: PASS" || echo "LISTABLE: FAIL"

# Check age
find /var/backups/ferrumgate-postgres -name "*.dump" -type f -mmin -16 | wc -l
```

**Pass criteria**:
- At least one backup file exists.
- Latest backup is ≤ 15 minutes old (or operator-defined interval).
- `pg_restore -l` succeeds.

**Evidence to record**: `ls` output, `pg_restore` result, age check result.

---

### P.8 — Retention pruning executing and verified

**Command**:
```bash
# Count backups before and after retention window
BACKUP_DIR="/var/backups/ferrumgate-postgres"
echo "Total dumps: $(find ${BACKUP_DIR} -name '*.dump' | wc -l)"
echo "Dumps older than 4 days: $(find ${BACKUP_DIR} -name '*.dump' -mmin +$((15*4*24)) | wc -l)"

# If pruning is scripted, show the script and its last run log
cat /etc/cron.d/ferrumgate-postgres-backup 2>/dev/null || echo "No cron file found"
journalctl -u ferrumgate-postgres-backup --since "1 day ago" --no-pager 2>/dev/null | tail -n 10
```

**Pass criteria**:
- No dumps older than retention window remain (or count is stable / not growing unbounded).
- Pruning script exists and has executed within the last window.

**Evidence to record**: Counts, script content (redact any inline passwords), last log lines.

---

### P.9 — Offsite sync executing and verified

**Command**:
```bash
# List local latest
LOCAL_LATEST=$(ls -1t /var/backups/ferrumgate-postgres/*.dump | head -n 1)

# List offsite latest (adapt for gsutil, aws s3, or rsync target)
# Example for GCS:
gsutil ls -l gs://<BUCKET>/ferrumgate-postgres/ | sort -k2 | tail -n 1

# Hash comparison
LOCAL_HASH=$(sha256sum "${LOCAL_LATEST}" | awk '{print $1}')
# Download offsite copy to temp and hash
# ...
echo "Local hash:  ${LOCAL_HASH}"
echo "Offsite hash: ${OFFSITE_HASH}"
[ "${LOCAL_HASH}" = "${OFFSITE_HASH}" ] && echo "HASH MATCH: PASS" || echo "HASH MATCH: FAIL"
```

**Pass criteria**:
- Offsite copy exists.
- SHA-256 hashes match between local latest and offsite copy.
- Sync lag ≤ 1 hour (or operator-defined).

**Evidence to record**: Sync command, hash strings, pass/fail verdict.

---

### P.10 — Alert rules deployed to live Prometheus

**Command**:
```bash
# Syntax validation (local)
promtool check rules /path/to/ferrumgate-alerts.yaml

# Verify rules are loaded in Prometheus
# Replace <PROM_ADDR> with your Prometheus URL
curl -sf http://<PROM_ADDR>:9090/api/v1/rules | jq '.data.groups[] | select(.name == "ferrumgate_postgres")'

# Verify alert state for a PG rule
curl -sf "http://<PROM_ADDR>:9090/api/v1/alerts?active=true" | jq '.data[] | select(.labels.alertname | contains("Postgres"))'
```

**Pass criteria**:
- `promtool check rules` returns `SUCCESS`.
- Prometheus API lists the `ferrumgate_postgres` rule group.
- At least one PG alert is known to Prometheus (state may be `inactive`, but rule must exist).

**Evidence to record**: `promtool` output, Prometheus rule group JSON (redact server URLs if sensitive).

---

### P.11 — Restore drill passed on production-like data

**Command**:
```bash
# Use the restore drill script from 109-p5c runbook
# Ensure drill DB is created, restored, row counts verified, then dropped.
# See docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md §P5c.2
bash /path/to/restore_drill.sh
```

**Pass criteria**:
- `pg_restore` into drill DB exits `0`.
- All expected tables present.
- Row counts match source.
- `/v1/readyz/deep` against a temporary ferrumd pointed at drill DB returns `200` with `store: healthy`.
- Drill DB is dropped after verification.

**Evidence to record**: Restore log (`/tmp/pg_restore_drill_*.log`), readyz output, cleanup confirmation.

---

### P.12 — PgBouncer validated (if multi-instance)

**Command**:
```bashn# Verify PgBouncer is listening
pg_isready -h <PGBOUNCER_HOST> -p 6432 -U <pool_user>

# Verify ferrumd DSN points at PgBouncer
grep FERRUMD_STORE_DSN /etc/ferrumgate/ferrumd.env

# Verify pool stats via PgBouncer admin console
psql -h <PGBOUNCER_HOST> -p 6432 -U pgbouncer pgbouncer -c "SHOW pools;"
```

**Pass criteria**:
- `pg_isready` to PgBouncer returns accepting connections.
- ferrumd DSN hostname matches PgBouncer host (not PostgreSQL direct).
- `SHOW pools` shows active connections and no `sv_active` exhaustion.

**Evidence to record**: `pg_isready` output, DSN line (redacted password), `SHOW pools` output.

---

## 5. Deployment configuration capture

Record the exact runtime configuration at the moment of signoff. Do not rely on memory.

**Command**:
```bash
# Runtime config (from env or config file; redact secrets)
cat /etc/ferrumgate/ferrumd.toml | grep -E 'store|pg_|tls|bouncer'
env | grep -E '^FERRUMD_' | sort

# Binary version
ferrumd --version 2>/dev/null || echo "ferrumd version: $(ferrumd --help 2>&1 | head -n 1)"
```

**Evidence to record**: Config file excerpt (redacted), env var list (redacted), binary version.

---

## 6. Health and metrics capture commands

These map to the Health and Metrics table in the signoff template.

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Deep readiness | `curl -sf http://<BIND>/v1/readyz/deep \| jq .` | HTTP `200`, `store: healthy` |
| PG pool metrics | `curl -sf http://<BIND>/v1/metrics \| grep ferrumgate_store_pg_pool_` | `pool_max`, `pool_size`, `pool_idle` present |
| Acquire timeout metric | `curl -sf http://<BIND>/v1/metrics \| grep ferrumgate_store_pg_acquire_timeouts_total` | Metric present (value may be `0`) |
| Prometheus scrape | `curl -sf http://<PROM_ADDR>:9090/api/v1/targets \| jq '.data.activeTargets[] \| select(.labels.job == "ferrumgate")'` | Health `UP` |

**Evidence to record**: Full command + output for each row.

---

## 7. Backup discipline capture commands

These map to the Backup Discipline table in the signoff template.

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Last backup age | `find /var/backups/ferrumgate-postgres -name '*.dump' -mmin -16 \| wc -l` | Count ≥ 1 |
| Backup integrity | `pg_restore -l <LATEST> > /dev/null && echo PASS` | `PASS` |
| Retention pruning | `find /var/backups/ferrumgate-postgres -mmin +$((15*4*24)) \| wc -l` | Count stable / not growing |
| Offsite sync lag | Compare timestamp of latest local vs offsite file | ≤ 1 hour |
| Offsite hash match | `sha256sum` local vs offsite copy | `MATCH` |

**Evidence to record**: Command output for each row.

---

## 8. Rollback / restore checks

Before signing off, the operator must confirm that rollback is possible.

**Checklist**:

| # | Rollback check | Method | Pass criteria |
|---|----------------|--------|---------------|
| R.1 | Latest backup is restorable | Run `pg_restore` into a drill DB | Exit `0`, tables present, row counts match |
| R.2 | ferrumd can be pointed back at SQLite (if emergency revert) | Change `FERRUMD_STORE_DSN` to SQLite path, restart, probe `readyz/deep` | HTTP `200`, `store: healthy` (if SQLite file exists and is valid) |
| R.3 | Schema version is documented | `psql "${FERRUMD_STORE_DSN}" -c "SELECT version FROM _schema_version ORDER BY version DESC LIMIT 1;"` | Returns a version number matching the ferrumd binary expectation |
| R.4 | Operator knows the emergency stop command | `sudo systemctl stop ferrumd` | Command is documented and tested in non-prod |

> **Non-claim**: Rollback checks are procedural only. No live emergency rollback has been executed against production data.

---

## 9. Operator signoff boundaries

| Responsibility | Engineering | Operator |
|----------------|-------------|----------|
| Provision PostgreSQL instance | Advise, document | Execute |
| Configure ferrumd DSN and TLS | Document, provide binary | Execute, validate |
| Run `ferrum-migrate` | Provide binary, document | Execute, validate row counts |
| Configure scheduled backup / retention / offsite | Provide runbook, examples | Execute, monitor |
| Deploy alert rules to Prometheus | Provide template, `promtool` validation | Deploy, validate in live Prometheus |
| Execute restore drill | Provide script, document | Execute, record evidence |
| Validate PgBouncer (if used) | Document connection math | Deploy, validate |
| Sign off on production PG deployment | Review evidence, approve template | Final signoff on real evidence |

> **Rule**: Engineering may review and comment on operator evidence, but **only the operator may check the final signoff box** in [`TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md).

---

## 10. Non-claims

- **NOT a production-ready claim by itself**: PostgreSQL production deployment is a prerequisite for production-ready, not sufficient alone.
- **NOT HA**: This runbook covers single-node PostgreSQL only. HA is a separate evidence pack.
- **NOT validated for all configs**: TLS, PgBouncer, and alert validation are operator-environment-dependent.
- **NOT self-executing**: This runbook records commands only. Real execution and evidence creation are required.
- **NOT retroactive**: Signoff applies only to the specific PG version, ferrumd version, and environment listed.
- **Block A remains open unless separately closed**: PostgreSQL deployment does not close Block A (real domain).
- **No live evidence exists yet**: Every command in this runbook is a template. No output has been captured from a production PostgreSQL instance.

---

## 11. Related docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PostgreSQL hardening plan.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md) — Signoff template to fill after executing this runbook.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-target-deployment-evidence.md`](./TEMPLATE-pg-target-deployment-evidence.md) — Target deployment evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-tls-dsn-evidence.md`](./TEMPLATE-pg-tls-dsn-evidence.md) — TLS evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-scheduled-backup-evidence.md`](./TEMPLATE-pg-scheduled-backup-evidence.md) — Scheduled backup evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-retention-pruning-evidence.md`](./TEMPLATE-pg-retention-pruning-evidence.md) — Retention pruning evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-offsite-sync-evidence.md`](./TEMPLATE-pg-offsite-sync-evidence.md) — Offsite sync evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-alert-deployment-evidence.md`](./TEMPLATE-pg-alert-deployment-evidence.md) — Alert deployment evidence template.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-pgbouncer-evidence.md`](./TEMPLATE-pg-pgbouncer-evidence.md) — PgBouncer evidence template.
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) — Backup/restore runbook.
- [`docs/guides/operator.md`](../../guides/operator.md) — General operator guide.

---

*End of PG Production Evidence Pack Runbook — planning artifact only (2026-05-25).*
