# Hosted Deployment Guide

> **Status**: Scaffold. Docker Compose local exists; target-host deployment docs are planned.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Deployment modes

### Mode A — Local demo (development only)

```bash
# SQLite in-memory, auth disabled, loopback only
cargo run --bin ferrumd
```

Purpose: quickstart, demos, development.

> **Not for production. Do not expose to the internet.**

### Mode B — Single-node self-hosted (conditional pilot)

Components:
- ferrumd + SQLite persistent
- systemd service
- nginx/Caddy TLS reverse proxy
- backup timer

Purpose: conditional pilot, small internal deployments.

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
FERRUMD_STORE_DSN=sqlite:/var/lib/ferrumgate/ferrumgate.db
FERRUMD_AUTH_MODE=Bearer
FERRUMD_BEARER_TOKEN=<generate-with-openssl-rand-hex-32>
FERRUMD_LOG_FORMAT=json
```

Enable:

```bash
systemctl daemon-reload
systemctl enable --now ferrumgate
```

### Mode C — PostgreSQL self-hosted (production foundation)

Components:
- ferrumd + PostgreSQL
- systemd or Docker Compose
- backup/restore
- metrics

Purpose: production foundation.

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

> **Note**: PostgreSQL production hardening is planned. See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md).

### Mode D — Kubernetes (future)

Components:
- ferrumd Deployment
- PostgreSQL external or managed
- Secret, ConfigMap, Service, Ingress
- Prometheus ServiceMonitor

Purpose: hosted production-like.

> **Not yet implemented.** Helm chart is a P1/P2 item. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 8.

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

See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) for:
- Connection hardening
- Metrics and alerts
- Backup/restore evidence
- Schema migration discipline
- HA roadmap

## Backup / restore in hosted mode

### SQLite

Use `ferrumctl backup` and `ferrumctl restore`. See [`operator.md`](./operator.md).

### PostgreSQL

Use `pg_dump` / `pg_restore` with retention pruning. See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §Phase PG-3.

## Status caveat

> **production-ready = NO**. Mode B is the only validated deployment for conditional pilot. Mode C requires PG hardening before production claim. Mode D is not implemented. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 8.

## Related docs

- [`operator.md`](./operator.md) — Config, backup, incident response.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PG hardening plan.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime configuration.
