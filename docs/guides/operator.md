# Operator Guide

> **Status**: Scaffold. ferrumctl exists; admin CLI expansion is planned.
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

## Deployment checklist

- [ ] Choose store backend (SQLite for pilot; PostgreSQL for production foundation).
- [ ] Generate bearer token with `openssl rand -hex 32`.
- [ ] Configure reverse proxy with TLS termination (nginx/Caddy).
- [ ] Set up systemd service with env file.
- [ ] Enable backup timer/cron.
- [ ] Configure AlertManager for off-VM alerting.
- [ ] Verify `/v1/readyz/deep` returns 200 before taking traffic.

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

## Token rotation

1. Generate new token on the target host (never print to logs):
   ```bash
   openssl rand -hex 32
   ```

2. Update config/env with new token.

3. Restart ferrumd.

4. Verify new token works (200) and old token fails (401).

5. Record rotation in audit log.

## Incident response

1. **Check health**: `curl /v1/healthz` and `/v1/readyz/deep`
2. **Check metrics**: `curl /v1/metrics`
3. **Check logs**: `journalctl -u ferrumgate -n 500`
4. **Check backup age**: verify latest backup is within RPO
5. **If store unhealthy**: fail closed; do not bypass gateway
6. **If capability issue**: check provenance chain, do not reuse capabilities

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

## Status caveat

> **production-ready = NO**. This guide describes intended operator procedures. Some CLI expansions (admin status, approval queue, token management) are planned but not yet implemented. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 6.

## Related docs

- [`hosted-deployment.md`](./hosted-deployment.md) — systemd, Docker, K8s deployment modes.
- [`slo-sla.md`](./slo-sla.md) — Draft SLO targets.
- [`troubleshooting.md`](./troubleshooting.md) — Common issues and fixes.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime config and stress baselines.
