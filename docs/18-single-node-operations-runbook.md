# 18 — Single-Node Operations Runbook

FerrumGate v1 single-node scope. Operator-facing guide for deployment,
verification, backup, restore, and recovery of a single-node FerrumGate
instance backed by SQLite.

---

## 1. Scope and Support Boundaries

> **Canonical support contract**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
> **Operator checks**: [20-v1-single-node-operator-checks.md](./20-v1-single-node-operator-checks.md)

### Supported (single-node v1)
- Single-node governance core with SQLite-backed persistence
- Gateway flow: evaluate -> mint -> authorize -> prepare -> compensate
- Approval queries: GET /v1/approvals, GET /v1/approvals/{id}
- Provenance/lineage: GET /v1/provenance/lineage/{id}, POST /v1/provenance/lineage, POST /v1/provenance/query
- Health and readiness: GET /v1/healthz, GET /v1/readyz
- CLI inspect commands: health, inspect-execution, inspect-approvals,
  inspect-approval, inspect-lineage, inspect-provenance
- Config file, environment variable, and CLI argument configuration
- Bearer-token authentication mode

### Partial (v1 single-node, not production-verified)
- Adapter surfaces (fs, sqlite, maildraft, git, http) — crate/API shape only,
  no real side-effect integrations
- compensate() — may be noop-backed; not guaranteed to produce external undo
- healthz / readyz — shallow endpoints; require a functional probe after startup

### Not supported (post-v1)
- Multi-node, HA, or read-replica deployments
- Real adapter implementations (fs, sqlite, maildraft, git, http)
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

---

## 3. Deploy or Restart Procedure

### 3.1 Normal startup

```bash
# From project root or wherever the binary lives
ferrumd --config /path/to/ferrumgate.toml
```

Or with individual flags:

```bash
ferrumd \
  --bind-addr 127.0.0.1:8080 \
  --store-dsn "sqlite:///var/lib/ferrumgate/ferrumgate.db" \
  --auth-mode bearer \
  --bearer-token "$FERRUM_BEARER_TOKEN" \
  --log-filter info
```

### 3.2 Environment variable approach

```bash
export FERRUMD_BIND_ADDR=127.0.0.1:8080
export FERRUMD_STORE_DSN="sqlite:///var/lib/ferrumgate/ferrumgate.db"
export FERRUMD_AUTH_MODE=bearer
export FERRUMD_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"
export FERRUMD_LOG_FILTER=info

ferrumd
```

### 3.3 Startup sequence

1. Server binds to the configured address.
2. `ferrum-store` applies embedded migrations to the SQLite store.
3. Gateway registers routes.
4. Server begins serving HTTP.

If the server fails to bind or migrations fail, the process exits with a
non-zero code and an error message to stderr.

---

## 4. Post-Start Verification

After startup, verify the node is operational:

```bash
# 1. Shallow health check (server is listening)
curl http://127.0.0.1:8080/v1/healthz
# Expected: 200 OK, {"status":"ok"} or similar

# 2. Readiness check (shallow — confirms HTTP endpoint is reachable; does not validate store or internal state)
curl http://127.0.0.1:8080/v1/readyz
# Expected: 200 OK if ready; 200 OK alone does not guarantee store or governance loop is functional

# 3. Deep verification (requires bearer auth if auth_mode=bearer)
export FERRUMCTL_SERVER_URL=http://127.0.0.1:8080
export FERRUMCTL_BEARER_TOKEN="$FERRUM_BEARER_TOKEN"
ferrumctl server health
ferrumctl server inspect-execution 00000000-0000-0000-0000-000000000001
# The execution inspect will return 404 for unknown IDs (expected);
# absence of transport error confirms connectivity is functional.

# 4. Verify approvals endpoint reachable
curl http://127.0.0.1:8080/v1/approvals?limit=1 \
  -H "Authorization: Bearer $FERRUM_BEARER_TOKEN"
# Returns empty list or actual approvals; 401 indicates auth issue.
# Note: omit the -H flag when auth_mode=disabled.
```

**Important:** healthz and readyz are shallow checks. They confirm the
server process is alive and the HTTP endpoint is reachable, but they do
not guarantee that the governance loop or store is fully functional for
your workload. Always perform a functional probe (such as an execution
inspect or approvals list) after startup to confirm end-to-end readiness.

---

## 5. Backup Procedure (Manual SQLite File Backup)

FerrumGate persists all state to a single SQLite database file. There is
no built-in backup command; the operator must perform manual file-level
backup.

### 5.1 Pre-backup checklist

- Confirm the server is running and healthy.
- Ideally, pause new execution creation during backup (out of scope for
  v1 — coordinate manually with operators).

### 5.2 Backup steps

```bash
# Assumes store DSN is sqlite:///var/lib/ferrumgate/ferrumgate.db
STORE_PATH="/var/lib/ferrumgate/ferrumgate.db"
BACKUP_DIR="/var/backups/ferrumgate"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/ferrumgate_${TIMESTAMP}.db"

# Create backup directory if it does not exist
mkdir -p "$BACKUP_DIR"

# Copy the SQLite file (server should be stable during copy)
cp "$STORE_PATH" "$BACKUP_FILE"

# Verify the backup is a valid SQLite database
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;"
# Expected: "ok"

# Optionally: set restrictive permissions on the backup
chmod 600 "$BACKUP_FILE"

echo "Backup complete: $BACKUP_FILE"
```

### 5.3 Notes

- Use `sqlite3` CLI tool or equivalent to validate backup file integrity.
- Store multiple snapshots with timestamps for point-in-time recovery.
- Backup files can be large; monitor disk usage in the backup directory.
- This is a crash-consistent backup: it captures the state at the moment
  of the copy. It does not flush in-flight transactions.

---

## 6. Restore Procedure (Manual SQLite File Restore)

To restore from a backup:

### 6.1 Pre-restore checklist

- Stop the running `ferrumd` process.
- Confirm the backup file exists and is valid:
  `sqlite3 /var/backups/ferrumgate/ferrumgate_<timestamp>.db "PRAGMA integrity_check;"`
- Identify the current store path (from config or --store-dsn flag).

### 6.2 Restore steps

```bash
CURRENT_STORE="/var/lib/ferrumgate/ferrumgate.db"
BACKUP_FILE="/var/backups/ferrumgate/ferrumgate_20260330_120000.db"
FERRUM_PID=$(pgrep -f ferrumd)

# Stop ferrumd if running
if [ -n "$FERRUM_PID" ]; then
  kill "$FERRUM_PID"
  sleep 2
fi

# Replace current store with backup (keep a pre-restore copy just in case)
cp "$CURRENT_STORE" "${CURRENT_STORE}.pre_restore"
cp "$BACKUP_FILE" "$CURRENT_STORE"

# Verify restored file
sqlite3 "$CURRENT_STORE" "PRAGMA integrity_check;"
# Expected: "ok"

# Restart ferrumd
ferrumd --config /path/to/ferrumgate.toml
# or restart via systemd / init script as appropriate
```

### 6.3 Notes

- Restore replaces the entire store; any executions, intents, or approvals
  created after the backup timestamp are lost.
- After restore, run post-start verification (Section 4) to confirm
  the node is operational.
- There is no incremental restore; a full file restore is the only option
  in v1.

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

1. **Compensate is the primary recovery endpoint.** POST /v1/executions/{id}/commit
   and POST /v1/executions/{id}/rollback are exposed in the v1 router. For
   typical single-node operations, compensate is the preferred recovery path.

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

5. **No built-in backup command.** Backups must be performed manually
   by copying the SQLite file. There is no incremental backup, no
   automated scheduling, and no backup retention policy built into
   FerrumGate. Operators must implement their own backup scheduling and
   rotation.

6. **Restore is full-file, not incremental.** Restoring from a backup
   overwrites the entire store. Any executions, intents, approvals, or
   provenance events created after the backup timestamp are lost.

7. **No adapter-backed real undo.** Adapter surfaces (fs, sqlite,
   maildraft, git, http) are skeleton implementations in v1. Real
   side-effect undo via adapters is a post-v1 item.

---

## References

- Configuration reference: `docs/15-deployment-and-operations.md`
- API endpoint reference: `docs/14-api-and-contracts-map.md`
- Troubleshooting: `docs/17-troubleshooting.md`
- Persistence model: `docs/12-persistence-and-data-model.md`
- Observability minimums (logs, probes, thresholds): `docs/21-v1-single-node-observability-minimums.md`
- v1 RC evidence: `docs/implementation-path/25-v1-single-node-rc-evidence.md`
- Invariant control matrix: `docs/implementation-path/26-v1-single-node-invariant-control-test-evidence-matrix.md`
- Production readiness: `docs/implementation-path/23-production-readiness-assessment.md`
