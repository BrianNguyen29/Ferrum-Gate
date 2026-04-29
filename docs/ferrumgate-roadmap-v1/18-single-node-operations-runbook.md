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
1. `ferrumctl server health` — shallow check (no auth); or `curl http://127.0.0.1:8080/v1/healthz`
2. `ferrumctl server readiness` — shallow readiness (no auth); or `curl http://127.0.0.1:8080/v1/readyz`
3. `ferrumctl server readiness --deep` — deep readiness with store probe (no auth); or `curl http://127.0.0.1:8080/v1/readyz/deep`
4. `ferrumctl server readiness --functional` — functional probe with bearer auth; confirms store, auth, and governance loop; or `curl http://127.0.0.1:8080/v1/approvals?limit=1 -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"`

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

# Create backup with automatic retention pruning (keeps 7 days of backups)
ferrumctl backup create --db-path "$STORE_PATH" --output-dir "$BACKUP_DIR" --retention-days 7
# After creating the new backup, older backups matching the same source DB
# name pattern (ferrumgate_*.db) with mtime > 7 days are deleted.
# The newly created backup is never deleted.
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
- `--retention-days N` prunes old backups after successful backup creation.
  Only files matching `<source_db_name>_*.db` with mtime > N days are deleted.
  N must be at least 1. Omitting the flag performs no pruning (backward-compatible).

### 5.4 External Scheduling (Operator-Owned)

FerrumGate v1 does not include a built-in backup scheduler. Backup
scheduling and encryption are operator-owned external concerns.
`--retention-days` provides opt-in retention pruning after each backup,
but automated scheduling must still be implemented externally (cron, systemd
timers, etc.). Below are examples of operator-implemented scheduling.

#### cron example

```bash
# /etc/cron.d/ferrumgate-backup
# Run backup at 02:00 daily with 7-day retention pruning
SHELL=/bin/bash
PATH=/usr/local/bin:/usr/bin:/bin
0 2 * * * root /usr/local/bin/ferrumctl backup create \
    --db-path "/var/lib/ferrumgate/ferrumgate.db" \
    --output-dir "/var/backups/ferrumgate" \
    --retention-days 7
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
    --output-dir "/var/backups/ferrumgate" \
    --retention-days 7
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
```

**Important**: These are operator-owned examples. Adjust paths, schedules,
and notifications to match your operational requirements. Scheduling,
encryption, and offsite transfer remain external responsibilities.

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
   with safety guardrails and opt-in retention pruning (`--retention-days N`).
   There is no incremental backup, no automated scheduling, and no encryption.
   Operators must implement their own backup scheduling and encryption if needed.
   Backup/restore is SQLite-only; no PostgreSQL backup in v1.

6. **Restore is full-file, not incremental.** Restoring from a backup
   overwrites the entire store. Any executions, intents, approvals, or
   provenance events created after the backup timestamp are lost.

7. **No adapter-backed real undo.** Adapter surfaces (fs, sqlite,
   maildraft, git, http) are skeleton implementations in v1. Real
   side-effect undo via adapters is a post-v1 item.

8. **Rollback class handling is resolved, but still operator-relevant.**
   Weak Spot 1 is resolved in the v1 evidence matrix: R3 `auto_commit=false`
   is verified and rollback-class handling is covered by tests. Operators must
   still ensure callers choose the correct `rollback_class` for their workload
   before signing G2.7 accepted-risk review.

9. **Single-use capability enforcement is resolved in gateway flow.**
   Weak Spot 3 is resolved in the v1 evidence matrix: durable capability-use
   marking is wired and tested. Operators must still monitor clients for retry
   behavior that attempts to reuse expired or already-used capabilities.

10. **Provenance chain completeness is test-covered.**
    Weak Spot 4 is resolved in the v1 evidence matrix: integration tests query
    lineage and verify the expected execution/provenance chain. Operators should
    still include lineage checks in pilot daily/incident procedures.

---

## 10. Production Pilot Procedures (Path 2)

> **Context**: FerrumGate v1 is RC-ready/conditional for single-node SQLite only.
> No production-ready claim is made. This section supplements the full
> `31-release-paths-todo.md` §Path 2 pilot runbook with operator-facing
> procedural detail for the active pilot period.

### 10.1 Pilot Start Conditions

Before the first production pilot deployment, confirm all of:

| # | Condition | Verification |
|---|---|---|
| 1 | All G2 gates satisfied with documented operator signoff | `54-operator-signoff-packet.md` completed and signed |
| 2 | Write workload modeled against SQLite capacity | Expected sustained writes ≤300 writes/s |
| 3 | Bearer auth configured; TLS/reverse proxy confirmed | Config review |
| 4 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` |
| 5 | Restore drill completed with `PRAGMA integrity_check` passing | Operator evidence |
| 6 | RPO/RTO formally accepted for target workload | Operator signoff |
| 7 | All production evaluation dimensions SATISFIED or CONDITIONAL | `27-production-evaluation-plan.md` Evaluation Decision Framework |
| 8 | Accepted risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed |
| 9 | Compensate noop risk formally accepted | Operator acknowledgment |

### 10.2 Daily Pilot Checks

| Check | Frequency | Threshold | Action if Exceeded |
|---|---|---|---|
| `GET /v1/readyz/deep` returns HTTP 200 | Daily | HTTP 503 = store unreachable | Investigate; restore from backup if corruption |
| `ferrumctl backup verify` passes | After each backup | `PRAGMA integrity_check` failure | Do not use backup; take new backup after fixing |
| Error rate on S4/S5/S6/S7 | Per monitoring interval | >0% error rate | Page on-call; evaluate against abort criteria |
| Write queue depth | Per monitoring interval | Sustained backlog >100 items | Evaluate write throughput fit |
| Disk space on store volume | Daily | <10% free | Alert; risk of DB lock |

### 10.3 Monitoring Thresholds

| Metric | Warning | Critical | Go/No-Go |
|---|---|---|---|
| Sustained write rate | >200 writes/s | >250 writes/s | >300 writes/s triggers Path 3 evaluation |
| p50 write latency | >50ms | >100ms | >200ms triggers Path 3 evaluation |
| Error rate (any scenario) | >0.1% | >0% | >0% = abort pilot |
| Backup verify | N/A | `PRAGMA integrity_check` fail | Do not deploy; fix before proceeding |

### 10.4 Abort Triggers

| Trigger | Action |
|---|---|
| Write throughput exceeds Phase 1 capacity (>300 writes/s sustained) | Abort pilot; migrate to Path 3 PostgreSQL |
| `PRAGMA integrity_check` fails on any backup or store | Abort pilot; restore from last known-good backup |
| Error rate >0% on S4/S5/S6/S7 | Abort pilot; investigate regression |
| RPO/RTO no longer meets target workload SLA | Abort pilot; evaluate Path 3 |
| Any G2 signoff item declined by operator | Abort pilot; resolve or formally accept risk |
| Compensate noop risk unacceptable for target adapters | Abort pilot; adapter implementation required before R1/R2/R3 use |
| SQLite store corruption or data integrity failure | Abort pilot; restore from backup and investigate |

### 10.5 Completion Criteria

| # | Criterion | Evidence Required |
|---|---|---|
| 1 | Pilot workload processed for agreed evaluation period | Operator logs / monitoring data |
| 2 | All governance behaviors verified for pilot workflow | Integration test evidence or manual verification log |
| 3 | Backup/restore drill completed successfully | Operator evidence with `PRAGMA integrity_check` passing |
| 4 | No abort triggers encountered during pilot period | Operator incident log |
| 5 | Operator formally accepts pilot outcome | Signed completion statement per `54-operator-signoff-packet.md` |

### 10.6 Decision Log Template

| Date | Decision | Owner | Rationale |
|---|---|---|---|
| YYYY-MM-DD | Pilot started | Operator | Reason for pilot scope and target workload |
| YYYY-MM-DD | Abort / Continue / Complete | Operator | Evidence-based assessment |
| YYYY-MM-DD | Proceed to Path 3 or single-node production | Operator + Engineering lead | Based on pilot outcome |

### 10.7 Cross-References

| Document | Purpose |
|---|---|
| `31-release-paths-todo.md` §Path 2 | Full pilot path with G2 gates and checklists |
| `54-operator-signoff-packet.md` | Operator signoff form with evidence fields and final acceptance statement |
| `27-production-evaluation-plan.md` | Production evaluation framework |
| `55-phase-3-go-no-go-review.md` | Phase 3 go/no-go gates (G3.1 satisfied by v0.1.0-rc.1; G3.2–G3.4 pending) |

---

## References

- Configuration reference: [15-deployment-and-operations.md](./15-deployment-and-operations.md)
- Operator checks (daily/pre-change verification): [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)
- Observability minimums (logs, probes, thresholds): [21-v1-single-node-observability-minimums.md](./21-v1-single-node-observability-minimums.md)
- Support contract: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
- API endpoint reference: [14-api-and-contracts-map.md](./14-api-and-contracts-map.md)
- Troubleshooting: [17-troubleshooting.md](./17-troubleshooting.md)
- Persistence model: [12-persistence-and-data-model.md](./12-persistence-and-data-model.md)
