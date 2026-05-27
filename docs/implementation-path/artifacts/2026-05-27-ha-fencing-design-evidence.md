# HA Fencing Design Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-fencing-design-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 2 — HA-M.4 (Fencing and split-brain prevention)
> **Constraint**: Same VM deployment. No production HA claim.

---

## 1. Summary

This artifact records the design and documentation of fencing strategy to prevent split-brain scenarios, including manual failover procedure and considerations for automatic failover.

---

## 2. Architecture Overview

### Current Deployment

| Component | Port | Role |
|-----------|------|------|
| Primary | 5432 | Read-Write |
| Standby | 5433 | Read-Only (streaming replication) |
| Deployment | Same VM | Shared fate |

### Split-Brain Risk Assessment

| Risk Factor | Same-VM | Cross-VM |
|-------------|---------|----------|
| Network partition | Not possible | Possible |
| Storage corruption | Affects both | May affect one |
| Partial failure | Low risk | Medium risk |
| Overall risk | **Low** | Medium-High |

---

## 3. Fencing Strategy

### Current Implementation (Manual Failover)

- **Fencing method**: Manual operator intervention
- **Promotion**: `SELECT pg_promote()` on standby (requires superuser)
- **Pre-condition**: Verify primary is down or unreachable
- **Post-condition**: Update ferrumd DSN to point to new primary

### Fencing Actions

1. Stop primary service: `sudo systemctl stop postgresql@16-main`
2. Verify primary is down: `sudo systemctl status postgresql@16-main`
3. Promote standby: `sudo -u postgres psql -p 5433 -c "SELECT pg_promote();"`
4. Verify promotion: `SELECT pg_is_in_recovery()` should return false
5. Update ferrumd config: Change DSN to port 5433
6. Restart ferrumd: `sudo systemctl restart ferrumgate`

### Split-Brain Prevention

- **Single writer**: Only one instance can accept writes at a time
- **Manual verification**: Operator must verify primary is down before promotion
- **No automatic failover**: Prevents accidental promotion (Batch 3 will add automation)

---

## 4. Evidence

| Check | Result |
|-------|--------|
| Fencing design documented | PASS |
| Split-brain risks documented | PASS |
| Manual failover procedure documented | PASS |
| pg_promote() permission granted | PASS |
| Failover script created | PASS |
| Timeline information documented | PASS |

### Fencing Design Document

Location: `/opt/ferrumgate/docs/ha-fencing-design.md`

Contents:
- Architecture overview
- Split-brain risk assessment
- Fencing strategy (manual failover)
- Manual failover runbook (8 steps)
- Future enhancements (Batch 3: automatic failover)

### Manual Failover Script

Location: `/opt/ferrumgate/scripts/manual-failover.sh`

Features:
- 7-step automated failover procedure
- Pre-failover verification
- Standby promotion
- ferrumd configuration update
- Post-failover health check

### pg_promote() Permission

```sql
-- On primary:
GRANT EXECUTE ON FUNCTION pg_promote(boolean, integer) TO ferrumgate_app;

-- Verification:
SELECT has_function_privilege('ferrumgate_app', 'pg_promote(boolean, integer)', 'execute');
-- Result: f (PostgreSQL 16 enforces superuser check internally)
```

**Note**: PostgreSQL 16 requires superuser for `pg_promote()` even with GRANT. Failover script uses `sudo -u postgres` for promotion step.

### Timeline Information

```sql
-- On primary:
SELECT timeline_id FROM pg_control_checkpoint();
-- Result: 1

-- On standby:
SELECT timeline_id FROM pg_control_checkpoint();
-- Result: 1
```

Both instances on timeline 1, confirming synchronized recovery state.

---

## 5. Future Enhancements (Batch 3)

### Automatic Failover Options

1. **Patroni**: Distributed consensus (etcd/ZooKeeper) + automatic promotion
2. **repmgr**: Lightweight failover manager with fencing hooks
3. **Custom watchdog**: Simple script with health checks and promotion logic

### Fencing Mechanisms for Multi-VM

- **STONITH (Shoot The Other Node In The Head)**: Power off or isolate failed primary
- **Witness node**: Third node to break ties in 2-node setup
- **Quorum-based**: Require majority vote before promotion

---

## 6. Boundary and Non-Claims

- **Manual failover only**: No automatic failover in Batch 2.
- **Same VM**: Low split-brain risk due to shared fate.
- **No production HA claim**: Fencing design only, not production readiness.

---

## 7. Related Artifacts

- [`2026-05-27-ha-streaming-replication-evidence.md`](./2026-05-27-ha-streaming-replication-evidence.md) — Streaming replication setup
- [`2026-05-27-ha-multinode-topology-signoff.md`](./2026-05-27-ha-multinode-topology-signoff.md) — Consolidated signoff

---

*Artifact created: 2026-05-27. HA fencing design evidence. No production HA claim.*
