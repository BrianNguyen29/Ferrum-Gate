# PG Local Batch + Scheduled Timer Simulation Evidence — 2026-05-26

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-26 `make pg-local-batch` full run and `make pg-scheduled-timer-simulation` pass.
> **Owner**: Engineering
> **Date**: 2026-05-26
> **Scope**: Local Makefile wiring and text-level timer simulation only; no target-host or production claims
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | All evidence is local Makefile wiring and text-level simulation only. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Engineering-owned local evidence only. |
| **Block A closed** | **NO** | Real owned domain is still required for production-ready or full G2 closure. |
| **PostgreSQL production deployment** | **NO** | Local Docker PostgreSQL only for the heavy drills; timer simulation uses no Docker at all. |
| **HA / multi-node** | **NO** | No replica, failover, or multi-node behavior was exercised. |
| **Live systemd timer installed** | **NO** | Timer simulation is text-only; no units were installed or enabled on the host. |
| **Heavy batch run completed in this artifact** | **YES — LOCAL ONLY** | `make pg-local-batch` completed all four heavy local Docker PG drills plus the timer simulation on 2026-05-26. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-26 |
| Host scope | Local development workstation |
| make version | GNU Make 4.3 |
| bash version | 5.1.16 |
| python3 version | 3.10.x |

---

## 2. New Make Targets

### 2.1 `make pg-scheduled-timer-simulation`

**Purpose**: Lightweight text-only simulation of a systemd timer/service backup schedule. Validates unit file text, required fields, and due/skip behavior without installing systemd units or touching host schedules.

**Command:**

```bash
make pg-scheduled-timer-simulation
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (`date`, `python3`) | ✅ PASS |
| Service unit text generated | ✅ PASS |
| Timer unit text generated | ✅ PASS |
| Service unit has `[Service]` section | ✅ PASS |
| Service unit `Type=oneshot` | ✅ PASS |
| Service unit has `ExecStart` | ✅ PASS |
| Timer unit has `[Timer]` section | ✅ PASS |
| Timer unit has `OnCalendar` | ✅ PASS |
| Timer unit `Persistent=true` | ✅ PASS |
| No prior run state → due (simulated) | ✅ PASS |
| Prior run today → skip (simulated) | ✅ PASS |
| Prior run yesterday → due (simulated) | ✅ PASS |
| Lockfile exists → skip (simulated) | ✅ PASS |
| Lockfile absent → eligible (simulated) | ✅ PASS |
| `OnCalendar=daily` valid expression | ✅ PASS |
| `Persistent=true` behavior documented | ✅ PASS |
| Backup script path well-formed | ✅ PASS |

**Summary:** Passed 18, Failed 0, Skipped 0.

**Interpretation:** The repository now has a runnable local simulation that validates the shape of a backup timer/service pair and exercises simple due/skip logic. This is not a substitute for real systemd installation or host schedule validation, but it provides a fast local gate that can be run without Docker or elevated privileges.

---

### 2.2 `make pg-local-batch`

**Purpose**: Aggregate Make target that runs all existing local PostgreSQL drills in a deterministic order, followed by the lightweight scheduled timer simulation.

**Order:**

1. `pg-migration-drill`
2. `pg-restore-drill`
3. `pg-backup-retention-drill`
4. `pg-partial-failure-drill`
5. `pg-scheduled-timer-simulation`

**Command:**

```bash
make pg-local-batch
```

**Makefile wiring:**

The target is declared in the top-level `Makefile` as:

```makefile
pg-local-batch:
	@echo "Running local PostgreSQL batch: migration, restore, backup/retention, partial-failure, timer simulation..."
	@$(MAKE) pg-migration-drill && \
	$(MAKE) pg-restore-drill && \
	$(MAKE) pg-backup-retention-drill && \
	$(MAKE) pg-partial-failure-drill && \
	$(MAKE) pg-scheduled-timer-simulation
	@echo "PG LOCAL BATCH: ALL TARGETS PASSED"
```

**Help listing:**

```text
make pg-scheduled-timer-simulation - local text-only systemd timer due/skip simulation (no install)
make pg-local-batch - run all local PostgreSQL drills + timer simulation in deterministic order
```

**Full batch result:**

```text
PG MIGRATION DRILL: ALL CHECKS PASSED
PG RESTORE DRILL: ALL CHECKS PASSED
PG BACKUP/RETENTION/OFFSITE DRILL: ALL CHECKS PASSED
PG PARTIAL-FAILURE/RESUME DRILL: ALL CHECKS PASSED
PG SCHEDULED TIMER SIMULATION: ALL CHECKS PASSED
PG LOCAL BATCH: ALL TARGETS PASSED
```

**Batch drill summaries:**

| Target | Summary |
|--------|---------|
| `pg-migration-drill` | Passed 11, Failed 0, Skipped 0 |
| `pg-restore-drill` | Passed 17, Failed 0, Skipped 0 |
| `pg-backup-retention-drill` | Passed 20, Failed 0, Skipped 0 |
| `pg-partial-failure-drill` | Passed 13, Failed 0, Skipped 0 |
| `pg-scheduled-timer-simulation` | Passed 18, Failed 0, Skipped 0 |

**Latest backup artifacts observed during the batch:**

| Drill | Path | Size | SHA-256 |
|-------|------|------|---------|
| `pg-restore-drill` | `/tmp/tmp.cXLHxaYTAF/ferrumgate_pg_restore_drill.dump` | `21722` bytes | `361982800fb5f26af4ccc29f4b6ef78189da7b6ade7aa959995f4f81a9305678` |
| `pg-backup-retention-drill` | `/tmp/tmp.y3mnumsj8k/backups/ferrumgate_local_20260526T011549Z.dump` | `21725` bytes | `dd651cb8e4516123f14c54ad92684f30a1e1de0cf4daf49b21c2922096d59708` |

> **Note:** Exact `pg_dump -Fc` archive size and SHA-256 may vary between reruns because archive metadata can include run-specific values such as creation timestamps. In-run source/offsite hash parity and restore fidelity remain the authoritative checks.

**Interpretation:** The aggregate target wires together the four heavy Docker-based PG drills and the lightweight timer simulation into a single deterministic sequence, and the full sequence passed locally on 2026-05-26.

---

## 3. Consolidated Interpretation

- The repository now has:
  - a `pg-local-batch` aggregate target that runs all local PG drills in order, and
  - a `pg-scheduled-timer-simulation` target that validates timer/service unit text and simulates due/skip behavior without Docker or systemd install.
- The timer simulation is intentionally lightweight: it can be run on any workstation with `bash` and `python3`, making it suitable for quick local gates and pre-commit checks.
- Both targets remain local-only engineering evidence. They do not prove target-host scheduling, real systemd behavior, live process interruption recovery, or PostgreSQL production readiness.

---

## 4. Known Gaps

- `pg-local-batch` includes four heavy Docker-based drills that require Docker, cargo, and local build time; the aggregate target is for manual/optional use, not CI.
- Timer simulation is text-level only; no systemd units are installed, enabled, or started.
- No target-host or managed PostgreSQL evidence.
- No real-domain, HTTPS, or Block A closure.

---

## 5. Verdict

```text
make pg-scheduled-timer-simulation: PASS
make pg-local-batch full run: PASS
Production-ready: NO
Full G2: NOT COMPLETE
PostgreSQL production deployment: NO
HA/multi-node: NO
Block A closed: NO
```
