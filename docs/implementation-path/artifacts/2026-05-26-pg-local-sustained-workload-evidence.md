# PG Local Sustained Workload Evidence — 2026-05-26

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-26 `make pg-sustained-workload-drill` local run.
> **Owner**: Engineering
> **Date**: 2026-05-26
> **Scope**: Local Docker PostgreSQL + ferrumd + workload generator only; no target-host or production claims.
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local Docker Compose only; no managed or production PostgreSQL. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Engineering-owned local evidence only. |
| **Block A closed** | **NO** | Real owned domain still required for production-ready or full G2 closure. |
| **PostgreSQL production deployment** | **NO** | Local Docker PostgreSQL only. |
| **HA / multi-node** | **NO** | No replica, failover, or multi-node behavior exercised. |
| **Canonical SLO certification** | **NO** | Workload is short and bounded; not a full SLO certification run. |
| **Sustained workload default** | **SHORT / BOUNDED** | Default is 30 s @ 1 rps (~30 requests). Env override available for longer runs. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-26 |
| Host scope | Local development workstation |
| Docker version | 29.3.1 |
| make version | GNU Make 4.3 |
| bash version | 5.1.16 |
| python3 version | 3.10.x |
| ferrumd binary | target/debug/ferrumd (postgres feature) |
| ferrum-migrate binary | target/debug/ferrum-migrate (postgres feature) |

---

## 2. Procedure

### 2.1 Command

```bash
make pg-sustained-workload-drill
```

### 2.2 Script

`scripts/run_pg_sustained_workload_drill.sh`

Steps performed:
1. Starts `docker-compose.postgres.yml` (`ferrumgate_postgres_p2`).
2. Waits for PostgreSQL healthcheck (`healthy`).
3. Builds/uses `ferrum-migrate` and `ferrumd` with `--features postgres`.
4. Seeds deterministic SQLite fixture via `scripts/seed_pg_local_fixture.py`.
5. Migrates fixture into PostgreSQL via `ferrum-migrate --apply`.
6. Starts `ferrumd` with `FERRUMD_STORE_DSN` pointing to local PG and `FERRUMD_AUTH_MODE=disabled`.
7. Pre-creates SQLite table for workload generator's `sqlite` adapter.
8. Executes short sustained workload via `scripts/run_real_workload_generator.py --execute`:
   - Default phases: `[{"name":"sustained","duration_sec":30,"rate_rps":1.0}]`
   - Default adapter mix: `fs`, `sqlite`, `maildraft` (excludes `http` and `git` for offline safety).
   - Bearer token: `dummy` (auth disabled; token ignored).
   - Ready probes during phase: every 10 s.
   - Post-workload ready probes: 3 probes at 5 s intervals.
9. Verifies:
   - Post-workload `/v1/readyz/deep` returns 200.
   - `/v1/metrics` contains PostgreSQL pool metrics (`ferrumgate_store_pg_pool_size`).
   - Workload results show zero errors and all 2xx responses.
10. Stops ferrumd and tears down container (unless `--no-cleanup`).

---

## 3. Expected vs Observed

| Check | Expected | Observed | Result |
|-------|----------|----------|--------|
| PostgreSQL container healthy | healthy within 30 s | healthy after ~15 s | ✅ PASS |
| ferrum-migrate binary available | exists | exists | ✅ PASS |
| ferrumd binary available | exists | exists | ✅ PASS |
| Fixture created | 10 tables, counts >0 | 10 tables, counts match | ✅ PASS |
| Migration apply | 10/10 count+hash match | 10/10 count+hash match | ✅ PASS |
| ferrumd readyz/deep | 200 within 30 s | 200 | ✅ PASS |
| sqlite prerequisite table | created | created | ✅ PASS |
| Workload generator completes | exit 0 | exit 0 | ✅ PASS |
| Total requests | ~30 | 29 | ✅ PASS |
| Status distribution | all 2xx | `{'200': 29}` | ✅ PASS |
| Phase errors | 0 | 0 | ✅ PASS |
| Mid-run readyz probes | 200 | 200 (2 probes) | ✅ PASS |
| Post-workload readyz/deep | 200 | 200 | ✅ PASS |
| Post-workload metrics body | non-empty | non-empty | ✅ PASS |
| PG pool metrics present | `ferrumgate_store_pg_pool_size` present | present | ✅ PASS |
| No 5xx / unexpected errors | none | none | ✅ PASS |

---

## 4. Summary

```text
Passed:  17
Failed:  0
Skipped: 0
Drill dir: /tmp/tmp.Sid6OV3UCA
```

**Verdict**: `PG SUSTAINED WORKLOAD DRILL: ALL CHECKS PASSED`

---

## 5. Interpretation

- The repository now has a standalone local PostgreSQL sustained workload drill that:
  - starts a fresh Docker PostgreSQL container,
  - migrates a deterministic fixture,
  - starts ferrumd against PostgreSQL,
  - runs a short bounded request workload,
  - verifies readiness and PG pool metrics, and
  - emits a clear PASS/FAIL summary.
- Default duration is intentionally short (30 s, ~30 requests) so the drill can be run as a quick local gate.
- Environment variables (`SUSTAINED_PHASES`, `SUSTAINED_ADAPTER_MIX`) allow operators to override duration and adapter mix for longer or targeted runs.
- The drill is included in `make pg-local-batch` between `pg-partial-failure-drill` and `pg-scheduled-timer-simulation`, adding an incremental bounded runtime to the aggregate target.
- This remains local-only engineering evidence. It does not prove target-host behavior, production PostgreSQL readiness, or full SLO certification.

---

## 6. Known Gaps

- Workload duration is short by design; not a substitute for extended soak testing.
- No circuit breaker or reconnect stress is exercised (PG-2.3b deferred).
- No target-host or managed PostgreSQL evidence.
- No real-domain, HTTPS, or Block A closure.

---

## 7. Makefile Wiring

New target:

```makefile
pg-sustained-workload-drill:
	@echo "Running local PostgreSQL sustained workload drill..."
	@bash scripts/run_pg_sustained_workload_drill.sh
```

Updated aggregate target (`pg-local-batch`) order:

1. `pg-migration-drill`
2. `pg-restore-drill`
3. `pg-backup-retention-drill`
4. `pg-partial-failure-drill`
5. **`pg-sustained-workload-drill`**
6. `pg-scheduled-timer-simulation`

Help entry:

```text
make pg-sustained-workload-drill - local PostgreSQL sustained workload drill (short default, env override for longer)
```
