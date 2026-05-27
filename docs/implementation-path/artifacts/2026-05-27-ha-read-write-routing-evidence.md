# HA Read/Write Routing Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-read-write-routing-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 2 — HA-M.2 (Read/write routing validation)
> **Constraint**: Same VM deployment. No production HA claim.

---

## 1. Summary

This artifact records the validation of read/write routing behavior: writes succeed only on primary, reads succeed on both primary and standby.

---

## 2. Routing Configuration

### Current Architecture

| Component | Port | Mode | Role |
|-----------|------|------|------|
| Primary | 5432 | Read-Write | Accepts writes and reads |
| Standby | 5433 | Read-Only | Accepts reads only |
| PgBouncer | 6432 | Transaction | Routes to primary |

### ferrumd Configuration

```toml
[server]
store_dsn = "postgres://ferrumgate_app:<REDACTED>@127.0.0.1:6432/ferrumgate?sslmode=disable"
```

ferrumd connects to PgBouncer (port 6432), which routes to primary (port 5432).

---

## 3. Evidence

| Check | Result |
|-------|--------|
| Write on primary succeeds | PASS |
| Write on standby fails | PASS (expected) |
| Read on primary succeeds | PASS |
| Read on standby succeeds | PASS |
| Standby is read-only | PASS |
| DDL on standby fails | PASS (expected) |
| ferrumd connected to primary | PASS |

### Write on Primary (Success)

```sql
-- On primary (port 5432):
CREATE TABLE routing_test (id serial PRIMARY KEY, data text);
INSERT INTO routing_test (data) VALUES ('primary_write_test');
SELECT * FROM routing_test;
-- Result: id=1, data=primary_write_test
```

### Write on Standby (Expected Failure)

```sql
-- On standby (port 5433):
INSERT INTO routing_test (data) VALUES ('standby_write_test');
-- ERROR: cannot execute INSERT in a read-only transaction
```

### Read on Primary (Success)

```sql
-- On primary (port 5432):
SELECT * FROM routing_test;
-- Result: id=1, data=primary_write_test
```

### Read on Standby (Success)

```sql
-- On standby (port 5433):
SELECT * FROM routing_test;
-- Result: id=1, data=primary_write_test (replicated from primary)
```

### Standby Recovery Mode

```sql
SELECT pg_is_in_recovery();
-- Result: t (true)
```

### DDL on Standby (Expected Failure)

```sql
-- On standby (port 5433):
CREATE TABLE test_ddl (id int);
-- ERROR: cannot execute CREATE TABLE in a read-only transaction
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

---

## 4. Routing Strategy

### Current Strategy (Batch 2)

- **All traffic**: Routes to primary via PgBouncer
- **Read replicas**: Not used (future enhancement)
- **Failover**: Manual promotion only

### Future Strategy (Batch 3+)

- **Read replicas**: Route read-only queries to standby
- **Automatic failover**: Patroni/repmgr with automatic promotion
- **Connection pooling**: PgBouncer with multiple backends

---

## 5. Boundary and Non-Claims

- **No read replica routing**: All queries currently go to primary.
- **No automatic failover**: Manual promotion only.
- **No production HA claim**: Routing validation only, not production readiness.

---

## 6. Related Artifacts

- [`2026-05-27-ha-streaming-replication-evidence.md`](./2026-05-27-ha-streaming-replication-evidence.md) — Streaming replication setup
- [`2026-05-27-ha-replication-lag-evidence.md`](./2026-05-27-ha-replication-lag-evidence.md) — Replication lag measurement

---

*Artifact created: 2026-05-27. HA read/write routing evidence. No production HA claim.*
