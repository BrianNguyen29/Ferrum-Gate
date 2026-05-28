# Hosted Deployment Guide

> **Status**: Expanded guide. Tier 1.5 PostgreSQL target deployment and HA manual-drill evidence incorporated.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

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

> **DEP-4 status**: Target-host systemd runtime validated on `ferrumgate-nonprod` with evidence at [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md). Service name: `ferrumgate.service`. This is **not** a production-ready claim.
>
> **Block A / DuckDNS context**: DuckDNS was accepted by the operator on 2026-05-17 for single-node SQLite pilot only. A real owned domain and DNS configuration are still required for production-ready status or full G2 closure. Block A remains **WAIVED/CONDITIONAL**.

### Mode C — PostgreSQL self-hosted (production foundation)

Components:
- ferrumd + PostgreSQL
- systemd or Docker Compose
- backup/restore
- metrics

Purpose: production foundation.

#### Tier 1.5 PostgreSQL deployment baseline

As of 2026-05-27, a Tier 1.5 target VM deployment exists with the following baseline. This is **not** a production-ready claim; it is an operator-environment reference.

| Component | Baseline |
|-----------|----------|
| PostgreSQL | 16.14 (Ubuntu 16.14-1.pgdg24.04+1) |
| PgBouncer | 1.25.2 (transaction mode, localhost:6432) |
| TLS | Self-signed CA, TLSv1.3, TLS_AES_256_GCM_SHA384 |
| Backup | `pg_dump -Fc` every 15 min, 4-day retention, GCS offsite via `gsutil rsync` |
| Alerts | 5 PG-specific Prometheus alert rules (metrics absent, pool saturation, slow acquire, backup stale, store unhealthy) |
| ferrumd DSN | Via PgBouncer (`127.0.0.1:6432`) or direct TLS PG (`localhost:5432`) |

Evidence:
- PG target deployment: [`docs/implementation-path/artifacts/2026-05-27-pg-target-deployment-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-target-deployment-evidence.md)
- TLS DSN: [`docs/implementation-path/artifacts/2026-05-27-pg-tls-dsn-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-tls-dsn-evidence.md)
- PgBouncer: [`docs/implementation-path/artifacts/2026-05-27-pg-pgbouncer-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-pgbouncer-evidence.md)
- Backup/restore: [`docs/implementation-path/artifacts/2026-05-27-pg-restore-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-restore-drill-evidence.md)
- Alert deployment: [`docs/implementation-path/artifacts/2026-05-27-pg-alert-deployment-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-alert-deployment-evidence.md)

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

> **Note**: A local demo compose file (`docker-compose.postgres-demo.yml`) exists for development only. It is NOT production-ready. PostgreSQL production hardening is planned. See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md).

### Mode D — Kubernetes (future)

Components:
- ferrumd Deployment
- PostgreSQL external or managed
- Secret, ConfigMap, Service, Ingress
- Prometheus ServiceMonitor

Purpose: hosted production-like.

> **Not yet implemented.** Helm chart is a P1/P2 item. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 8.

## Tier readiness context

| Tier | What it means | Domain required? |
|------|---------------|------------------|
| Tier 0 — Conditional pilot | Single-node SQLite with operator conditional signoff | No (DuckDNS accepted) |
| Tier 1 — Domainless production-candidate | B+C+HA-B engineering complete; credible candidate once domain added | No |
| Tier 1.5 — Domainless production infrastructure | PG target deployment + same-VM HA + same-VM automated failover complete | No |
| Tier 2 — Production-ready | Real domain, revalidation, sustained SLO, full G2, final signoff | **Yes** |

> **Block A = WAIVED/CONDITIONAL**. A real owned domain and DNS are still required for Tier 2 or full G2 closure. Tier 1.5 does not close Block A.
>
> **production-ready = NO** at all tiers below Tier 2.

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

### Quick validation checklist (PostgreSQL mode)

1. `systemctl status postgresql@16-main` — active
2. `pg_isready -h localhost -p 5432` — accepting connections
3. `curl http://localhost:8080/v1/readyz/deep` — store, write_queue, pool all healthy
4. `curl http://localhost:8080/v1/metrics | grep ferrumgate_store_pg_pool_max` — non-zero
5. `promtool check rules /etc/prometheus/rules/ferrumgate-postgres-alerts.yml` — syntax pass

## Backup / restore in hosted mode

### SQLite

Use `ferrumctl backup` and `ferrumctl restore`. See [`operator.md`](./operator.md).

### PostgreSQL

Use `pg_dump` / `pg_restore` with retention pruning. See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §Phase PG-3.

> **DEP-6 status**: Hosted single-node SQLite temp-copy restore drill passed with evidence at [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md). This is **not** production-ready. PostgreSQL production backup/restore is **not** claimed. A hosted backup-mode planning/preflight checklist is prepared at [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md).

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

# 4. Restore to a drill database first (do not overwrite production)
sudo -u postgres pg_restore -d ferrumgate_restore_drill \
  /var/backups/ferrumgate-postgres/ferrumgate-*.dump

# 5. Verify row counts match
# 6. Restart ferrumd
sudo systemctl start ferrumgate

# 7. Validate deep readiness
curl -s http://localhost:8080/v1/readyz/deep | jq .
```

> **PG-P.5 evidence**: Restore drill passed on target VM with row-count and hash verification. See [`docs/implementation-path/artifacts/2026-05-27-pg-restore-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-pg-restore-drill-evidence.md).

## HA operational posture

FerrumGate's HA story is staged. Do not conflate local simulation or manual drills with production HA.

| Stage | Status | Evidence |
|-------|--------|----------|
| Same-VM primary/standby streaming replication | ✅ Tier 1.5 complete | [`2026-05-27-ha-streaming-replication-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-streaming-replication-evidence.md) |
| Same-VM automated failover (watchdog + PgBouncer reconnect) | ✅ Tier 1.5 complete | [`2026-05-27-ha-automated-failover-signoff.md`](../../implementation-path/artifacts/2026-05-27-ha-automated-failover-signoff.md) |
| Multi-host manual failover/failback drills | ✅ Phase 9 manual evidence | [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-multihost-drill-evidence.md) |
| Host B PgBouncer/ferrumd redundancy + bounded fenced drill | ✅ Phase 9 operator-controlled | [`2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md`](../../implementation-path/artifacts/2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md) |
| Multi-host production HA | ❌ NOT COMPLETE | HA-4 deferred; no unattended automated failover |

### What operators should know

- **Failover is manual or operator-controlled** outside of Tier 1.5 same-VM scope.
- **RPO/RTO measured** in manual drills: RPO 0 marker loss; RTO improved from 246s (Drill 1) to 22s (Drill 4) after TLS/config parity fixes.
- **Fencing**: GCP instance-stop script exists and was validated on standby host B only. App-host guard blocks primary fencing by default.
- **Failback**: Rebuilding the old primary as standby requires matching WAL settings (`max_wal_senders`, `max_replication_slots`) and TLS cert parity.

## Status caveat

> **production-ready = NO**. Mode B is the only validated deployment for conditional pilot. Mode C has Tier 1.5 target evidence but is not production-ready. Mode D is not implemented. Multi-host production HA and unattended automated failover are NOT COMPLETE. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 8 and Phase 9.

## Non-claims

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** |
| **full G2** | **NOT COMPLETE** |
| **Block A** | **WAIVED/CONDITIONAL** — real domain still required for Tier 2 |
| **Tier 2** | **NOT COMPLETE** |
| **multi-host production HA** | **NO** |
| **HA-4 unattended automated failover** | **NOT COMPLETE** |
| **sustained SLO window** | **NO** |

## Related docs

- [`operator.md`](./operator.md) — Config, backup, incident response.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PG hardening plan.
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Runtime configuration.
- [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md`](../../implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md) — DEP-4 target-host systemd runbook (prepared).
- [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md) — DEP-4 target-host systemd evidence (captured; not production-ready).
- [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md) — DEP-6 hosted single-node SQLite temp-copy restore evidence (captured; not production-ready).
- [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md`](../../implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md) — DEP-6 hosted backup preflight checklist (prepared).
- [`docs/implementation-path/artifacts/2026-05-27-pg-production-deployment-signoff.md`](../../implementation-path/artifacts/2026-05-27-pg-production-deployment-signoff.md) — Tier 1.5 PG deployment consolidated signoff.
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — HA roadmap and Phase 9 evidence.
- [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md) — Manual failover runbook (planning artifact).
