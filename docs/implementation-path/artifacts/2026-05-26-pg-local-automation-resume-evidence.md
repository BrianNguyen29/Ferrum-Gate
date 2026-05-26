# PG Local Automation + Resume Evidence — 2026-05-26

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-26 PostgreSQL local automation and resume-simulation runs.
> **Owner**: Engineering
> **Date**: 2026-05-26
> **Scope**: Local Docker PostgreSQL only; deterministic synthetic SQLite fixture; no target-host or production claims
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | All runs executed on a local workstation with Docker Compose PostgreSQL only. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | These are engineering-owned local drills only. |
| **Block A closed** | **NO** | Real owned domain is still required for production-ready or full G2 closure. |
| **PostgreSQL production deployment** | **NO** | The target remains local Docker PostgreSQL, not target-host or managed PostgreSQL. |
| **HA / multi-node** | **NO** | No replica, failover, or multi-node behavior was exercised. |
| **True live interruption recovery proven** | **NO** | The resume drill is a deterministic partial-failure simulation, not a real OS-level process kill mid-migration. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-26 |
| Host scope | Local development workstation |
| PostgreSQL container | `ferrumgate_postgres_p2` |
| PostgreSQL database (source) | `ferrumgate_p2_test` |
| PostgreSQL database (offsite restore drill) | `ferrumgate_pg_offsite_restore_drill` |
| PostgreSQL port | `5432` |
| SQLite fixture source | `scripts/seed_pg_local_fixture.py` |
| Retention policy simulated | `find ... -name 'ferrumgate_*.dump' -mtime +4 -delete` |

Deterministic fixture row counts used in both runs:

| Table | Rows |
|-------|-----:|
| `intents` | 1 |
| `proposals` | 1 |
| `capabilities` | 1 |
| `executions` | 1 |
| `rollback_contracts` | 1 |
| `approvals` | 1 |
| `provenance_events` | 2 |
| `provenance_edges` | 1 |
| `ledger_entries` | 2 |
| `policy_bundles` | 1 |

---

## 2. Run Summary

| # | Run | Command | Verdict |
|---|-----|---------|---------|
| 2.1 | Local PostgreSQL backup/retention/offsite wrapper | `make pg-backup-retention-drill` | ✅ PASS |
| 2.2 | Local PostgreSQL partial-failure/resume simulation | `make pg-partial-failure-drill` | ✅ PASS |

---

## 3. Detailed Results

### 3.1 Local PostgreSQL Backup/Retention/Offsite Wrapper (2.1)

**Command:**

```bash
make pg-backup-retention-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (`docker`, `docker compose`, `cargo`, `python3`) | ✅ PASS |
| Local PostgreSQL container healthy | ✅ PASS |
| Populated PostgreSQL source prepared via `ferrum-migrate` | ✅ PASS |
| Old retention simulation files created | ✅ PASS |
| `pg_dump -Fc --no-owner --no-privileges` archive created | ✅ PASS |
| `pg_restore -l` listed expected FerrumGate tables | ✅ PASS |
| Old matching dump file pruned | ✅ PASS |
| Nonmatching old dump file preserved | ✅ PASS |
| Current backup file preserved | ✅ PASS |
| Offsite copy hash matched local backup | ✅ PASS |
| Fresh offsite restore drill database created | ✅ PASS |
| Offsite copy restored successfully | ✅ PASS |
| Restored public table count | **11** |
| Core-table source vs restored row counts | **10 / 10 matched** |

**Backup artifact details:**

| Field | Value |
|-------|-------|
| Host temp path | `/tmp/tmp.q6BRpkOn7N/backups/ferrumgate_local_20260526T005937Z.dump` |
| Size | `21722` bytes |
| SHA-256 | `c303815bd994b6eed0df512f551470fd8fb95481621fb6b5241928c637ca51de` |

**Retention/offsite observations:**

| Check | Result |
|-------|--------|
| `ferrumgate_old_20260501.dump` removed by retention simulation | ✅ YES |
| `other_service_20260501.dump` preserved | ✅ YES |
| Local/offsite SHA-256 match | ✅ YES |

**Core-table count comparison after offsite restore:**

| Table | Source count | Restored count | Match |
|-------|-------------:|---------------:|-------|
| `intents` | 1 | 1 | ✅ |
| `proposals` | 1 | 1 | ✅ |
| `capabilities` | 1 | 1 | ✅ |
| `executions` | 1 | 1 | ✅ |
| `rollback_contracts` | 1 | 1 | ✅ |
| `approvals` | 1 | 1 | ✅ |
| `provenance_events` | 2 | 2 | ✅ |
| `provenance_edges` | 1 | 1 | ✅ |
| `ledger_entries` | 2 | 2 | ✅ |
| `policy_bundles` | 1 | 1 | ✅ |

**Summary:** Passed 20, Failed 0, Skipped 0.

> **Note:** The exact byte size and archive SHA-256 may vary between reruns because `pg_dump -Fc` embeds run-specific metadata such as creation timestamps. The pass/fail contract for this drill is based on archive creation, listability, retention behavior, local-vs-offsite hash parity within the same run, and restore fidelity.

**Interpretation:** The repository now has a runnable local wrapper for three previously separate manual PG-local checks: backup creation, retention pruning simulation, and offsite copy integrity. The wrapper also restores from the simulated offsite copy into a fresh drill database and verifies row-count parity across all 10 core tables.

---

### 3.2 Local PostgreSQL Partial-Failure / Resume Simulation (2.2)

**Command:**

```bash
make pg-partial-failure-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (`docker`, `docker compose`, `cargo`, `python3`) | ✅ PASS |
| Local PostgreSQL container healthy | ✅ PASS |
| Deterministic SQLite fixture created | ✅ PASS |
| Baseline migration completed | ✅ PASS |
| Checkpoints remaining after simulation | **7** |
| Selected tables truncated before resume | ✅ PASS |
| Resume migration completed | ✅ PASS |
| Expected re-migrated tables | `approvals`, `ledger_entries`, `policy_bundles` |
| Expected skipped tables | `intents`, `proposals`, `capabilities`, `executions`, `rollback_contracts`, `provenance_events`, `provenance_edges` |
| Checkpoints after resume | **10** |
| Final deterministic row counts | **10 / 10 matched** |

**Resume detail:**

| Table | Source | Target | Migrated | Status |
|-------|-------:|-------:|---------:|--------|
| `intents` | 1 | 1 | 0 | skipped |
| `proposals` | 1 | 1 | 0 | skipped |
| `capabilities` | 1 | 1 | 0 | skipped |
| `executions` | 1 | 1 | 0 | skipped |
| `rollback_contracts` | 1 | 1 | 0 | skipped |
| `approvals` | 1 | 1 | 1 | re-migrated |
| `provenance_events` | 2 | 2 | 0 | skipped |
| `provenance_edges` | 1 | 1 | 0 | skipped |
| `ledger_entries` | 2 | 2 | 2 | re-migrated |
| `policy_bundles` | 1 | 1 | 1 | re-migrated |

**Final row-count verification:**

| Table | Expected | Actual | Match |
|-------|---------:|-------:|-------|
| `intents` | 1 | 1 | ✅ |
| `proposals` | 1 | 1 | ✅ |
| `capabilities` | 1 | 1 | ✅ |
| `executions` | 1 | 1 | ✅ |
| `rollback_contracts` | 1 | 1 | ✅ |
| `approvals` | 1 | 1 | ✅ |
| `provenance_events` | 2 | 2 | ✅ |
| `provenance_edges` | 1 | 1 | ✅ |
| `ledger_entries` | 2 | 2 | ✅ |
| `policy_bundles` | 1 | 1 | ✅ |

**Summary:** Passed 13, Failed 0, Skipped 0.

**Interpretation:** The current `--resume` path can be exercised locally in a deterministic way by deleting three checkpoints and truncating the corresponding target tables. On rerun, `ferrum-migrate` skips the seven tables whose checkpoints still match source counts and re-migrates only the three simulated-loss tables. This validates checkpoint skip behavior and stale-checkpoint recovery logic, but it is **not** a true live process interruption test.

---

## 4. Consolidated Interpretation

- The repository now has automated local wrappers for:
  - populated backup + retention-pruning + offsite-copy simulation, and
  - deterministic partial-failure / resume migration behavior.
- This extends the local PostgreSQL evidence set beyond the earlier migration/restore mechanics by covering:
  - local retention/offsite operational mechanics, and
  - the `ferrum-migrate --resume` recovery path.
- Both runs remain engineering-owned local evidence only. They do **not** prove target-host scheduling, real offsite transport, live process interruption recovery, or PostgreSQL production readiness.

---

## 5. Known Gaps

- Local Docker PostgreSQL only; no target-host or managed PostgreSQL evidence.
- Retention pruning uses backdated local files; no real scheduler or long-lived backup set.
- Offsite sync uses local filesystem copy only; no GCS/S3/rsync/SFTP or credential path tested.
- Resume simulation is deterministic; no actual process kill mid-migration was attempted.
- No real-domain, HTTPS, or Block A closure.

---

## 6. Verdict

```text
PG local automation + resume batch: PASS
PG backup/retention/offsite wrapper: PASS
PG partial-failure/resume simulation: PASS
Production-ready: NO
Full G2: NOT COMPLETE
PostgreSQL production deployment: NO
HA/multi-node: NO
Block A closed: NO
```
