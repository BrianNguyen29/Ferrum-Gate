# Troubleshooting Guide

> **Status**: Scaffold. Common issues compiled from local testing and target-host drills.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## ferrumd will not start

### Symptom: `Permission denied` on SQLite database

**Cause**: ferrumd user does not have write access to the database directory.

**Fix**:
```bash
chown -R ferrumgate:ferrumgate /var/lib/ferrumgate
chmod 750 /var/lib/ferrumgate
```

### Symptom: `Address already in use`

**Cause**: Another process is bound to port 8080.

**Fix**:
```bash
lsof -i :8080
# Then stop the conflicting process or change bind_addr
```

### Symptom: `unsupported DSN scheme`

**Cause**: DSN prefix is not recognized or postgres feature is not enabled.

**Fix**: For PostgreSQL, build with `--features postgres`.

---

## `/v1/readyz/deep` returns 503

### Symptom: Deep readiness failing

**Possible causes**:
1. SQLite database is locked or corrupted.
2. Write queue depth exceeds 100.
3. Store connection pool exhausted.

**Diagnostic**:
```bash
curl http://localhost:8080/v1/readyz/deep
curl http://localhost:8080/v1/metrics | grep ferrumgate_store_health_up
curl http://localhost:8080/v1/metrics | grep ferrumgate_write_queue_depth
```

**Fixes**:
- If store unhealthy: check disk space, run `PRAGMA integrity_check` on SQLite.
- If queue depth high: reduce incoming request rate or consider PostgreSQL.

---

## Auth failures

### Symptom: 401 on all mutating endpoints

**Cause**: Bearer token mismatch or missing.

**Fix**:
1. Verify `auth_mode` is `Bearer` in config.
2. Verify token in request header matches config token.
3. Check for whitespace or encoding issues in header.

### Symptom: Token was working, now 401

**Cause**: Token was rotated.

**Fix**: Use the new token. Verify old token fails (expected).

---

## Backup/restore issues

### Symptom: `ferrumctl backup` fails

**Cause**: Insufficient disk space or permissions.

**Fix**:
```bash
df -h /backups
ls -ld /backups
```

### Symptom: Restore succeeds but data is missing

**Cause**: Restored to wrong database path or ferrumd is still running with old file handle.

**Fix**:
1. Stop ferrumd before restore.
2. Verify `--db-path` matches the running config.
3. Start ferrumd after restore.
4. Verify with `ferrumctl health` and inspect latest execution.

---

## MCP connection issues

### Symptom: `tools/list` returns fewer than 19 tools

**Cause**: MCP server connected to wrong gateway or gateway is unhealthy.

**Fix**:
1. Verify `FERRUMGATE_GATEWAY_URL` points to the correct gateway.
2. Check gateway health: `curl <gateway>/v1/healthz`.
3. Check MCP server logs for connection errors.

### Symptom: Mutating tools succeed without auth

**Cause**: Gateway is in `Disabled` auth mode.

**Fix**: Configure `auth_mode = "Bearer"` and set a bearer token.

---

## Performance issues

### Symptom: High latency on write endpoints

**Cause**: SQLite lock contention or queue backlog.

**Diagnostic**:
```bash
curl http://localhost:8080/v1/metrics | grep write_queue_depth
curl http://localhost:8080/v1/metrics | grep request_duration_seconds
```

**Fixes**:
- If queue depth is growing: reduce concurrency or move to PostgreSQL.
- If latency spikes: check disk I/O performance.

---

## Getting help

1. Check [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) for runtime configuration details.
2. Check [`operator.md`](./operator.md) for config and incident response procedures.
3. Review logs: `journalctl -u ferrumgate -n 500`
4. Check metrics: `curl /v1/metrics`
5. If issue persists, capture:
   - ferrumd version (`ferrumd --version`)
   - Config (redact token)
   - Metrics snapshot
   - Repro steps

## Status caveat

> **production-ready = NO**. This troubleshooting guide is based on local testing and conditional pilot experience. Target-host issues may differ. Always verify with target-host evidence.

## Related docs

- [`operator.md`](./operator.md) — Config, backup, incident response.
- [`hosted-deployment.md`](./hosted-deployment.md) — Deployment modes.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime configuration and stress baselines.
