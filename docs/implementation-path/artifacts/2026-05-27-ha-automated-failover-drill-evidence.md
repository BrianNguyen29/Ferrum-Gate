# HA Automated Failover Drill Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-automated-failover-drill-evidence
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 3 — HA-A.1 through HA-A.5 automated failover drills
> **Constraint**: Same-VM topology only. No production-ready or multi-host production HA claim.

---

## 1. Summary

This artifact records three passing same-VM automated failover drills. The watchdog stopped/fenced the current primary, promoted the standby, switched PgBouncer to the promoted node, and restored ferrumd readiness without restarting ferrumd.

---

## 2. Implementation

| Component | Value |
|-----------|-------|
| Watchdog | `/opt/ferrumgate/scripts/auto-failover-watchdog.sh` |
| Role-aware watchdog | `/opt/ferrumgate/scripts/auto-failover-watchdog-v2.sh` |
| Reset helper | `/tmp/reset_node_as_standby.sh` |
| PgBouncer | backend rewritten between 5432 and 5433, followed by `RECONNECT` |
| Sentinel | `/var/run/ferrumgate-failover-complete` |
| Log | `/var/log/ferrumgate/failover.log` |

---

## 3. Gate Evidence

| Gate | Requirement | Result |
|------|-------------|--------|
| HA-A.1 | Failover occurs without manual `pg_promote` | ✅ PASS — watchdog executed promotion automatically. |
| HA-A.2 | ferrumd reconnects to new primary without manual restart | ✅ PASS — ferrumd PID stayed `342943` across all drills. |
| HA-A.3 | RTO and RPO measured | ✅ PASS — RTO/RPO recorded per drill. |
| HA-A.4 | No split-brain observed | ✅ PASS — old primary stopped/port closed before promotion. |
| HA-A.5 | At least 3 drills with pass evidence | ✅ PASS — three drills passed. |

---

## 4. Drill Results

| Drill | Direction | RTO | RPO | ferrumd PID | Split-brain check |
|-------|-----------|-----|-----|-------------|-------------------|
| 1 | 5432 → 5433 | 6s | marker present, 0 rows lost | unchanged (`342943`) | PASS |
| 2 | 5433 → 5432 | 5s | marker present, 0 rows lost | unchanged (`342943`) | PASS |
| 3 | 5432 → 5433 | 15s | marker present, 0 rows lost | unchanged (`342943`) | PASS |

Drill 3 had a higher RTO than the first two drills, likely due PgBouncer/sqlx pool warm-up or retry backoff. It still passed the bounded Tier 1.5 threshold.

---

## 5. Final Topology

After the third drill:

| Component | State |
|-----------|-------|
| PostgreSQL 5433 | Primary / read-write |
| PostgreSQL 5432 | Standby / in recovery / streaming |
| PgBouncer 6432 | Routing to 5433 |
| ferrumd | Healthy; PID unchanged |
| Sentinel | Present to prevent failover loops |

---

## 6. Safety Notes

- Data directories were timestamp-backed up before reset/rebuild operations.
- The inactive node was rebuilt from the current primary with `pg_basebackup` between drills.
- The watchdog timer was left disabled unless/reset until operator intentionally enables it.

---

## 7. Boundary and Non-Claims

- **Same VM only**: This is not multi-host production HA.
- **No Tier 2 claim**: `production-ready = NO` remains true.
- **No full G2 claim**: `full G2 = NOT COMPLETE` remains true.
- **Block A remains conditional**: `Block A = WAIVED/CONDITIONAL` remains true.

---

*Artifact created: 2026-05-27. Automated failover drill evidence. No production-ready claim.*
