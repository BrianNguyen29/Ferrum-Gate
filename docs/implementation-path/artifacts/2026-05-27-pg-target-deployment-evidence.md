# PostgreSQL Target Deployment Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-target-deployment-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 1 — PG-P.1 (PostgreSQL provisioned) + PG-P.2 (ferrumd with production DSN)
> **Constraint**: Target VM deployment, not local Docker. No production-ready claim.

---

## 1. Summary

This artifact records the successful deployment of PostgreSQL 16 on the ferrumgate-nonprod VM and migration of ferrumd from SQLite to PostgreSQL backend.

---

## 2. PG-P.1: PostgreSQL Provisioned and Reachable

### Installation Details

| Parameter | Value |
|-----------|-------|
| PostgreSQL version | 16.14 (Ubuntu 16.14-1.pgdg24.04+1) |
| Cluster | 16/main |
| Port | 5432 |
| Status | online |
| Data directory | /var/lib/postgresql/16/main |
| Log file | /var/log/postgresql/postgresql-16-main.log |

### Database and User

| Parameter | Value |
|-----------|-------|
| Database | ferrumgate |
| Owner | ferrumgate_app |
| Host | localhost |
| Port | 5432 |

### Evidence

| Check | Result |
|-------|--------|
| PostgreSQL 16 installed | PASS |
| Cluster 16/main online | PASS |
| Service active (running) | PASS |
| Database ferrumgate created | PASS |
| User ferrumgate_app can connect | PASS |

---

## 3. PG-P.2: ferrumd with Production PostgreSQL DSN

### Migration Details

| Parameter | Value |
|-----------|-------|
| Source | SQLite /var/lib/ferrumgate/ferrumgate.db (6.9 MB) |
| Target | PostgreSQL ferrumgate database |
| Migration method | Manual CSV-based migration |
| Rows migrated | 4,511 total |

### Row Count Verification

| Table | SQLite | PostgreSQL | Match |
|-------|--------|------------|-------|
| intents | 4,459 | 4,459 | ✅ |
| proposals | 13 | 13 | ✅ |
| capabilities | 13 | 13 | ✅ |
| provenance_events | 26 | 26 | ✅ |

### ferrumd Configuration

```toml
[server]
store_dsn = "postgres://ferrumgate_app:<REDACTED>@localhost:5432/ferrumgate"
bind_addr = "0.0.0.0:19080"

[store]
pg_max_connections = 10
pg_min_idle = 2
pg_acquire_timeout_secs = 5
pg_statement_timeout_ms = 5000
pg_idle_in_transaction_timeout_ms = 10000

[auth]
mode = "bearer"
```

### Evidence

| Check | Result |
|-------|--------|
| SQLite data migrated | PASS |
| ferrumd config updated | PASS |
| ferrumd restarted | PASS |
| /v1/readyz/deep returns 200 | PASS |
| PG pool metrics visible | PASS |

### Health Check Output

```json
{
  "status": "ok",
  "healthy": true,
  "components": [
    {"component": "store", "status": "ok", "healthy": true},
    {"component": "write_queue", "status": "ok: depth=0, threshold=100", "healthy": true},
    {"component": "pool", "status": "ok: idle=1/total=2/max=10", "healthy": true}
  ]
}
```

### PG Pool Metrics

```
ferrumgate_store_health_up 1
ferrumgate_store_pg_pool_size 2
ferrumgate_store_pg_pool_idle 1
ferrumgate_store_pg_pool_max 10
ferrumgate_store_pg_acquire_timeouts_total 0
```

---

## 4. Boundary and Non-Claims

- **Target VM deployment**: This is a nonprod VM, not a production environment.
- **No production-ready claim**: This deployment validates PG backend on target VM only.
- **No HA claim**: Single-node PostgreSQL, no replication or failover.
- **No sustained SLO claim**: Bounded validation only.

---

## 5. Related Artifacts

- [`2026-05-27-pg-tls-dsn-evidence.md`](./2026-05-27-pg-tls-dsn-evidence.md) — TLS/SSL setup
- [`2026-05-27-pg-pgbouncer-evidence.md`](./2026-05-27-pg-pgbouncer-evidence.md) — PgBouncer deployment
- [`docs/production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md`](../../production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md) — Tier 1.5 framework

---

*Artifact created: 2026-05-27. PostgreSQL target deployment evidence. No production-ready claim.*
