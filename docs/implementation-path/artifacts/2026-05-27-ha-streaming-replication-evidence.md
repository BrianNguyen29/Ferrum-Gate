# HA Streaming Replication Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-streaming-replication-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 2 — HA-M.1 (Streaming replication setup)
> **Constraint**: Same VM deployment (primary + standby). No production HA claim.

---

## 1. Summary

This artifact records the successful setup of PostgreSQL streaming replication between primary (port 5432) and standby (port 5433) on the ferrumgate-nonprod VM.

---

## 2. Replication Configuration

### Primary Settings

| Parameter | Value |
|-----------|-------|
| Port | 5432 |
| wal_level | replica |
| max_wal_senders | 5 |
| wal_keep_size | 256MB |
| hot_standby | on |

### Standby Settings

| Parameter | Value |
|-----------|-------|
| Port | 5433 |
| Data directory | /var/lib/postgresql/16/standby |
| Replication user | replicator |
| Replication method | Streaming (async) |

### Replication User

```sql
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD '<REDACTED>';
```

---

## 3. Setup Procedure

1. Configured primary with `wal_level=replica`, `max_wal_senders=5`, `wal_keep_size=256MB`, `hot_standby=on`
2. Created replication user `replicator` on primary
3. Updated `pg_hba.conf` to allow replication connections from localhost
4. Created standby data directory `/var/lib/postgresql/16/standby`
5. Cloned primary to standby using `pg_basebackup -h 127.0.0.1 -p 5432 -U replicator -D /var/lib/postgresql/16/standby -Fp -Xs -P -R`
6. Configured standby to use port 5433
7. Created systemd service `postgresql@16-standby`
8. Started standby and verified streaming replication

---

## 4. Evidence

| Check | Result |
|-------|--------|
| Primary configured for replication | PASS |
| Replication user created | PASS |
| Standby data directory cloned | PASS |
| Standby running on port 5433 | PASS |
| Streaming replication active | PASS |
| Standby in recovery mode | PASS |
| Test data replicated | PASS |

### Replication Status (pg_stat_replication)

```
client_addr: 127.0.0.1
state: streaming
sent_lsn: 0/5039CB0
write_lsn: 0/5039CB0
flush_lsn: 0/5039CB0
replay_lsn: 0/5039CB0
sync_state: async
```

### Standby Recovery Mode

```sql
SELECT pg_is_in_recovery();
-- Result: t (true)
```

### Test Data Replication

```sql
-- On primary:
CREATE TABLE replication_test (id serial PRIMARY KEY, data text);
INSERT INTO replication_test (data) VALUES ('test_data_1');

-- On standby (after 2 seconds):
SELECT * FROM replication_test;
-- Result: id=1, data=test_data_1
```

---

## 5. Boundary and Non-Claims

- **Same VM deployment**: Both primary and standby on ferrumgate-nonprod VM.
- **Async replication**: Not synchronous; minimal lag but not zero-latency.
- **No automated failover**: Manual promotion only (Batch 3 will add automation).
- **No production HA claim**: This is procedure rehearsal, not production readiness.

---

## 6. Related Artifacts

- [`2026-05-27-ha-read-write-routing-evidence.md`](./2026-05-27-ha-read-write-routing-evidence.md) — Read/write routing validation
- [`2026-05-27-ha-replication-lag-evidence.md`](./2026-05-27-ha-replication-lag-evidence.md) — Replication lag measurement
- [`2026-05-27-ha-fencing-design-evidence.md`](./2026-05-27-ha-fencing-design-evidence.md) — Fencing and split-brain prevention

---

*Artifact created: 2026-05-27. HA streaming replication evidence. No production HA claim.*
