# Operator Guide

> **Parent**: [`guides/README.md`](./README.md)

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
| `FERRUMD_FS_WORKDIR` | Filesystem adapter sandbox root | `/var/lib/ferrumgate/workdir` |
| `FERRUMD_GIT_REPO_ROOTS` | Comma-separated Git repository parent roots | `/srv/repos,/var/lib/ferrumgate/repos` |
| `FERRUMD_SQLITE_DB_ROOTS` | Comma-separated SQLite database parent roots | `/var/lib/ferrumgate/databases` |
| `FERRUMD_LOG_FILTER` | Log filter | `info` |
| `FERRUMD_LOG_FORMAT` | Log format | `json` |
| `FERRUMD_RATE_LIMIT_PER_SECOND` | Rate limit | `2` |
| `FERRUMD_RATE_LIMIT_BURST` | Rate limit burst | `50` |
| `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND` | Allow non-local bind without TLS | `false` (default) |
| `FERRUMD_STORE_SYNCHRONOUS` | SQLite synchronous pragma | `NORMAL` |
| `FERRUMD_STORE_WAL_AUTOCHECKPOINT` | SQLite WAL autocheckpoint pages | `1000` |
| `FERRUMD_WRITE_QUEUE_THRESHOLD` | Write queue depth threshold for deep readiness | `100` |
| `FERRUMD_PG_MAX_CONNECTIONS` | PostgreSQL max connections | `10` |
| `FERRUMD_PG_MIN_IDLE` | PostgreSQL min idle connections | `1` |
| `FERRUMD_PG_ACQUIRE_TIMEOUT_SECS` | PostgreSQL connection acquire timeout | `5` |
| `FERRUMD_PG_STATEMENT_TIMEOUT_MS` | PostgreSQL statement timeout | `5000` |
| `FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS` | PostgreSQL idle-in-transaction timeout | `10000` |

OIDC env vars are listed in [`configs/examples/ferrumd.env.example`](../../configs/examples/ferrumd.env.example).

#### CLI client env vars

| Variable | Purpose | Example |
|----------|---------|---------|
| `FERRUMCTL_SERVER_URL` | Override ferrumctl server URL | `http://127.0.0.1:8080` |
| `FERRUMCTL_BEARER_TOKEN` | Bearer token for ferrumctl (should not be printed/logged) | `hex32-string` |

### Config file example

```toml
[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "Bearer"
bearer_token = "<generate-with-openssl-rand-hex-32>"
fs_workdir = "/var/lib/ferrumgate/workdir"
git_repo_roots = ["/var/lib/ferrumgate/repos"]
sqlite_db_roots = ["/var/lib/ferrumgate/databases"]
log_format = "json"
rate_limit_per_second = 2
rate_limit_burst = 50
store_dsn = "sqlite:///var/lib/ferrumgate/ferrumgate.db"
```

Supported auth modes: `disabled`, `bearer`, `scoped`, `oidc`, and `agent`. For agent auth setup, see [`docs/security/agent-identity-ed25519.md`](../security/agent-identity-ed25519.md).

### Local development config

The repository includes `configs/ferrumgate.dev.toml`:

- `auth_mode = "Disabled"`
- In-memory SQLite
- Loopback binding (`127.0.0.1:8080`)

This config auto-loads if no `--config` is specified and the file exists. **Never use dev config for exposed interfaces.**

---

## Deployment checklist

- [ ] Choose store backend (SQLite for local use; PostgreSQL for higher throughput).
- [ ] Generate bearer token with `openssl rand -hex 32`.
- [ ] Set `fs_workdir` / `FERRUMD_FS_WORKDIR` for any non-loopback production-like deployment.
- [ ] Set Git and SQLite root allowlists before enabling their mutation adapters.
- [ ] Configure reverse proxy with TLS termination (nginx/Caddy).
- [ ] Set up systemd service with env file.
- [ ] Enable backup timer/cron.
- [ ] Configure AlertManager for off-VM alerting.
- [ ] Verify `/v1/readyz/deep` returns 200 before taking traffic.

### Local-vs-hosted caveats

| Concern | Local dev | Hosted / staging | Shared / managed |
|---------|-----------|------------------|-------------------|
| Store | In-memory SQLite | File-backed SQLite or PostgreSQL | PostgreSQL |
| Auth | Disabled | Bearer | Bearer |
| TLS | None | Reverse proxy TLS | Reverse proxy TLS + cert rotation |
| Backup | None | Manual or cron | Automated with retention |
| Monitoring | Logs only | Metrics endpoint + alerting | Metrics + alerting + dashboards |
| Domain | `localhost` | Temporary / dynamic DNS | Real owned domain + DNS |

> **Note**: Real owned domain and DNS are recommended for shared deployments. See [`docs/PRODUCTION_NOTES.md`](../../docs/PRODUCTION_NOTES.md).

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
TOKEN="${FERRUMD_BEARER_TOKEN:?set bearer token}"
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/readyz/deep
```

Checks:
- Store health (can execute a test query)
- Write queue depth within threshold
- Connection pool not saturated

Expected: HTTP 200 with `ok` status. A non-200 here means the gateway should not receive traffic.

### TUI dashboard

For a lightweight terminal view of the same endpoints:

```bash
ferrum-tui --server-url http://127.0.0.1:18080
```

The TUI shows:
- Configured base URL and token presence (redacted)
- Live status of `/v1/healthz`, `/v1/readyz`, and `/v1/readyz/deep`
- Per-endpoint latency
- Auto-refresh every 5 seconds

Keyboard shortcuts:
- `r` — refresh now
- `?` / `h` — toggle help
- `q` — quit

> **Scope**: Operator convenience only. See `bins/ferrum-tui/README.md` for details.

---

## Lifecycle outbox operator review

FerrumGate marks lifecycle records as `NeedsOperatorReview` when automatic provenance reconciliation cannot safely repair state, for example missing or ambiguous parent provenance, execution state drift, rollback state drift, or repeated reconciliation failures.

### Inspect records

```bash
ferrumctl admin lifecycle-outbox list --status needs_operator_review --limit 50
ferrumctl admin lifecycle-outbox get <outbox-id>
```

Review `last_error`, `attempt_count`, `previous_*_state`, `next_*_state`, `provenance_obligations`, and the linked execution/rollback identifiers before taking action.

### Retry after fixing data

Use retry only after the underlying issue has been corrected, such as restoring a missing provenance parent or resolving a temporary store failure:

```bash
ferrumctl admin lifecycle-outbox retry <outbox-id> \
  --actor-id "<operator-id>" \
  --reason "restored missing parent provenance event"
```

Retry resets the record to pending reconciliation and emits an audit trail with the operator actor and reason.

### Resolve manually

Use resolve only when the operator has verified the lifecycle state externally and automatic repair should not run again:

```bash
ferrumctl admin lifecycle-outbox resolve <outbox-id> \
  --actor-id "<operator-id>" \
  --reason "verified execution terminal state against external audit log"
```

Resolution requires a non-empty reason. Keep the reason specific enough for incident review.

### Post-action checks

```bash
TOKEN="${FERRUMD_BEARER_TOKEN:?set bearer token}"
curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/readyz/deep
curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/metrics | grep ferrumgate_lifecycle_outbox
```

Expected after remediation: `ferrumgate_lifecycle_outbox_operator_review` returns to `0`, expired leases do not grow, and deep readiness no longer reports lifecycle outbox degradation.

---

## Backup and restore

### Manual backup

```bash
ferrumctl backup create --db-path /var/lib/ferrumgate/ferrumgate.db --output-dir /backups
```

### Verify backup

```bash
ferrumctl backup verify --db-path /backups/ferrumgate-YYYYMMDD-HHMMSS.db
```

### Restore

```bash
# Stop ferrumd
systemctl stop ferrumgate

# Restore database
ferrumctl backup restore --db-path /var/lib/ferrumgate/ferrumgate.db --from /backups/ferrumgate-YYYYMMDD-HHMMSS.db --confirm

# Start ferrumd
systemctl start ferrumgate

# Verify
ferrumctl health
```

> **Warning**: Restore overwrites the current database. Always verify backup integrity first. Never restore without explicit confirmation on a live system.

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

### Using ferrumctl (recommended)

```bash
ferrumctl admin tokens rotate <TOKEN_ID> --reason "rotation" --expires-in-days 30
```

### Using the API directly

1. Generate new token on the target host (never print to logs):
   ```bash
   openssl rand -hex 32
   ```

2. Update config/env with new token.

3. Restart ferrumd.

4. Verify new token works (200) and old token fails (401):
   ```bash
    curl -H "Authorization: Bearer ${FERRUMD_BEARER_TOKEN}" http://127.0.0.1:8080/v1/intents
    curl -H "Authorization: Bearer ${OLD_TOKEN}" http://127.0.0.1:8080/v1/intents
   ```

5. Record rotation in audit log.

> **Note**: Token rotation procedures are documented below.

---

## Incident response

> For the full incident runbook, see [`docs/operations/runbook.md`](../../docs/operations/runbook.md).

1. **Check health**: `/v1/healthz` and `/v1/readyz` are public; `/v1/readyz/deep` requires bearer auth when auth is enabled.
2. **Check metrics**: `/v1/metrics` requires bearer auth when auth is enabled.
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

### Service metrics

See [Service Metrics](./slo-sla.md) for observability baselines.

---

## PostgreSQL reconnect and recovery

> **Scope**: This section documents the **current** behavior of `sqlx::PgPool` reconnect. It is a runbook.

### What the pool does automatically

When FerrumGate uses PostgreSQL, `sqlx::PgPool` manages connections with the following built-in behavior:

- **Transparent reconnect on new acquisition**: If a connection is dropped (e.g., PostgreSQL restart, network blip), the next `pool.acquire()` attempts to create a fresh connection. The pool does this with an internal retry/backoff strategy; you do not need to restart `ferrumd` for new requests to recover.
- **Existing connections fail**: In-flight queries on a connection that was severed will return an error. The caller (gateway endpoint) surfaces that as a 503 or 500 depending on context.
- **No application-level circuit breaker**: There is no custom reconnect policy, no bounded retry with jitter, and no automatic fallback to read-only mode.

### Operator checks during and after a PostgreSQL outage

1. **During outage**:
   - `curl -H "Authorization: Bearer $TOKEN" /v1/readyz/deep` will likely return 503 because the store health check cannot acquire a healthy connection.
   - Metrics `ferrumgate_store_health_up` drops to `0`.
   - Metrics `ferrumgate_store_pg_acquire_timeouts_total` may increment if the pool is exhausted.
   - The gateway is **fail-closed**: requests that need the store will fail rather than bypass governance.

2. **After PostgreSQL returns**:
   - Watch `ferrumgate_store_health_up` return to `1`.
   - Confirm `curl -H "Authorization: Bearer $TOKEN" /v1/readyz/deep` returns HTTP 200.
   - Confirm `ferrumgate_store_pg_pool_idle` is > 0 (pool has recovered spare connections).
   - No `ferrumd` restart is required for basic recovery.

### When to restart `ferrumd`

Restart is **not** required for a transient PostgreSQL outage. Restart only if:

- The DSN or credentials changed (pool uses the original DSN; it does not reload config).
- `readyz/deep` stays unhealthy for longer than your incident-response threshold despite PostgreSQL being reachable.
- You observe a memory or connection leak that outlasts the outage.
- You are instructed to restart as part of a specific upgrade or config migration.
- For full upgrade procedures including zero-downtime tradeoffs, maintenance-window requirements, and rollback procedures, see [`docs/guides/zero-downtime-upgrade.md`](./zero-downtime-upgrade.md).

### Limitations

- Recovery speed depends on `sqlx` internals; there is no operator-tunable reconnect interval.
- Pool saturation (`idle == 0 && size >= max`) is reported as degraded readiness, but no automatic scaling or queue shedding exists.
- These behaviors are validated locally with Docker Compose.

### Manual failover runbook

For the full procedure to promote a PostgreSQL standby and update ferrumd's DSN manually, consult the hosted deployment guide. Manual failover is documented; operator must validate in their environment.

### Read replica design

For the design of read replica routing, consistency semantics, and observability, consult the hosted deployment guide. Read replica design is documented; no implementation is provided.

## PostgreSQL TLS/SSL DSN configuration

> **Scope**: Operator-configured TLS between ferrumd and PostgreSQL.

### TLS modes

| Mode | Encryption | Certificate verification | When to use |
|------|-----------|--------------------------|-------------|
| `disable` | None | None | Never for exposed networks |
| `require` | Yes | None | Minimum for encrypted transport; acceptable within trusted VPC |
| `verify-ca` | Yes | CA only | Recommended default for live use; verifies server cert against CA |
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
3. Verify `/v1/readyz/deep` returns 200 with bearer auth.
4. Confirm `ferrumgate_store_pg_pool_idle` > 0.

> **Note**: TLS guidance for operator configuration.

## PgBouncer / connection pooling

> **Scope**: Optional PgBouncer deployment guidance.

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

> **Note**: PgBouncer deployment guidance.

## Alert deployment validation

> **Scope**: Validation of FerrumGate alert templates in a live Prometheus environment.

### Quick validation checklist

1. **Syntax**: `promtool check rules configs/monitoring/ferrumgate-alerts.yaml`
2. **Deploy**: Copy `ferrumgate-alerts.yaml` to Prometheus rules directory; reload Prometheus.
3. **Verify state**: `curl http://<prometheus>:9090/api/v1/rules` — confirm `ferrumgate` group is present and rules are not unexpectedly firing.
4. **PG alerts (if PG backend active)**: Confirm `ferrumgate_store_pg_pool_max` is scraped and `FerrumGatePostgresMetricsAbsent` is inactive.
5. **Optional simulation**: In a test environment, temporarily stop ferrumd and confirm AlertManager receives the alert.

### Full runbook

See [`configs/monitoring/README.md`](../../configs/monitoring/README.md) §"Alert Deployment Validation Runbook" for the complete procedure, evidence artifact template, and notes.

## Related docs

- [`hosted-deployment.md`](./hosted-deployment.md) — systemd, Docker, K8s deployment modes.
- [Service Metrics](./slo-sla.md) — Observability baselines.
- [`troubleshooting.md`](./troubleshooting.md) — Common issues and fixes.
- [`docs/PRODUCTION_NOTES.md`](../../docs/PRODUCTION_NOTES.md) — Runtime config and stress baselines.
- [`api.md`](./api.md) — Endpoint reference.
