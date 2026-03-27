# Runbook: SQLite Backup, Restore, and Capacity Planning

## Context

`ferrumd` persists all state to a SQLite database configured via `store.dsn` in the config file or `--store-dsn` / `FERRUMD_STORE_DSN` env var. The repo default dev config uses `sqlite://ferrumgate.dev.db`. The repo production config (`configs/ferrumgate.prod.toml`) uses `sqlite://ferrumgate.prod.db`.

SQLite persistence includes:
- Intent envelopes
- Proposals and decisions
- Execution records and state transitions
- Provenance events and edges
- Capability leases
- Rollback contracts

There is **no multi-process SQLite access** today; ferrumd is single-process. Backup and restore are straightforward file-level operations.

## Runtime assumptions

| Item | Value |
|------|-------|
| ferrumd bind | `0.0.0.0:8080` (production) or `127.0.0.1:8080` (development) |
| ferrumd auth | `bearer` mode |
| Default dev DB path | `sqlite://ferrumgate.dev.db` (repo root) |
| Default prod DB path | `sqlite://ferrumgate.prod.db` (repo root, configurable) |
| SQLite file extension | `.db` |

These are documented in [15-deployment-and-operations.md](../15-deployment-and-operations.md).

## Prerequisites

- `sqlite3` CLI (version 3.x) for backup/restore operations
- Ferrumd process running with a persistent SQLite store (not `sqlite::memory:?cache=shared`)
- Verify store is persistent before backing up:
  ```sh
  cargo run -p ferrumd -- --print-effective-config | grep store_dsn
  # Expected output (non-memory):
  # store_dsn = "sqlite://ferrumgate.prod.db"   (source: file)
  #
  # If you see "memory" in the DSN, backup is not needed (ephemeral state).
  ```

---

## Backup

### Online backup with ferrumd running (preferred)

Use SQLite's online `.backup` command via the `sqlite3` CLI while ferrumd is running. SQLite's writers never block readers, and the backup is a consistent snapshot.

```sh
# Determine the actual DB file path from the resolved DSN
# For file-based DSNs like sqlite://ferrumgate.prod.db,
# the actual file is ferrumgate.prod.db in the process working directory.

# For a production deployment running from /opt/ferrumgate:
DB_FILE="/opt/ferrumgate/ferrumgate.prod.db"
BACKUP_FILE="/opt/ferrumgate/backups/ferrumgate-$(date +%Y%m%d-%H%M%S).db"

# Create the backup directory if it does not exist
mkdir -p "$(dirname "$BACKUP_FILE")"

# Online backup via sqlite3 CLI (ferrumd remains running and serving)
sqlite3 "$DB_FILE" ".backup '$BACKUP_FILE'"

# Verify the backup is valid
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;"

# Expected output: ok
```

### Offline backup (ferrumd stopped)

If you prefer to stop ferrumd before backing up:

```sh
# 1. Stop ferrumd
sudo systemctl stop ferrumd   # or kill the process

# 2. Copy the DB file
DB_FILE="/opt/ferrumgate/ferrumgate.prod.db"
BACKUP_FILE="/opt/ferrumgate/backups/ferrumgate-$(date +%Y%m%d-%H%M%S).db"
cp "$DB_FILE" "$BACKUP_FILE"

# 3. Restart ferrumd
sudo systemctl start ferrumd

# 4. Verify the backup
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;"
```

### Automated nightly backup (cron)

```sh
# /etc/cron.d/ferrumgate-backup
# Run daily at 03:00 UTC
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin

0 3 * * * root \
  DB_FILE="/opt/ferrumgate/ferrumgate.prod.db" && \
  BACKUP_DIR="/opt/ferrumgate/backups" && \
  mkdir -p "$BACKUP_DIR" && \
  BACKUP_FILE="$BACKUP_DIR/ferrumgate-$(date +\%Y\%m\%d-\%H\%M\%S).db" && \
  sqlite3 "$DB_FILE" ".backup '$BACKUP_FILE'" && \
  sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" && \
  find "$BACKUP_DIR" -name "ferrumgate-*.db" -mtime +7 -delete
```

This cron job:
- Backs up daily at 03:00 UTC
- Verifies each backup with `PRAGMA integrity_check`
- Deletes backups older than 7 days

Adjust `mtime +7` and the schedule to match your RPO/RTO requirements.

---

## Restore

### Single backup restore

To restore from a specific backup file:

```sh
# 1. Stop ferrumd
sudo systemctl stop ferrumd

# 2. Identify the backup to restore
BACKUP_FILE="/opt/ferrumgate/backups/ferrumgate-20260327-030000.db"

# 3. Verify the backup before restoring
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;"
# Expected: ok

# 4. Replace the current DB file with the backup
DB_FILE="/opt/ferrumgate/ferrumgate.prod.db"
cp "$BACKUP_FILE" "$DB_FILE"

# 5. Restart ferrumd
sudo systemctl start ferrumd

# 6. Verify ferrumd is ready
curl -s http://127.0.0.1:8080/v1/readyz
# Expected: {"status":"ready"}
```

### Restore caution for sidecar SQLite files (advanced)

Depending on SQLite runtime settings, sidecar files such as `-wal` and `-shm`
may exist next to the main database file. If they exist, treat them as part of
the database state when diagnosing restore issues.

```sh
# 1. Stop ferrumd
sudo systemctl stop ferrumd

# 2. Check for WAL and SHM files
DB_FILE="/opt/ferrumgate/ferrumgate.prod.db"
ls -la "$DB_FILE"*.db*  # lists .db, .db-wal, .db-shm

# 3. For FerrumGate single-node operations, prefer restoring from a known-good
#    `.backup` snapshot. Point-in-time recovery is not a documented operator
#    path in this repo today.

# 4. For a clean slate (destroys all current state):
rm -f "$DB_FILE" "$DB_FILE-wal" "$DB_FILE-shm"
# Then either:
#   - Restore from a known-good .backup file (see Single backup restore)
#   - Let ferrumd recreate an empty DB on next start (state loss)
```

---

## Capacity Planning

### Database growth estimates

SQLite database growth is driven by:

| Data type | Growth rate | Notes |
|-----------|-------------|-------|
| Provenance events | ~200-500 bytes/event | Core audit trail; grows with execution volume |
| Execution records | ~1-2 KB/execution | Includes state transitions and metadata |
| Proposals | ~500 bytes/proposal | One per intent compile |
| Intents | ~500 bytes/intent | Intent envelopes are stored |
| Capability leases | ~200 bytes/lease | Short-lived; cleaned up after consume/expiry |
| Rollback contracts | ~1-5 KB/contract | Size depends on adapter metadata |

**Typical growth for a moderate deployment:**

- 1000 executions/day: ~1-2 MB/day in provenance events
- 10,000 executions/day: ~10-20 MB/day
- Monthly growth: ~30-600 MB per 1000 executions/day

A baseline database for a pilot deployment (~100 executions/day) is typically under 50 MB after 30 days.

### Monitoring DB size

```sh
# Check current DB file size
ls -lh /opt/ferrumgate/ferrumgate.prod.db
# -rw-r--r-- 1 ferrumgate ferrumgate 128M Mar 27 10:00 ferrumgate.prod.db

# Check number of rows in key tables
sqlite3 /opt/ferrumgate/ferrumgate.prod.db \
  "SELECT 'executions: ' || COUNT(*) FROM execution_records UNION ALL
   SELECT 'provenance_events: ' || COUNT(*) FROM provenance_events UNION ALL
   SELECT 'capabilities: ' || COUNT(*) FROM capability_leases;"

# Expected output (example):
# executions: 15234
# provenance_events: 98234
# capabilities: 8934
```

### Concurrency and connection planning

FerrumGate currently supports a single-process SQLite deployment. The main
capacity concern is sustained write pressure, not operator tuning of an exposed
`max_connections` setting.

- Recommended: keep `ferrumd` single-process and avoid concurrent external
  writers against the same database file.
- If you see `SQLITE_BUSY` under high write load, treat it as a workload/
  backend limit signal rather than something to solve with undocumented DSN
  tuning.
- `configs/ferrumgate.prod.toml` currently uses `sqlite://ferrumgate.prod.db`.

**High-volume deployments (>10,000 executions/day):**

If write throughput becomes a bottleneck, consider:
1. Batching execution requests to reduce write frequency
2. Offloading read queries to a read replica (future P2 work)
3. Migration to a different backend (future P2/HA work)

For single-node v1, SQLite is the only supported backend.

### Disk I/O considerations

| Scenario | Recommendation |
|----------|----------------|
| HDD storage | Avoid; use SSD for production |
| Network storage (NFS) | Not recommended for SQLite; use local filesystem |
| Docker volume | Use a named volume or bind mount to local filesystem |
| Encryption at rest | SQLite encryption at rest is not implemented; rely on disk-level encryption (LUKS, cloud KMS) |

---

## Dependencies

- `sqlite3` CLI (version 3.x)
- Persistent SQLite store (not in-memory)
- Sufficient disk space: maintain at least 2x the current DB size in free space
- Backup destination: local filesystem or network storage with acceptable RTO

---

## Related documentation

- Deployment and operations: [15-deployment-and-operations.md](../15-deployment-and-operations.md)
- TLS ingress runbook: [ops-tls-ingress-runbook.md](ops-tls-ingress-runbook.md)
- Troubleshooting: [17-troubleshooting.md](../17-troubleshooting.md)
