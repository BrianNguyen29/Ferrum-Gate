# Hosted Deployment Guide

This guide covers deployment modes for FerrumGate.

---

## Deployment modes

### Mode A — Local demo (development only)

```bash
# SQLite in-memory, auth disabled, loopback only
cargo run --bin ferrumd
```

Or use Docker Compose:

```bash
docker compose -f docker-compose.demo.yml up -d --build
```

Purpose: quickstart, demos, development.

> **Local demo only. Do not expose to the internet.**

### Mode B — Single-node self-hosted

Components:
- ferrumd + SQLite persistent
- systemd service
- nginx/Caddy TLS reverse proxy
- backup timer

Purpose: small internal deployments.

#### systemd service example

Create `/etc/systemd/system/ferrumgate.service`:

```ini
[Unit]
Description=FerrumGate Governance Gateway
After=network.target

[Service]
Type=simple
User=ferrumgate
Group=ferrumgate
EnvironmentFile=/etc/ferrumgate/ferrumgate.env
ExecStart=/opt/ferrumgate/ferrumd --config /etc/ferrumgate/ferrumd.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Create `/etc/ferrumgate/ferrumgate.env`:

```bash
FERRUMD_STORE_DSN=sqlite:///var/lib/ferrumgate/ferrumgate.db
FERRUMD_AUTH_MODE=Bearer
FERRUMD_BEARER_TOKEN=<generate-with-openssl-rand-hex-32>
FERRUMD_LOG_FORMAT=json
```

Enable:

```bash
systemctl daemon-reload
systemctl enable --now ferrumgate
```

> **Note**: A temporary domain may be used for local testing. A real owned domain and DNS configuration are recommended for internet-facing deployments.

### Mode C — PostgreSQL self-hosted

Components:
- ferrumd + PostgreSQL
- systemd or Docker Compose
- backup/restore
- metrics

Purpose: deployments requiring higher write throughput or stronger transactional guarantees.

#### Docker Compose (PostgreSQL)

```yaml
version: "3.8"
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: ferrumgate
      POSTGRES_USER: ferrumgate
      POSTGRES_PASSWORD: <strong-password>
    volumes:
      - pgdata:/var/lib/postgresql/data

  ferrumd:
    image: ferrumgate:latest
    environment:
      FERRUMD_STORE_DSN: postgres://ferrumgate:<strong-password>@postgres:5432/ferrumgate
      FERRUMD_AUTH_MODE: Bearer
      FERRUMD_BEARER_TOKEN: <token>
    ports:
      - "8080:8080"
    depends_on:
      - postgres

volumes:
  pgdata:
```

> **Note**: A local demo compose file (`docker-compose.postgres-demo.yml`) exists for development only. PostgreSQL hardening is the operator's responsibility.

### Mode D — Kubernetes

Components:
- ferrumd Deployment
- PostgreSQL external or managed
- Secret, ConfigMap, Service, Ingress
- Prometheus ServiceMonitor

Purpose: shared scalable deployment.

## Reverse proxy / TLS

FerrumGate does not terminate TLS. Deploy behind a reverse proxy.

### Caddy example

```
ferrumgate.example.com {
    reverse_proxy localhost:8080
}
```

### nginx example

```nginx
server {
    listen 443 ssl;
    server_name ferrumgate.example.com;

    ssl_certificate /etc/letsencrypt/live/ferrumgate.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ferrumgate.example.com/privkey.pem;

    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## PostgreSQL deployment

For PostgreSQL deployment details, see:
- Connection hardening
- Metrics and alerts
- Backup/restore procedures
- Schema migration discipline

### Quick validation checklist (PostgreSQL mode)

1. `systemctl status postgresql@16-main` — active
2. `pg_isready -h localhost -p 5432` — accepting connections
3. `curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/readyz/deep` — store, write_queue, pool all healthy
4. `curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_pool_max` — non-zero
5. `promtool check rules /etc/prometheus/rules/ferrumgate-postgres-alerts.yml` — syntax pass

## Managed PostgreSQL guide

If you are using a managed PostgreSQL service (e.g., Amazon RDS, Google Cloud SQL, Azure Database) or a self-managed instance, keep the following in mind:

- **Do not inline passwords in environment files**. Use `PGPASSFILE` (mode `600`) or your secrets manager. See [`configs/examples/postgres-target-env.template`](../../configs/examples/postgres-target-env.template) for a placeholder-only env template.
- **Connect via PgBouncer or direct TLS**. If the managed instance supports TLS, prefer `sslmode=require` or `verify-ca` in the DSN and keep certificates in `/etc/ferrumgate/certs/`.
- **Set conservative pool limits**. Managed instances often enforce connection limits; align `FERRUMD_PG_MAX_CONNECTIONS` with the instance size.
- **Backups are operator-owned**. Managed services usually provide automated backups, but you should still test restore procedures and validate row counts independently.

## Backup / restore in hosted mode

### SQLite

Use `ferrumctl backup` and `ferrumctl restore`. See [`operator.md`](./operator.md).

### PostgreSQL

Use `pg_dump` / `pg_restore` with retention pruning.

### PostgreSQL rollback / validation commands

Before any upgrade or config migration:

```bash
# 1. Capture a fresh backup
sudo -u postgres pg_dump -Fc ferrumgate \
  -f /var/backups/ferrumgate-postgres/ferrumgate-$(date +%Y%m%d-%H%M%S).dump

# 2. Verify backup is listable
pg_restore -l /var/backups/ferrumgate-postgres/ferrumgate-*.dump > /dev/null \
  && echo "LISTABLE=PASS"

# 3. Stop ferrumd
sudo systemctl stop ferrumgate

# 4. Restore to a drill database first (do not overwrite live data)
sudo -u postgres pg_restore -d ferrumgate_restore_drill \
  /var/backups/ferrumgate-postgres/ferrumgate-*.dump

# 5. Verify row counts match
# 6. Restart ferrumd
sudo systemctl start ferrumgate

# 7. Validate deep readiness
TOKEN="${FERRUMD_BEARER_TOKEN:?set bearer token}"
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/readyz/deep | jq .
```

> **Upgrade guidance**: For full upgrade procedures including zero-downtime tradeoffs, maintenance-window requirements, and rollback procedures, see [`docs/guides/zero-downtime-upgrade.md`](./zero-downtime-upgrade.md).

### Automated backup scheduling

FerrumGate does not run a backup scheduler itself. Use the host scheduler (systemd timer or cron) with the example units below. Review paths, users, and retention before installing.

**systemd timer examples**

- PostgreSQL (`pg_dump` every 15 min): [`configs/examples/postgres-backup.timer`](../../configs/examples/postgres-backup.timer) + [`configs/examples/postgres-backup.service`](../../configs/examples/postgres-backup.service)
- SQLite (daily): [`configs/examples/ferrumgate-backup.timer`](../../configs/examples/ferrumgate-backup.timer) + [`configs/examples/ferrumgate-backup.service`](../../configs/examples/ferrumgate-backup.service)

**cron examples**

- PostgreSQL: [`configs/examples/postgres-backup.cron`](../../configs/examples/postgres-backup.cron)
- SQLite: [`configs/examples/ferrumgate-backup.cron`](../../configs/examples/ferrumgate-backup.cron)

> **Operator-owned**: These examples are templates. Do not install them without reviewing credentials, paths, and retention. No scheduler is active by default.

## HA operational notes

| Item | Support |
|------|---------|
| Same-VM primary/standby streaming replication | Supported |
| Same-VM automated failover (watchdog + PgBouncer reconnect) | Supported |
| Multi-host manual failover/failback drills | Documented |
| Multi-host clustering | Operator-designed |

### What operators should know

- **Failover is manual or operator-controlled** outside of same-VM scope.
- **Fencing**: GCP instance-stop script exists and was validated on standby host B only. App-host guard blocks primary fencing by default.
- **Failback**: Rebuilding the old primary as standby requires matching WAL settings (`max_wal_senders`, `max_replication_slots`) and TLS cert parity.

## Deployment feature matrix

**Legend:**

| Symbol | Meaning |
|--------|---------|
| ✅ | Supported |
| ⚠️ | Conditional / partial / operator-owned |
| ❌ | Unsupported / not applicable |
| ⏳ | Planned |

**Feature matrix by deployment mode:**

| Feature | Mode A<br>Local demo | Mode B<br>Single-node SQLite | Mode C<br>PostgreSQL self-hosted | Mode D<br>Kubernetes/Helm (planned) |
|---------|:---:|:---:|:---:|:---:|
| **Intended use** | Development / quickstart only | Small internal deployments | Shared deployments requiring higher throughput | Shared scalable deployment |
| **Auth bearer** | ❌ Disabled (loopback) | ✅ Supported | ✅ Supported | ✅ Supported |
| **Scoped tokens / RBAC** | ⚠️ Present but not enforced in dev mode | ⚠️ Present; operator-owned enforcement | ⚠️ Present; operator-owned enforcement | ⚠️ Present; operator-owned enforcement |
| **TLS termination / domain** | ❌ None (loopback only) | ⚠️ Operator-owned reverse proxy required | ⚠️ Operator-owned reverse proxy required | ⚠️ Operator-owned reverse proxy + Ingress required |
| **Persistent storage** | ❌ In-memory only | ✅ SQLite on filesystem | ✅ PostgreSQL (local or managed) | ⏳ PostgreSQL external / managed |
| **Backup / restore** | ❌ Not applicable | ⚠️ `ferrumctl backup`; operator-owned scheduler | ⚠️ `pg_dump` / `pg_restore`; operator-owned scheduler | ⏳ Operator-owned |
| **Health / readiness endpoints** | ✅ `/healthz`, `/readyz` | ✅ `/healthz`, `/readyz`; `/readyz/deep` requires auth | ✅ `/healthz`, `/readyz`; `/readyz/deep` requires auth | ✅ Service health probes |
| **Metrics / observability** | ⚠️ Prometheus metrics endpoint | ⚠️ Metrics require auth when auth is enabled | ✅ Authenticated metrics + PG-specific alerts | ⏳ ServiceMonitor |
| **Grafana dashboards** | ❌ Not included | ⚠️ Operator-owned | ⚠️ Operator-owned | ⏳ Planned |
| **PostgreSQL support** | ❌ Not applicable | ❌ SQLite only | ✅ Native | ✅ External / managed |
| **Kubernetes / Helm support** | ❌ Not applicable | ❌ Not applicable | ❌ Not applicable | ⏳ Helm chart planned item |
| **Zero-downtime upgrade path** | ❌ Not applicable | ⚠️ Maintenance window required; see [`./zero-downtime-upgrade.md`](./zero-downtime-upgrade.md) | ⚠️ Maintenance window required; see [`./zero-downtime-upgrade.md`](./zero-downtime-upgrade.md) | ⏳ Planned |

**Links:**

- [Service Metrics](./slo-sla.md)
- Zero-downtime upgrade: [`docs/guides/zero-downtime-upgrade.md`](./zero-downtime-upgrade.md)

## Related docs

- [`operator.md`](./operator.md) — Config, backup, incident response.
- [`docs/PRODUCTION_NOTES.md`](../../docs/PRODUCTION_NOTES.md) — Runtime configuration.
