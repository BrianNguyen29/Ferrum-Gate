# Operator Guide

> **Status**: Expanded. Covers config, health, backup/restore, token rotation, monitoring, incident response, and local-vs-hosted caveats.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Configuration

FerrumGate config precedence:

```
CLI args > env vars > config file > defaults
```

### Key env vars

| Variable | Purpose | Example |
|----------|---------|---------|
| `FERRUMD_CONFIG` | Config file path | `/etc/ferrumgate/ferrumd.toml` |
| `FERRUMD_BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `FERRUMD_STORE_DSN` | Store DSN | `sqlite:/var/lib/ferrumgate/ferrumgate.db` |
| `FERRUMD_AUTH_MODE` | Auth mode | `Bearer` |
| `FERRUMD_BEARER_TOKEN` | Bearer token | `hex32-string` |
| `FERRUMD_LOG_FILTER` | Log filter | `info` |
| `FERRUMD_LOG_FORMAT` | Log format | `json` |
| `FERRUMD_RATE_LIMIT_PER_SECOND` | Rate limit | `2` |
| `FERRUMD_RATE_LIMIT_BURST` | Rate limit burst | `50` |
| `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND` | Allow non-local bind without TLS | `false` (default) |

### Config file example

```toml
[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "Bearer"
bearer_token = "<generate-with-openssl-rand-hex-32>"
log_format = "json"
rate_limit_per_second = 2
rate_limit_burst = 50

[store]
dsn = "sqlite:/var/lib/ferrumgate/ferrumgate.db"
```

### Local development config

The repository includes `configs/ferrumgate.dev.toml`:

- `auth_mode = "Disabled"`
- In-memory SQLite
- Loopback binding (`127.0.0.1:18080`)

This config auto-loads if no `--config` is specified and the file exists. **Never use dev config for production or exposed interfaces.**

---

## Deployment checklist

- [ ] Choose store backend (SQLite for pilot; PostgreSQL for production foundation).
- [ ] Generate bearer token with `openssl rand -hex 32`.
- [ ] Configure reverse proxy with TLS termination (nginx/Caddy).
- [ ] Set up systemd service with env file.
- [ ] Enable backup timer/cron.
- [ ] Configure AlertManager for off-VM alerting.
- [ ] Verify `/v1/readyz/deep` returns 200 before taking traffic.

### Local-vs-hosted caveats

| Concern | Local dev | Hosted / staging | Production |
|---------|-----------|------------------|------------|
| Store | In-memory SQLite | File-backed SQLite or PostgreSQL | PostgreSQL |
| Auth | Disabled | Bearer | Bearer |
| TLS | None | Reverse proxy TLS | Reverse proxy TLS + cert rotation |
| Backup | None | Manual or cron | Automated with retention |
| Monitoring | Logs only | Metrics endpoint + alerting | Metrics + alerting + SLO dashboards |
| Domain | `localhost` | DuckDNS / temporary | Real owned domain + DNS |

> **Block A**: Real owned domain and DNS are required for full G2 closure. DuckDNS is accepted for single-node SQLite pilot only. See [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md).

---

## Health checks

### Liveness

```bash
curl http://127.0.0.1:18080/v1/healthz
```

Expected: `{"status":"ok"}` (HTTP 200).

### Readiness

```bash
curl http://127.0.0.1:18080/v1/readyz
```

Expected: `{"status":"ready"}` (HTTP 200).

### Deep readiness

```bash
curl http://127.0.0.1:18080/v1/readyz/deep
```

Checks:
- Store health (can execute a test query)
- Write queue depth within threshold
- Connection pool not saturated

Expected: HTTP 200 with `ok` status. A non-200 here means the gateway should not receive traffic.

---

## Backup and restore

### Manual backup

```bash
ferrumctl backup --output /backups/ferrumgate-$(date +%Y%m%d-%H%M%S).db
```

### Verify backup

```bash
ferrumctl backup verify --file /backups/ferrumgate-YYYYMMDD-HHMMSS.db
```

### Restore

```bash
# Stop ferrumd
systemctl stop ferrumgate

# Restore database
ferrumctl restore --file /backups/ferrumgate-YYYYMMDD-HHMMSS.db --db-path /var/lib/ferrumgate/ferrumgate.db

# Start ferrumd
systemctl start ferrumgate

# Verify
ferrumctl health
```

> **Warning**: Restore overwrites the current database. Always verify backup integrity first. Never restore without explicit confirmation in production.

### SQLite WAL considerations

For file-backed SQLite, FerrumGate uses WAL mode with:
- `synchronous=NORMAL`
- `wal_autocheckpoint=1000`
- `busy_timeout=5000ms`

Backups should include both the `.db` and `-wal`/`-shm` files if taken while the process is running, or use `ferrumctl backup` which handles consistency.

### PostgreSQL backup

Use `pg_dump` or your hosting provider's backup mechanism. FerrumGate does not manage PostgreSQL backups internally.

---

## Token rotation

1. Generate new token on the target host (never print to logs):
   ```bash
   openssl rand -hex 32
   ```

2. Update config/env with new token.

3. Restart ferrumd.

4. Verify new token works (200) and old token fails (401):
   ```bash
   curl -H "Authorization: Bearer NEW_TOKEN" http://127.0.0.1:8080/v1/intents
   curl -H "Authorization: Bearer OLD_TOKEN" http://127.0.0.1:8080/v1/intents
   ```

5. Record rotation in audit log.

> **Note**: Token rotation has been validated on target host. See [`docs/implementation-path/artifacts/2026-05-17-sendgrid-rotation-evidence.md`](../../implementation-path/artifacts/2026-05-17-sendgrid-rotation-evidence.md) for related secret-rotation evidence.

---

## Incident response

1. **Check health**: `curl /v1/healthz` and `/v1/readyz/deep`
2. **Check metrics**: `curl /v1/metrics`
3. **Check logs**: `journalctl -u ferrumgate -n 500`
4. **Check backup age**: verify latest backup is within RPO
5. **If store unhealthy**: fail closed; do not bypass gateway
6. **If capability issue**: check provenance chain, do not reuse capabilities

### Common incident patterns

| Symptom | Likely cause | Action |
|---------|--------------|--------|
| `readyz/deep` 503 | Store unhealthy or queue backpressure | Check store connectivity and load |
| `metrics` shows high `write_queue_depth` | Write saturation | Scale to PostgreSQL or reduce burst |
| 401 on all workload endpoints | Token mismatch or auth mode changed | Verify `FERRUMD_AUTH_MODE` and token |
| 429 rate limited | Governor burst exceeded | Review `rate_limit_per_second` and `rate_limit_burst` |

---

## Monitoring

### Key metrics to alert on

| Metric | Threshold | Action |
|--------|-----------|--------|
| `ferrumgate_store_health_up` | 0 | Page immediately |
| `ferrumgate_write_queue_depth` | > 100 | Alert; may indicate overload |
| `ferrumgate_governance_errors_total` | spike | Investigate |
| Backup age | > RPO (15min) | Alert; verify backup timer |

### SLO/SLA

See [`slo-sla.md`](./slo-sla.md) for draft targets. Not yet ratified.

---

## Status caveat

> **production-ready = NO**. This guide describes intended operator procedures. Some CLI expansions (admin status, approval queue, token management) are planned but not yet implemented. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 6.

## PostgreSQL reconnect and recovery

> **Scope**: This section documents the **current** behavior of `sqlx::PgPool` reconnect. It is a runbook, not a guarantee of production resilience. PostgreSQL production deployment remains **NO**.

### What the pool does automatically

When FerrumGate uses PostgreSQL, `sqlx::PgPool` manages connections with the following built-in behavior:

- **Transparent reconnect on new acquisition**: If a connection is dropped (e.g., PostgreSQL restart, network blip), the next `pool.acquire()` attempts to create a fresh connection. The pool does this with an internal retry/backoff strategy; you do not need to restart `ferrumd` for new requests to recover.
- **Existing connections fail**: In-flight queries on a connection that was severed will return an error. The caller (gateway endpoint) surfaces that as a 503 or 500 depending on context.
- **No application-level circuit breaker**: There is no custom reconnect policy, no bounded retry with jitter, and no automatic fallback to read-only mode. Those are deferred to PG-5 HA design.

### Operator checks during and after a PostgreSQL outage

1. **During outage**:
   - `curl /v1/readyz/deep` will likely return 503 because the store health check cannot acquire a healthy connection.
   - Metrics `ferrumgate_store_health_up` drops to `0`.
   - Metrics `ferrumgate_store_pg_acquire_timeouts_total` may increment if the pool is exhausted.
   - The gateway is **fail-closed**: requests that need the store will fail rather than bypass governance.

2. **After PostgreSQL returns**:
   - Watch `ferrumgate_store_health_up` return to `1`.
   - Confirm `curl /v1/readyz/deep` returns HTTP 200.
   - Confirm `ferrumgate_store_pg_pool_idle` is > 0 (pool has recovered spare connections).
   - No `ferrumd` restart is required for basic recovery.

### When to restart `ferrumd`

Restart is **not** required for a transient PostgreSQL outage. Restart only if:

- The DSN or credentials changed (pool uses the original DSN; it does not reload config).
- `readyz/deep` stays unhealthy for longer than your incident-response threshold despite PostgreSQL being reachable.
- You observe a memory or connection leak that outlasts the outage.
- You are instructed to restart as part of a specific upgrade or config migration.

### Limitations

- Recovery speed depends on `sqlx` internals; there is no operator-tunable reconnect interval today.
- Pool saturation (`idle == 0 && size >= max`) is reported as degraded readiness, but no automatic scaling or queue shedding exists.
- These behaviors are validated locally with Docker Compose only, not on a production-like target host.

### Manual failover runbook

For the full procedure to promote a PostgreSQL standby and update ferrumd's DSN manually, see [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md). This is a planning artifact only; no live drill has been performed.

### Read replica design

For the design of read replica routing, consistency semantics, and observability, see [`docs/production-readiness-v2/read-replica-design.md`](../../production-readiness-v2/read-replica-design.md). This is a planning artifact only; no read replica code or deployment exists.

## PostgreSQL TLS/SSL DSN configuration

> **Scope**: Runbook guidance for operator-configured TLS between ferrumd and PostgreSQL. No live TLS validation performed.

### TLS modes

| Mode | Encryption | Certificate verification | When to use |
|------|-----------|--------------------------|-------------|
| `disable` | None | None | Never for production or exposed networks |
| `require` | Yes | None | Minimum for encrypted transport; acceptable within trusted VPC |
| `verify-ca` | Yes | CA only | Recommended default for production; verifies server cert against CA |
| `verify-full` | Yes | CA + hostname | Strongest; requires hostname in cert to match connection host |

### DSN format

```text
postgres://user:pass@host:5432/db?sslmode=verify-ca&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt
```

**Parameters**:
- `sslmode` — TLS mode (see table above)
- `sslrootcert` — Path to CA certificate file
- `sslcert` — Path to client certificate (for client cert auth)
- `sslkey` — Path to client private key (for client cert auth)

### File permissions

```bash
# CA certificate: readable by ferrumd user
sudo chown root:ferrumgate /etc/ferrumgate/certs/pg-ca.crt
sudo chmod 644 /etc/ferrumgate/certs/pg-ca.crt

# Client key: readable ONLY by ferrumd user
sudo chown ferrumgate:ferrumgate /etc/ferrumgate/certs/pg-client.key
sudo chmod 600 /etc/ferrumgate/certs/pg-client.key
```

### Rotation

Certificate rotation requires a `ferrumd` restart because the DSN is parsed once at startup. Plan rotation during a maintenance window:

1. Replace certificate files.
2. Restart `ferrumd`.
3. Verify `/v1/readyz/deep` returns 200.
4. Confirm `ferrumgate_store_pg_pool_idle` > 0.

> **Non-claim**: TLS guidance is runbook-only. No live TLS-encrypted PG connection has been validated with ferrumd.

## PgBouncer / connection pooling

> **Scope**: Runbook guidance for optional PgBouncer deployment. No live PgBouncer validation performed.

### When to add PgBouncer

- More than 2 ferrumd instances share one PostgreSQL.
- PostgreSQL `max_connections` is approached under normal load.
- Connection churn is observed in PG logs.

### Recommended configuration

```ini
; /etc/pgbouncer/pgbouncer.ini
[databases]
ferrumgate = host=localhost port=5432 dbname=ferrumgate

[pgbouncer]
listen_port = 6432
listen_addr = 127.0.0.1
auth_type = hba
auth_file = /etc/pgbouncer/userlist.txt
pool_mode = transaction
max_client_conn = 200
default_pool_size = 20
reserve_pool_size = 5
reserve_pool_timeout = 3
server_idle_timeout = 600
server_lifetime = 3600
```

**Key settings**:
- `pool_mode = transaction` — reuse connections per transaction (best for ferrumd's short queries).
- `default_pool_size` — set based on PG `max_connections` and number of PgBouncer instances.
- `server_idle_timeout` — close idle backends to free PG connections.

### ferrumd DSN with PgBouncer

```text
FERRUMD_STORE_DSN=postgres://user:pass@localhost:6432/ferrumgate?sslmode=require
```

> **Non-claim**: PgBouncer guidance is runbook-only. No live PgBouncer deployment has been validated with ferrumd.

## Alert deployment validation

> **Scope**: Operator-run validation of FerrumGate alert templates in a live Prometheus. No live validation performed in engineering environment.

### Quick validation checklist

1. **Syntax**: `promtool check rules configs/monitoring/ferrumgate-alerts.yaml`
2. **Deploy**: Copy `ferrumgate-alerts.yaml` to Prometheus rules directory; reload Prometheus.
3. **Verify state**: `curl http://<prometheus>:9090/api/v1/rules` — confirm `ferrumgate` group is present and rules are not unexpectedly firing.
4. **PG alerts (if PG backend active)**: Confirm `ferrumgate_store_pg_pool_max` is scraped and `FerrumGatePostgresMetricsAbsent` is inactive.
5. **Optional simulation**: In non-production, temporarily stop ferrumd and confirm AlertManager receives the alert.

### Full runbook

See [`configs/monitoring/README.md`](../../configs/monitoring/README.md) §"Alert Deployment Validation Runbook" for the complete procedure, evidence artifact template, and non-claims.

## Related docs

- [`hosted-deployment.md`](./hosted-deployment.md) — systemd, Docker, K8s deployment modes.
- [`slo-sla.md`](./slo-sla.md) — Draft SLO targets.
- [`troubleshooting.md`](./troubleshooting.md) — Common issues and fixes.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime config and stress baselines.
- [`api.md`](./api.md) — Endpoint reference.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-2.3b — Full deferred rationale.
