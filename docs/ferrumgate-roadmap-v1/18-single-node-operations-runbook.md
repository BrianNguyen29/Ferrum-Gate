# 18 — Single-Node Operations Runbook

FerrumGate v1 single-node scope. Operator-facing guide for deployment,
verification, backup, restore, and recovery of a single-node FerrumGate
instance backed by SQLite.

---

## 1. Scope and Support Boundaries

> **Canonical support contract**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
> **Operator checks (concise daily/pre-change verification)**: [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)
> **Observability minimums**: [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)
> **Configuration reference**: [15-deployment-and-operations.md](./15-deployment-and-operations.md)

### Supported (single-node v1)
- Single-node governance core with SQLite-backed persistence
- Gateway flow: evaluate -> mint -> authorize -> prepare -> compensate
- Approval queries: GET /v1/approvals, GET /v1/approvals/{id}
- Provenance/lineage: GET /v1/provenance/lineage/{id}, POST /v1/provenance/lineage, POST /v1/provenance/query
- Health and readiness: GET /v1/healthz, GET /v1/readyz, GET /v1/readyz/deep
- CLI inspect commands: health, inspect-execution, inspect-approvals,
  inspect-approval, inspect-lineage, inspect-provenance
- Config file, environment variable, and CLI argument configuration
- Bearer-token authentication mode

### Partial (v1 single-node, not production-verified)
- Adapter surfaces (fs, sqlite, maildraft, git, http) — crate/API shape only,
  no real side-effect integrations
- compensate() — may be noop-backed; not guaranteed to produce external undo
- healthz / readyz — shallow endpoints; readyz/deep provides bounded store probe (opt-in)

### Not supported (post-v1)
- Multi-node, HA, or read-replica deployments
- Real adapter implementations (fs, sqlite, maildraft, git, http)
- Commit and rollback routes (not exposed in the v1 router)
- U1-U4 upgrade tracks

---

## 2. Preconditions and Safety Checks

Before deploying or restarting FerrumGate:

1. **Store DSN must be set.** Use `sqlite://` for a persistent file or
   `sqlite::memory:` for a transient in-memory store. The store DSN
   determines where the SQLite database file resides. If the parent
   directory does not exist or is not writable, the server will fail to
   start with "failed to connect to sqlite" or "failed to apply migrations".

2. **Auth mode and bind address must be consistent.**
   - If `auth_mode = "disabled"`, you may only bind to loopback addresses
     (127.0.0.1 or ::1). Binding to a non-loopback address (including
     0.0.0.0) with auth disabled will cause startup failure with:
     "binding to non-loopback address requires --allow-insecure-nonlocal-bind
     when auth is disabled"
   - If you need non-loopback access, set `auth_mode = "bearer"` and provide
     a non-empty `bearer_token`. This is required for any production
     deployment exposed beyond localhost.

3. **Disk space.** SQLite persists to a file; ensure adequate disk space
   for the store path. The database grows with intents, executions,
   capabilities, approvals, and provenance events.

4. **Config file location.** If using a config file, confirm the path is
   correct and readable. Config precedence (highest first):
   CLI arguments > environment variables (FERRUMD_*) > config file > defaults.
   Full config reference: [15-deployment-and-operations.md](./15-deployment-and-operations.md).

---

## 3. Deploy or Restart Procedure

> **Full CLI/flag reference**: [15-deployment-and-operations.md](./15-deployment-and-operations.md)

```bash
# From project root or wherever the binary lives
ferrumd --config /path/to/ferrumgate.toml
```

Startup sequence:
1. Server binds to the configured address.
2. `ferrum-store` applies embedded migrations to the SQLite store.
3. Gateway registers routes.
4. Server begins serving HTTP.

If the server fails to bind or migrations fail, the process exits with a
non-zero code and an error message to stderr.

---

## 4. Post-Start Verification

After startup, verify the node is operational. **For the full verification ladder,
see** [20-v1-single-node-operator-checks.md §3](./20-v1-single-node-operator-checks.md#3-startup-health-verification-ladder).

Summary:
1. `GET /v1/healthz` — shallow check (no auth)
2. `GET /v1/readyz` — shallow check (no auth)
3. `GET /v1/readyz/deep` — deep check with store probe; returns HTTP 200 if store healthy, HTTP 503 if not (no auth)
4. Functional probe: `GET /v1/approvals?limit=1` with bearer auth — confirms store, auth, and governance loop

> **Important:** healthz and readyz are shallow checks. They confirm the
> server process is alive and the HTTP endpoint is reachable, but they do
> not guarantee that the governance loop or store is fully functional for
> your workload. Always perform a functional probe after startup.

---

## 5. Backup Procedure (SQLite Backup)

FerrumGate persists all state to a single SQLite database file. The
`ferrumctl backup` command provides a bounded SQLite backup workflow using
the rusqlite backup API for consistent snapshots.

### 5.1 Pre-backup checklist

- Confirm the server is running and healthy.
- Ideally, pause new execution creation during backup (out of scope for
  v1 — coordinate manually with operators).

### 5.2 Backup steps

```bash
# Assumes store DSN is sqlite:///var/lib/ferrumgate/ferrumgate.db
STORE_PATH="/var/lib/ferrumgate/ferrumgate.db"
BACKUP_DIR="/var/backups/ferrumgate"

# Create backup using ferrumctl (backup API + restrictive permissions)
ferrumctl backup create --db-path "$STORE_PATH" --output-dir "$BACKUP_DIR"
# Output: /var/backups/ferrumgate/ferrumgate_<timestamp>.db

# Verify the backup is valid
ferrumctl backup verify --db-path "$BACKUP_DIR/ferrumgate_<timestamp>.db"
# Expected: OK
```

### 5.3 Notes

- `ferrumctl backup create` uses the rusqlite backup API for a consistent
  snapshot; it opens the source DB read-only and copies via SQLite's
  internal backup mechanism.
- Backup file is created with restrictive permissions (0600) on Unix.
- Store multiple snapshots with timestamps for point-in-time recovery.
- Backup files can be large; monitor disk usage in the backup directory.
- This is a crash-consistent backup: it captures the state at the moment
  of the copy. It does not flush in-flight transactions.
- SQLite-only; there is no PostgreSQL backup in v1.

### 5.4 External Scheduling (Operator-Owned)

FerrumGate v1 does not include a built-in backup scheduler. Backup
scheduling and retention policy are operator-owned concerns. Below are
examples of operator-implemented external scheduling.

#### cron example

```bash
# /etc/cron.d/ferrumgate-backup
# Run backup at 02:00 daily, keep 7 daily snapshots
SHELL=/bin/bash
PATH=/usr/local/bin:/usr/bin:/bin
0 2 * * * root /usr/local/bin/ferrumctl backup create \
    --db-path "/var/lib/ferrumgate/ferrumgate.db" \
    --output-dir "/var/backups/ferrumgate" \
    && find /var/backups/ferrumgate -name "ferrumgate_*.db" -mtime +7 -delete
```

#### systemd timer example

```bash
# /etc/systemd/system/ferrumgate-backup.service
[Unit]
Description=FerrumGate SQLite Backup
Requires=ferrumd.service

[Service]
Type=oneshot
ExecStart=/usr/local/bin/ferrumctl backup create \
    --db-path "/var/lib/ferrumgate/ferrumgate.db" \
    --output-dir "/var/backups/ferrumgate"
PrivateTmp=true

# /etc/systemd/system/ferrumgate-backup.timer
[Unit]
Description=FerrumGate SQLite Backup (daily)

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
# Enable and start the timer
systemctl enable ferrumgate-backup.timer
systemctl start ferrumgate-backup.timer

# For retention, add a separate cleanup service or use tmpfiles.d
# Example: /etc/tmpfiles.d/ferrumgate-backup.conf
# d /var/backups/ferrumgate 0755 root root 7d
```

**Important**: These are operator-owned examples. Adjust paths, schedules,
retention periods, and notifications to match your operational
requirements. FerrumGate provides `ferrumctl backup` as the backup
primitive only; scheduler, retention rotation, and offsite transfer are
external responsibilities.

---

## 6. Restore Procedure (SQLite Restore)

To restore from a backup using `ferrumctl backup restore`:

### 6.1 Pre-restore checklist

- **Stop the running `ferrumd` process** (required — restore will refuse
  if the DB is locked by another process).
- Confirm the backup file exists and is valid:
  `ferrumctl backup verify --db-path /var/backups/ferrumgate/<backup>.db`
- Identify the current store path (from config or --store-dsn flag).

### 6.2 Restore steps

```bash
CURRENT_STORE="/var/lib/ferrumgate/ferrumgate.db"
BACKUP_FILE="/var/backups/ferrumgate/ferrumgate_<timestamp>.db"

# Stop ferrumd if running
FERRUM_PID=$(pgrep -f ferrumd)
if [ -n "$FERRUM_PID" ]; then
  kill "$FERRUM_PID"
  sleep 2
fi

# Restore (requires --confirm; preserves .pre_restore copy automatically)
ferrumctl backup restore \
  --db-path "$CURRENT_STORE" \
  --from "$BACKUP_FILE" \
  --confirm

# Restart ferrumd
ferrumd --config /path/to/ferrumgate.toml
# or restart via systemd / init script as appropriate
```

### 6.3 Safety guardrails

- **`--confirm` is required** — restore will not proceed without it.
- **Exclusive lock detection** — restore attempts to open the current DB
  read-write before touching anything. If the DB is locked (server
  running), restore refuses with a clear error message.
- **Pre-restore copy** — before overwriting, restore automatically copies
  the current DB to `<db_path>.pre_restore` so the pre-restore state is
  preserved.
- **Post-restore verification** — restore runs `PRAGMA integrity_check`
  on the restored file and refuses to proceed if it fails.

### 6.4 Notes

- Restore replaces the entire store; any executions, intents, or approvals
  created after the backup timestamp are lost.
- After restore, run post-start verification (Section 4) to confirm
  the node is operational.
- There is no incremental restore; a full file restore is the only option
  in v1.
- Restore is SQLite-only; there is no PostgreSQL restore in v1.

---

## 7. Recovery Procedure (Compensate + Manual Restore Fallback)

FerrumGate v1 provides a compensate path but no guaranteed external undo.

### 7.1 Compensate path (preferred first step)

The compensate route is available at:

```
POST /v1/executions/{execution_id}/compensate
```

Compensate will attempt to reverse a prepared execution using the
registered rollback contract and adapter (if any). However, in v1:

- compensate may be noop-backed depending on the adapter implementation
  for the execution's rollback class.
- Compensate is not guaranteed to produce a visible external side-effect
  undo (e.g., file system revert, database row revert).
- The API will return 200 on a successful compensate call even if the
  adapter is a no-op.

To attempt compensate:

```bash
curl -X POST http://127.0.0.1:8080/v1/executions/<execution_id>/compensate \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Returns execution state; check for compensated status
```

**After compensate, always verify** the affected resource is in the
expected state. Do not assume compensate produced external undo.

### 7.2 Manual restore fallback (if compensate is insufficient)

If compensate did not resolve the incident, restore from a known-good
SQLite backup:

1. Identify the most recent valid backup (Section 6).
2. Follow the restore procedure in Section 6.
3. Verify the restored state and re-run any lost executions as needed.

### 7.3 Recovery decision guide

| Situation | Action |
|---|---|
| Execution in PREPARED state, no external change observed | Attempt compensate first; if no-op or inconclusive, restore from backup |
| Execution in EXECUTED or VERIFIED state, external change occurred | Compensate may be noop; manual restore likely required |
| Server crash with corrupt store | Restore from latest valid backup |
| Accidental data deletion via intent | Restore from latest backup taken before deletion |

---

## 8. Common Incidents

### Incident 1: Server refuses to bind with "binding to non-loopback address requires --allow-insecure-nonlocal-bind when auth is disabled"

**Cause:** auth_mode is "disabled" and bind_addr is set to a non-loopback
address (e.g., 0.0.0.0 or the machine's LAN IP).

**Resolution (choose one):**
- Set `auth_mode = "bearer"` in config and provide a non-empty
  `bearer_token` — this allows non-loopback bind.
- Set `allow_insecure_nonlocal_bind = true` in config — only for
  development.
- Change `bind_addr` to `127.0.0.1:8080` — restricts access to localhost.

### Incident 2: "bearer token cannot be empty when auth mode is bearer" at startup

**Cause:** auth_mode is set to "bearer" but no bearer_token is configured.

**Resolution:** Provide a non-empty `bearer_token` in the config file or
via the `FERRUMD_BEARER_TOKEN` environment variable.

### Incident 3: "failed to connect to sqlite" or "failed to apply migrations" at startup

**Cause:** Store DSN is invalid, the parent directory does not exist, or
the process does not have write permission on the store path.

**Resolution:**
- Verify the store DSN is correctly formatted:
  - Persistent file: `sqlite:///var/lib/ferrumgate/ferrumgate.db`
  - In-memory (transient): `sqlite::memory:`
- Confirm the parent directory exists and is writable.
- Check disk space.

### Incident 4: 401 Unauthorized on API calls

**Cause:** Bearer token is missing, invalid, or mismatched.

**Resolution:**
- Confirm `Authorization: Bearer <token>` header is present.
- Verify `FERRUMCTL_BEARER_TOKEN` matches the configured `bearer_token`.
- Check that the server's auth_mode matches the token configuration
  (if server uses bearer, client must use bearer).

### Incident 5: 404 Not Found on execution or approval inspect

**Cause:** The requested resource ID does not exist in the store.

**Resolution:**
- Verify the execution_id or approval_id is correct.
- Use `curl http://127.0.0.1:8080/v1/approvals?limit=N \
    -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"` to enumerate known
  approval IDs (omit -H when auth_mode=disabled).
- The resource may have been lost if a restore from backup was performed.

### Incident 6: healthz/readyz return 200 but the node appears unresponsive

**Cause:** healthz and readyz are shallow checks that confirm the HTTP
server is alive but do not validate the store or governance loop.

**Resolution:**
- Run a functional probe: `curl http://127.0.0.1:8080/v1/approvals?limit=1 \
    -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"` (omit -H when auth_mode=disabled).
- If that fails, check ferrumd logs for store or migration errors.
- If the store is unreachable, restore from a known-good backup.

### Incident 7: Compensate returns success but external state is unchanged

**Cause:** The compensate adapter for the execution's rollback class is
a no-op in v1.

**Resolution:**
- Do not rely on compensate for external undo in v1.
- Proceed to manual restore from a known-good backup (Section 6).

---

## 9. Known Limitations and Operator Caveats

1. **No commit/rollback routes.** The v1 router does not expose
   POST /v1/executions/{id}/commit or POST /v1/executions/{id}/rollback.
   Compensate is the only provided recovery endpoint.

2. **Compensate may be noop-backed.** Depending on the adapter
   implementation and rollback class (R0/R1/R2/R3), compensate() may
   return 200 without performing any external undo action. Always verify
   resource state manually after compensate.

3. **Health and ready are shallow.** GET /v1/healthz and GET /v1/readyz
   confirm the server process is alive and the HTTP endpoint is
   reachable. They do not validate that the store, migrations, or
   governance loop are fully functional. Always run a functional probe
   (e.g., inspect-execution or inspect-approvals) after startup to
   confirm end-to-end readiness.

4. **Single-node only, no HA.** There is no multi-node, read-replica,
   or HA configuration in v1. The SQLite store is the only persistence
   layer. If the node fails, recovery is via manual SQLite restore from
   a backup.

5. **Bounded backup command.** `ferrumctl backup` provides create/verify/restore
   with safety guardrails, but there is no incremental backup, no
   automated scheduling, and no backup retention policy built into
   FerrumGate. Operators must implement their own backup scheduling and
   rotation. Backup/restore is SQLite-only; no PostgreSQL backup in v1.

6. **Restore is full-file, not incremental.** Restoring from a backup
   overwrites the entire store. Any executions, intents, approvals, or
   provenance events created after the backup timestamp are lost.

7. **No adapter-backed real undo.** Adapter surfaces (fs, sqlite,
   maildraft, git, http) are skeleton implementations in v1. Real
   side-effect undo via adapters is a post-v1 item.

8. **No rollback contract auto-enforcement at prepare.** The gateway's
   prepare handler hardcodes rollback class at the prepare step. The
   caller is responsible for ensuring the correct rollback_class is
   persisted at intent creation. See `26-v1-single-node-invariant-control-test-evidence-matrix.md`
   Weak Spot 1.

9. **Single-use capability not enforced end-to-end at authorize.**
   The capability service's `mark_used` is not called by the gateway's
   authorize path. Caller must ensure single-use capability mapping is
   respected client-side. See `26-v1-single-node-invariant-control-test-evidence-matrix.md`
   Weak Spot 3.

10. **Provenance chain completeness not test-covered end-to-end.**
    Lineage events are emitted at each gateway step, but there is no
    integration test that queries the lineage endpoint for a full
    execution chain and confirms every step appears. Manual tracing or
    ad-hoc verification is required to confirm completeness. See
    `26-v1-single-node-invariant-control-test-evidence-matrix.md`
    Weak Spot 4.

---

## References

- Configuration reference: [15-deployment-and-operations.md](./15-deployment-and-operations.md)
- Operator checks (daily/pre-change verification): [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)
- Observability minimums (logs, probes, thresholds): [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)
- Support contract: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
- API endpoint reference: [14-api-and-contracts-map.md](./14-api-and-contracts-map.md)
- Troubleshooting: [17-troubleshooting.md](./17-troubleshooting.md)
- Persistence model: [12-persistence-and-data-model.md](./12-persistence-and-data-model.md)
