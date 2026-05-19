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

## Related docs

- [`hosted-deployment.md`](./hosted-deployment.md) — systemd, Docker, K8s deployment modes.
- [`slo-sla.md`](./slo-sla.md) — Draft SLO targets.
- [`troubleshooting.md`](./troubleshooting.md) — Common issues and fixes.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime config and stress baselines.
- [`api.md`](./api.md) — Endpoint reference.
