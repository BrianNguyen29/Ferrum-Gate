# PgBouncer Connection Pooling Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-pgbouncer-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 1 — PG-P.4 (PgBouncer operational)
> **Constraint**: Transaction mode, localhost deployment. No production-ready claim.

---

## 1. Summary

This artifact records the successful deployment of PgBouncer as a connection pooler between ferrumd and PostgreSQL.

---

## 2. PgBouncer Configuration

### Installation Details

| Parameter | Value |
|-----------|-------|
| PgBouncer version | 1.25.2 |
| Listen address | 127.0.0.1:6432 |
| Pool mode | transaction |
| Auth type | md5 |
| Max client connections | 30 |
| Default pool size | 15 |
| Reserve pool size | 5 |

### Backend TLS to PostgreSQL

```ini
server_tls_sslmode = verify-ca
server_tls_ca_file = /etc/ferrumgate/certs/pg-ca.crt
```

### ferrumd DSN via PgBouncer

```
postgres://ferrumgate_app:<REDACTED>@127.0.0.1:6432/ferrumgate?sslmode=disable
```

Note: ferrumd connects to PgBouncer without TLS (localhost), PgBouncer connects to PG with TLS.

---

## 3. Evidence

| Check | Result |
|-------|--------|
| PgBouncer installed | PASS |
| PgBouncer running on 6432 | PASS |
| ferrumd connects via PgBouncer | PASS |
| PgBouncer pools active | PASS |
| Backend TLS to PG | TLSv1.3 |
| PG pool metrics visible | PASS |

### PgBouncer Service Status

```
Active: active (running) since Wed 2026-05-27 04:48:19 UTC
```

### PgBouncer Pools

```
SHOW POOLS;
-- Result: ferrumgate | ferrumgate_app | cl_active=2 | sv_idle=1 | pool_mode=transaction
```

### PgBouncer Servers (Backend Connections)

```
SHOW SERVERS;
-- Result: ferrumgate_app@127.0.0.1:5432 | tls=TLSv1.3/TLS_AES_256_GCM_SHA384
```

### ferrumd Health Check

```json
{
  "status": "ok",
  "healthy": true,
  "components": [
    {"component": "store", "status": "ok", "healthy": true},
    {"component": "pool", "status": "ok: idle=1/total=2/max=10", "healthy": true}
  ]
}
```

### ferrumd PG Pool Metrics

```
ferrumgate_store_pg_pool_size 2
ferrumgate_store_pg_pool_idle 1
ferrumgate_store_pg_pool_max 10
```

---

## 4. Boundary and Non-Claims

- **Transaction mode**: Connections recycled between transactions.
- **Localhost only**: PgBouncer and ferrumd on same VM.
- **No production-ready claim**: Connection pooling validated on target VM only.

---

## 5. Related Artifacts

- [`2026-05-27-pg-tls-dsn-evidence.md`](./2026-05-27-pg-tls-dsn-evidence.md) — TLS setup
- [`2026-05-27-pg-restore-drill-evidence.md`](./2026-05-27-pg-restore-drill-evidence.md) — Backup/restore drill

---

*Artifact created: 2026-05-27. PgBouncer connection pooling evidence. No production-ready claim.*
