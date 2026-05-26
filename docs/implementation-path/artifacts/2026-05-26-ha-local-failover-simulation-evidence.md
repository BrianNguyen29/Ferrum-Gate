# HA Local Failover Simulation Evidence — 2026-05-26

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-26 `make ha-local-setup` + `make ha-local-failover-drill` local run.
> **Owner**: Engineering
> **Date**: 2026-05-26
> **Scope**: Local Docker primary/standby streaming replication simulation only; no target-host or production claims.
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local Docker Compose simulation only; no managed or production PostgreSQL HA. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Engineering-owned local evidence only. |
| **Block A closed** | **NO** | Real owned domain still required for production-ready or full G2 closure. |
| **PostgreSQL production deployment** | **NO** | Local Docker PostgreSQL only. |
| **HA / multi-node / true HA** | **NO** | Local single-host Docker simulation; no automated failover daemon; no multi-host consensus. |
| **Automated failover** | **NO** | Promotion is manual (`pg_promote()`). No Patroni/repmgr/operator. |
| **Split-brain prevention in production** | **NO** | Local-scope check only (old primary container stopped). No fencing/STONITH. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-26 |
| Host scope | Local development workstation |
| Docker version | 29.3.1 |
| Docker Compose version | v2.35.1 |
| make version | GNU Make 4.3 |
| bash version | 5.1.16 |
| PostgreSQL image | postgres:16 |
| Primary port | 5433 |
| Standby port | 5434 |

---

## 2. Procedure

### 2.1 `make ha-local-setup`

**Script**: `scripts/setup_ha_local.sh`

Steps performed:
1. Starts `docker-compose.ha-local.yml` primary (`ferrumgate_postgres_ha_primary`).
2. Waits for primary healthcheck (`healthy`).
3. Verifies replication user `replicator` exists on primary.
4. Runs `pg_basebackup` from a temporary container into the standby named volume.
5. Starts standby container (`ferrumgate_postgres_ha_standby`) using the basebackup-populated volume.
6. Waits for standby healthcheck (`healthy`).
7. Verifies streaming replication by creating a row on primary and observing it on standby.

**Result**: Passed 8, Failed 0, Skipped 0.

### 2.2 `make ha-local-failover-drill`

**Script**: `scripts/run_ha_local_failover_drill.sh`

Steps performed:
1. **Preflight**: verifies docker and both containers exist.
2. **Baseline primary writable**: creates `ha_drill_probe` table and inserts `baseline` row on primary.
3. **Baseline standby reachable**: waits for `baseline` row to appear on standby (streaming replication).
4. **Standby read-only check**: verifies `pg_is_in_recovery() = t` on standby.
5. **Standby write rejection**: attempts an insert on standby and confirms it fails while in recovery.
6. **Pre-failover seed**: inserts `pre-failover` row on primary; records row count.
7. **Failure injection**: stops primary container (`docker stop`).
8. **Promotion**: issues `SELECT pg_promote()` on standby.
9. **Wait for promotion**: polls `pg_is_in_recovery()` until `f` (up to 30 s).
10. **Post-promotion write**: inserts `post-promotion` row on promoted primary.
11. **RPO check**: compares pre-failover row count vs promoted row count.
12. **Old primary stopped**: verifies old primary container status is `exited`.
13. **Split-brain local check**: confirms old primary container does not accept `docker exec`.
14. **RTO measurement**: time from primary stop to successful promoted write.

**Result**: Passed 16, Failed 0, Skipped 0. Latest observed RTO: 3 s. RPO: 0 rows lost.

---

## 3. Expected vs Observed

| Check | Expected | Observed | Result |
|-------|----------|----------|--------|
| Primary container healthy | healthy within 30 s | healthy | ✅ PASS |
| Replication user exists | `replicator` role present | present | ✅ PASS |
| pg_basebackup completes | exit 0 | exit 0 | ✅ PASS |
| Standby container healthy | healthy within 30 s | healthy | ✅ PASS |
| Streaming replication verified | row `42` replicated | replicated | ✅ PASS |
| Primary writable | table created, row inserted | success | ✅ PASS |
| Standby reachable before promotion | row visible | visible | ✅ PASS |
| Standby in recovery | `pg_is_in_recovery() = t` | `t` | ✅ PASS |
| Standby rejects writes while in recovery | insert fails | failed as expected | ✅ PASS |
| Pre-failover row inserted | count >= 2 | count = 2 | ✅ PASS |
| Primary stopped | container exited | exited | ✅ PASS |
| Promotion command issued | `pg_promote()` returns `t` | `t` | ✅ PASS |
| Standby exits recovery | `pg_is_in_recovery() = f` | `f` | ✅ PASS |
| Promoted primary writable | insert succeeds | success | ✅ PASS |
| RPO check | promoted count >= pre-failover count | 3 >= 2 | ✅ PASS |
| Old primary not writable | container status `exited` | `exited` | ✅ PASS |
| Split-brain local check | `docker exec` fails | failed | ✅ PASS |
| RTO | measured | 3 s | ✅ PASS |

---

## 4. Summary

```text
HA LOCAL SETUP:        Passed 8,  Failed 0, Skipped 0
HA LOCAL FAILOVER DRILL: Passed 16, Failed 0, Skipped 0
RTO: 3 seconds
RPO: 0 rows lost
```

**Verdict**: `HA LOCAL FAILOVER DRILL: ALL CHECKS PASSED`

---

## 5. Interpretation

- The repository now has a runnable local Docker simulation of PostgreSQL streaming replication with primary and standby containers.
- `make ha-local-setup` starts the simulation and verifies replication is working.
- `make ha-local-failover-drill` executes a bounded failover drill that measures RTO and RPO against the local simulation.
- `make ha-local-teardown` cleans up containers and volumes.
- This is local-only engineering evidence for procedure rehearsal. It does **not** constitute production HA, automated failover, multi-node deployment, or true split-brain prevention.
- The latest measured RTO of 3 s reflects the local Docker stop + `pg_promote()` + recovery-exit polling time. In a real operator environment, RTO would be bounded by operator detection speed, network latency, and any required DSN updates or application restarts.
- The measured RPO of 0 rows lost in this run reflects synchronous local replication with immediate promotion. In a real async replication scenario, RPO would be bounded by replication lag and unflushed WAL.

---

## 6. Known Gaps

- **No automated failover daemon**: Promotion is manual via `pg_promote()`. No Patroni, repmgr, or operator controller.
- **No multi-host**: Both containers run on the same Docker host. No network partition simulation.
- **No real fencing/STONITH**: Old primary is simply stopped. In production, automated fencing is required to prevent split-brain.
- **No ferrumd reconnect verification**: The drill does not start ferrumd against the HA topology. A future drill could add ferrumd DSN switch and restart.
- **No sustained workload during failover**: The drill uses single-row probes. A future enhancement could run a lightweight write stream and measure in-flight request behavior.
- **No target-host or managed PostgreSQL evidence**.
- **No real-domain, HTTPS, or Block A closure**.

---

## 7. Makefile Wiring

New targets:

```makefile
ha-local-setup:
	@echo "Setting up local HA PostgreSQL simulation..."
	@bash scripts/setup_ha_local.sh

ha-local-failover-drill:
	@echo "Running local HA failover drill..."
	@bash scripts/run_ha_local_failover_drill.sh

ha-local-teardown:
	@echo "Tearing down local HA PostgreSQL simulation..."
	@bash scripts/teardown_ha_local.sh
```

Help entries:

```text
make ha-local-setup         - start local HA primary/standby PostgreSQL simulation
make ha-local-failover-drill - run local HA failover drill (requires ha-local-setup first)
make ha-local-teardown      - stop and remove local HA simulation containers/volumes
```

---

## 8. Files Added / Changed

- `docker-compose.ha-local.yml` — local primary/standby compose
- `scripts/ha_local_primary_init.sh` — primary init script (replication user + HBA rule)
- `scripts/setup_ha_local.sh` — setup orchestration
- `scripts/run_ha_local_failover_drill.sh` — failover drill
- `scripts/teardown_ha_local.sh` — teardown
- `Makefile` — three new targets + help entries
- `docs/implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md` — this artifact
- `docs/production-readiness-v2/09-ha-roadmap.md` — updated HA-2 status
- `docs/production-readiness-v2/manual-failover-runbook.md` — referenced local simulation
- `docs/production-readiness-v2/ha-adr.md` — noted local simulation evidence
- `docs/production-readiness-v2/10-evidence-checklist.md` — added HA-B checklist items
- `docs/implementation-path/01-current-state.md` — added HA local evidence summary

---

## 9. Verdict

```text
make ha-local-setup: PASS
make ha-local-failover-drill: PASS
RTO: 3s
RPO: 0 rows lost
Production-ready: NO
Full G2: NOT COMPLETE
HA/multi-node: NO
Automated failover: NO
Block A closed: NO
```
