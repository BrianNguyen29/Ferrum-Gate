# PostgreSQL TLS/SSL DSN Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-tls-dsn-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 1 — PG-P.3 (TLS/SSL encrypted DSN validated)
> **Constraint**: Self-signed CA, same-VM deployment. No production-ready claim.

---

## 1. Summary

This artifact records the successful setup of TLS/SSL encryption for PostgreSQL connections using a self-signed CA certificate.

---

## 2. TLS Configuration

### Certificate Details

| Parameter | Value |
|-----------|-------|
| CA certificate | /etc/ferrumgate/certs/pg-ca.crt |
| Server certificate | /etc/ferrumgate/certs/pg-server.crt |
| Server key | /etc/ferrumgate/certs/pg-server.key |
| Certificate type | X509v3 (rustls compatibility) |
| Validity | 3650 days (10 years) |
| CA CN | ferrumgate-pg-ca |
| Server CN | localhost |

### PostgreSQL TLS Settings

```ini
ssl = on
ssl_cert_file = '/etc/ferrumgate/certs/pg-server.crt'
ssl_key_file = '/etc/ferrumgate/certs/pg-server.key'
ssl_ca_file = '/etc/ferrumgate/certs/pg-ca.crt'
```

### pg_hba.conf (SSL Required)

```
hostssl ferrumgate ferrumgate_app 127.0.0.1/32 scram-sha-256
hostssl ferrumgate ferrumgate_app ::1/128 scram-sha-256
host all all 127.0.0.1/32 reject
host all all ::1/128 reject
```

### ferrumd DSN with TLS

```
postgres://ferrumgate_app:<REDACTED>@localhost:5432/ferrumgate?sslmode=verify-ca&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt
```

---

## 3. Evidence

| Check | Result |
|-------|--------|
| TLS certificates generated | PASS |
| PostgreSQL ssl=on | PASS |
| ferrumd connects via TLS | PASS |
| TLS version | TLSv1.3 |
| TLS cipher | TLS_AES_256_GCM_SHA384 |
| Non-SSL connections rejected | PASS |

### SSL Status Query

```sql
SHOW ssl;
-- Result: on
```

### Connection SSL Info

```sql
SELECT * FROM pg_stat_ssl WHERE pid = pg_backend_pid();
-- Result: ssl=t, version=TLSv1.3, cipher=TLS_AES_256_GCM_SHA384
```

### ferrumd PG Connections

```sql
SELECT pid, ssl, version, client_addr 
FROM pg_stat_ssl 
JOIN pg_stat_activity USING (pid) 
WHERE usename='ferrumgate_app';
-- Result: 3 connections, all ssl=t TLSv1.3 from 127.0.0.1
```

### Non-SSL Rejection Test

```
FATAL: pg_hba.conf rejects connection for host '127.0.0.1', user 'ferrumgate_app', database 'ferrumgate', no encryption
```

---

## 4. Boundary and Non-Claims

- **Self-signed CA**: Not a public CA; suitable for same-VM deployment only.
- **sslmode=verify-ca**: Verifies CA but not hostname (localhost trivial).
- **No production-ready claim**: TLS validates encryption only, not production posture.

---

## 5. Related Artifacts

- [`2026-05-27-pg-target-deployment-evidence.md`](./2026-05-27-pg-target-deployment-evidence.md) — PG deployment
- [`2026-05-27-pg-pgbouncer-evidence.md`](./2026-05-27-pg-pgbouncer-evidence.md) — PgBouncer with TLS backend

---

*Artifact created: 2026-05-27. PostgreSQL TLS/SSL DSN evidence. No production-ready claim.*
