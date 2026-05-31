# Troubleshooting Guide

> **Parent**: [`guides/README.md`](./README.md)

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

### Symptom: Scoped token returns 403 on allowed endpoint

**Cause**: Token lacks the required scope for the endpoint.

**Fix**:
1. Check token scopes with `ferrumctl admin tokens list`.
2. Verify the endpoint-to-scope mapping in the API documentation.
3. Issue a new token with the correct scope or use a role that includes it.

---

## Rate limiting

### Symptom: 429 on most requests under load

**Cause**: Rate-limit profile is too conservative for the workload.

**Diagnostic**:
```bash
curl -s http://localhost:8080/v1/metrics | grep 'ferrumgate_governance_errors_total{status="429"}'
```

**Fix**:
- If running validation workloads, switch to the high-throughput profile (`1000/10000`).
- If running real traffic, measure per-IP sustained RPS and peak burst, then set `rate_limit_per_second` and `rate_limit_burst` to at least 2× observed values.
- See [`docs/operations/rate-limit-tuning-guide.md`](../operations/rate-limit-tuning-guide.md).

---

## PostgreSQL / PgBouncer issues

### Symptom: `readyz/deep` 503 after PostgreSQL restart

**Cause**: Pool connections were severed; in-flight queries failed.

**Diagnostic**:
```bash
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_health_up
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_acquire_timeouts_total
```

**Fix**:
- Wait for `sqlx::PgPool` to reconnect transparently on new acquisitions (no ferrumd restart required for transient outages).
- If readiness stays unhealthy > incident threshold, restart ferrumd.
- Verify PostgreSQL is reachable: `pg_isready -h localhost -p 5432`.

### Symptom: PgBouncer rejects connections

**Cause**: Auth file mismatch, max client connections reached, or backend TLS failure.

**Fix**:
1. Check PgBouncer logs: `journalctl -u pgbouncer -n 100`.
2. Verify `userlist.txt` matches PostgreSQL credentials.
3. Check `SHOW POOLS;` for saturation (`cl_active` near `max_client_conn`).
4. If backend TLS changed, ensure `server_tls_ca_file` and PostgreSQL cert are in sync.

### Symptom: High latency on PG-backed endpoints

**Cause**: Pool saturation, slow queries, or replication lag (if using replicas).

**Diagnostic**:
```bash
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_pool_idle
curl -s http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_pool_size
```

**Fix**:
- Increase `pg_max_connections` or PgBouncer `default_pool_size`.
- Reduce long-running transactions.
- For replica lag, consult the hosted deployment guide (design only; no implementation).

---

## HA failover / failback

### Symptom: ferrumd unhealthy after primary PostgreSQL failure

**Cause**: ferrumd cannot reach the old primary; standby has not been promoted or ferrumd is still pointing to the old primary.

**Fix** (manual / operator-controlled):
1. Confirm primary is down: `pg_isready -h <primary> -p 5432`.
2. Promote standby: `pg_ctlcluster 16 main promote` (or `pg_promote()`).
3. Update ferrumd DSN or PgBouncer backend to point to the new primary.
4. Restart ferrumd if the DSN changed.
5. Verify `readyz/deep` returns 200.
6. Rebuild old primary as standby before failback.

> **Note**: Automated unattended failover is not available. These steps are manual/operator-controlled outside same-VM scope.

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

### Symptom: PostgreSQL backup is stale

**Cause**: Backup timer failed or offsite sync lag.

**Diagnostic**:
```bash
systemctl status ferrumgate-postgres-backup.timer
systemctl status ferrumgate-postgres-retention.timer
ls -lt /var/backups/ferrumgate-postgres/ | head -n 5
```

**Fix**:
- Check timer logs for errors.
- Verify disk space: `df -h /var/backups/ferrumgate-postgres`.
- Re-run backup manually and verify offsite sync.

---

## MCP connection issues

### Symptom: `tools/list` returns fewer than 19 tools

**Cause**: MCP server connected to wrong gateway or gateway is unhealthy.

**Fix**:
1. Verify `FERRUM_GATEWAY_URL` points to the correct gateway.
2. Check gateway health: `curl <gateway>/v1/healthz`.
3. Check MCP server logs for connection errors.

### Symptom: Mutating tools succeed without auth

**Cause**: Gateway is in `Disabled` auth mode.

**Fix**: Configure `auth_mode = "Bearer"` and set a bearer token.

---

## Alert issues

### Symptom: Prometheus target shows `DOWN`

**Cause**: ferrumd is not running, scrape URL is wrong, or firewall blocks the scrape port.

**Fix**:
1. `curl http://localhost:19080/v1/metrics` from the Prometheus host.
2. Verify `prometheus.yml` job `ferrumgate-ferrumd` points to the correct port.
3. Check ferrumd service status: `systemctl status ferrumgate`.

### Symptom: `FerrumGatePostgresPoolSaturation` firing

**Cause**: Pool idle is 0 and pool size >= max.

**Fix**:
- Increase `pg_max_connections` or PgBouncer `default_pool_size`.
- Reduce concurrent long-running requests.
- Scale horizontally (not available in current release).

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

1. Check [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) for runtime configuration details.
2. Check [`operator.md`](./operator.md) for config and incident response procedures.
3. Review logs: `journalctl -u ferrumgate -n 500`
4. Check metrics: `curl /v1/metrics`
5. If issue persists, capture:
   - ferrumd version (`ferrumd --version`)
   - Config (redact token)
   - Metrics snapshot
   - Repro steps

### Validation commands cheat sheet

```bash
# Liveness
curl http://localhost:8080/v1/healthz

# Deep readiness
curl http://localhost:8080/v1/readyz/deep

# Metrics
curl http://localhost:8080/v1/metrics

# Audit log (admin:audit scope required)
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/admin/audit-logs?limit=5

# Token list (admin scope)
ferrumctl admin tokens list

# Backup verify
ferrumctl backup verify --db-path /backups/ferrumgate-YYYYMMDD-HHMMSS.db

# PG backup listable
pg_restore -l /var/backups/ferrumgate-postgres/ferrumgate-*.dump > /dev/null && echo OK
```

## Related docs

- [`operator.md`](./operator.md) — Config, backup, incident response.
- [`hosted-deployment.md`](./hosted-deployment.md) — Deployment modes.
- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration and stress baselines.
