# HA Local Rehearsal and Script Fix Evidence — 2026-05-28

> **Artifact ID**: 2026-05-28-ha-local-rehearsal-and-script-fix-evidence
> **Date**: 2026-05-28
> **Owner**: Engineering
> **Scope**: Local HA simulation rehearsal after fixing psql connection method and HA compose listen addresses.
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## 1. Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local Docker Compose simulation only. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Engineering-owned local evidence only. |
| **Multi-host production HA** | **NO** | Local single-host Docker simulation; no multi-host consensus. |
| **Automated failover** | **NO** | Promotion is manual (`pg_promote()`). No unattended automation. |
| **HA-4 unattended automated failover** | **NOT COMPLETE** | Local drills only; no unattended fencing/promotion/routing. |
| **Sustained SLO window** | **NO** | No sustained observation window claimed. |

---

## 2. Script Fix Summary

Two script/compose fixes were applied to make local HA drills deterministic:

| Fix | Before | After | Rationale |
|-----|--------|-------|-----------|
| **psql connection method** | Relied on default Unix socket path inside containers, which could resolve ambiguously depending on container setup. | All Docker-container `psql` calls forced to TCP with `-h localhost`. | Eliminates socket-path ambiguity; ensures consistent connection behavior across host environments. |
| **HA compose `listen_addresses`** | Used quoted `listen_addresses='*'` in the Compose command. | Simplified to unquoted `listen_addresses=*` in `docker-compose.ha-local.yml`. | Removes quoting ambiguity while preserving the intended PostgreSQL listen behavior. |

These fixes are currently in the canonical working tree (uncommitted modifications to `docker-compose.ha-local.yml`, `scripts/setup_ha_local.sh`, `scripts/run_ha_local_failover_drill.sh`, and `scripts/run_ha_local_ferrumd_reconnect_drill.sh`).

---

## 3. Rehearsal 1 — HA Local Failover Drill

### 3.1 Command

```bash
make ha-local-setup && make ha-local-failover-drill && make ha-local-teardown
```

### 3.2 Results

| Stage | Passed | Failed | Notes |
|-------|--------|--------|-------|
| Setup | 8 | 0 | Primary healthy, replication user present, `pg_basebackup` complete, standby healthy, streaming replication verified. |
| Failover drill | 16 | 0 | Baseline primary writable, standby read-only verified, failure injection, manual `pg_promote()`, promotion confirmed, post-promotion write, RPO check, old-primary stopped check. |
| Teardown | — | — | Containers and volumes removed cleanly. |

**RTO**: 4 seconds  
**RPO**: 0 rows lost

---

## 4. Rehearsal 2 — HA Local ferrumd Reconnect Drill

### 4.1 Command

```bash
make ha-local-setup && make ha-local-ferrumd-reconnect-drill && make ha-local-teardown
```

### 4.2 Results

| Stage | Passed | Failed | Notes |
|-------|--------|--------|-------|
| Setup | 8 | 0 | Same as rehearsal 1. |
| Reconnect drill | 13 | 0 | ferrumd starts against primary, readyz passes, primary stopped, standby promoted, ferrumd restarted against standby, readyz and smoke request pass. |
| Teardown | — | — | Containers and volumes removed cleanly. |

**App-level RTO**: 4 seconds  
**RPO**: 0 rows lost (no data loss between primary stop and standby promotion)

---

## 5. Expected vs Observed

| Check | Expected | Observed | Result |
|-------|----------|----------|--------|
| Setup passes all checks | 8 passed | 8 passed | ✅ PASS |
| Failover drill passes all checks | 16 passed | 16 passed | ✅ PASS |
| Reconnect drill passes all checks | 13 passed | 13 passed | ✅ PASS |
| RTO <= 10 s | <= 10 s | 4 s | ✅ PASS |
| RPO = 0 rows lost | 0 rows lost | 0 rows lost | ✅ PASS |
| Script fix improves determinism | More consistent connection behavior | TCP `-h localhost` used consistently | ✅ PASS |
| Teardown cleans up | Containers/volumes removed | Removed cleanly | ✅ PASS |

---

## 6. Summary

```text
HA LOCAL SETUP:                  Passed 8,  Failed 0
HA LOCAL FAILOVER DRILL:         Passed 16, Failed 0, RTO 4s, RPO 0
HA LOCAL FERRUMD RECONNECT DRILL: Passed 13, Failed 0, app-level RTO 4s, RPO 0
Script fix:                      psql forced to TCP (-h localhost), listen_addresses=*
Production-ready:                NO
Full G2:                         NOT COMPLETE
HA/multi-node:                   NO
Automated failover:              NO
Sustained SLO window:            NO
```

**Verdict**: `HA LOCAL REHEARSAL AFTER SCRIPT FIX: ALL CHECKS PASSED`

---

## 7. Interpretation

- The TCP `-h localhost` fix eliminates the primary source of non-determinism in local HA drills (Unix socket path ambiguity).
- The `listen_addresses=*` quote simplification removes environment-specific PostgreSQL listen configuration ambiguity from the critical path.
- Both rehearsals passed cleanly with consistent RTO (4 s) and RPO (0 rows lost).
- These are local-only engineering rehearsals. They do **not** constitute production HA, automated failover, multi-node deployment, or sustained SLO evidence.

---

## 8. Known Gaps

- **No unattended automated failover daemon**: Promotion is manual via `pg_promote()` or operator-controlled. No Patroni, repmgr, or unattended watchdog.
- **No multi-host**: Both containers run on the same Docker host.
- **No real fencing/STONITH**: Old primary is simply stopped. Production requires automated fencing.
- **No sustained workload during failover**: Drills use single-row probes and readyz checks.
- **No target-host or managed PostgreSQL evidence**.
- **No sustained SLO observation window**.

---

## 9. Files Changed

- `docker-compose.ha-local.yml` — `listen_addresses=*` quote simplification
- `scripts/setup_ha_local.sh` — psql calls forced to TCP with `-h localhost`
- `scripts/run_ha_local_failover_drill.sh` — psql calls forced to TCP with `-h localhost`
- `scripts/run_ha_local_ferrumd_reconnect_drill.sh` — psql calls forced to TCP with `-h localhost`
- `docs/implementation-path/artifacts/2026-05-28-ha-local-rehearsal-and-script-fix-evidence.md` — this artifact

---

## 10. Related Artifacts

- [`2026-05-26-ha-local-failover-simulation-evidence.md`](./2026-05-26-ha-local-failover-simulation-evidence.md) — prior local HA failover evidence
- [`2026-05-26-ha-local-ferrumd-reconnect-evidence.md`](./2026-05-26-ha-local-ferrumd-reconnect-evidence.md) — prior ferrumd reconnect evidence
- [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](./2026-05-27-ha-phase9-multihost-drill-evidence.md) — multi-host manual drill evidence
- [`2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md`](./2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md) — host B redundancy bounded fenced drill
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — HA roadmap
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — evidence checklist

---

*Artifact created: 2026-05-28. HA local rehearsal and script fix evidence. No production-ready claim.*
