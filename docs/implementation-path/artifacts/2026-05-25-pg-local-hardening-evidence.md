# PG Local Hardening Evidence — 2026-05-25

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-25 PostgreSQL local hardening runs.
> **Owner**: Engineering
> **Date**: 2026-05-25
> **Scope**: Local Docker PostgreSQL only; deterministic synthetic SQLite fixture; no target-host or production claims
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | All runs were executed on a local workstation using Docker Compose PostgreSQL. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | These are engineering-owned local drills only. |
| **Block A closed** | **NO** | A real owned domain is still required for production-ready or full G2 closure. |
| **PostgreSQL production deployment** | **NO** | The database target is the local `postgres_p2` Docker service only. |
| **HA / multi-node** | **NO** | No failover, replica, or multi-node behavior was exercised. |
| **Target-host backup automation complete** | **NO** | This batch validates local mechanics only, not operator-owned scheduled backup execution. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-25 |
| Host scope | Local development workstation |
| PostgreSQL container | `ferrumgate_postgres_p2` |
| PostgreSQL database (source) | `ferrumgate_p2_test` |
| PostgreSQL database (restore drill) | `ferrumgate_pg_restore_drill` |
| PostgreSQL port | `5432` |
| ferrumd migration-drill bind | `127.0.0.1:19087` |
| ferrumd restore-drill bind | `127.0.0.1:19086` |
| SQLite fixture source | `scripts/seed_pg_local_fixture.py` |

Synthetic fixture row counts used in both drills:

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
| 2.1 | Local SQLite → PostgreSQL migration drill | `make pg-migration-drill` | ✅ PASS |
| 2.2 | Local PostgreSQL populated backup/restore drill | `make pg-restore-drill` | ✅ PASS |

---

## 3. Detailed Results

### 3.1 Local SQLite → PostgreSQL Migration Drill (2.1)

**Command:**

```bash
make pg-migration-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (`docker`, `docker compose`, `cargo`, `python3`, `curl`) | ✅ PASS |
| Local PostgreSQL container healthy | ✅ PASS |
| Deterministic populated SQLite fixture created | ✅ PASS |
| `ferrum-migrate --apply --json` completed | ✅ PASS |
| Migrated tables | **10 / 10** count match + hash match |
| `ferrumd` `/v1/readyz/deep` against migrated PostgreSQL | ✅ PASS |

**Per-table migration summary:**

| Table | Source | Target | Migrated | Count match | Hash match |
|-------|-------:|-------:|---------:|-------------|------------|
| `intents` | 1 | 1 | 1 | true | true |
| `proposals` | 1 | 1 | 1 | true | true |
| `capabilities` | 1 | 1 | 1 | true | true |
| `executions` | 1 | 1 | 1 | true | true |
| `rollback_contracts` | 1 | 1 | 1 | true | true |
| `approvals` | 1 | 1 | 1 | true | true |
| `provenance_events` | 2 | 2 | 2 | true | true |
| `provenance_edges` | 1 | 1 | 1 | true | true |
| `ledger_entries` | 2 | 2 | 2 | true | true |
| `policy_bundles` | 1 | 1 | 1 | true | true |

**Summary:** Passed 11, Failed 0, Skipped 0.

**Interpretation:** The current `ferrum-migrate` binary can migrate a small but populated FerrumGate SQLite fixture into the local PostgreSQL target with content-hash preservation across all 10 core governance tables. `ferrumd` then reports `readyz/deep` healthy against the migrated PostgreSQL target. This is local engineering evidence only.

---

### 3.2 Local PostgreSQL Populated Backup/Restore Drill (2.2)

**Command:**

```bash
make pg-restore-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (`docker`, `docker compose`, `cargo`, `python3`, `curl`) | ✅ PASS |
| Local PostgreSQL container healthy | ✅ PASS |
| Populated PostgreSQL source prepared via `ferrum-migrate` | ✅ PASS |
| `pg_dump -Fc --no-owner --no-privileges` archive created | ✅ PASS |
| Backup artifact copied to host temp path | ✅ PASS |
| `pg_restore -l` listed expected FerrumGate tables | ✅ PASS |
| Fresh drill database created | ✅ PASS |
| `pg_restore` into drill database completed | ✅ PASS |
| Restored public table count | **11** |
| Core-table source vs restored row counts | **10 / 10 matched** |
| `ferrumd` `/v1/readyz/deep` against restored PostgreSQL | ✅ PASS |

**Backup artifact details:**

| Field | Value |
|-------|-------|
| Host temp path | `/tmp/tmp.S3s5jf1d38/ferrumgate_pg_restore_drill.dump` |
| Size | `21719` bytes |
| SHA-256 | `54742adc6cf729f4fa11f4d92a423b536dc045ae3ef21ef904e4a4252381d16f` |

**Core-table count comparison:**

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

**Summary:** Passed 17, Failed 0, Skipped 0.

**Interpretation:** The local PostgreSQL backup/restore path now has a repeatable populated-data drill, not just a schema-only smoke. The archive is listable, restore succeeds into a fresh drill database, row counts match across all 10 core tables, and `ferrumd` reports `readyz/deep` healthy against the restored database.

---

## 4. Consolidated Interpretation

- The repository now contains runnable local drills for both:
  - populated SQLite → PostgreSQL migration, and
  - populated PostgreSQL backup/restore.
- This strengthens earlier local Docker PostgreSQL evidence by moving beyond schema-only smoke and documenting a repeatable populated synthetic dataset path.
- The drills remain engineering-owned local evidence only. They do **not** prove operator-host deployment, production TLS/DSN hardening, scheduled backup execution, or HA behavior.

---

## 5. Known Gaps

- Local Docker PostgreSQL only; no target-host or managed PostgreSQL evidence.
- No long-running workload, concurrency, or failover proof beyond the separate PG restart drill.
- No production backup scheduler, retention pruning on the real target, or offsite sync closure from this batch alone.
- No real-domain, HTTPS, or Block A closure.
- Synthetic fixture is intentionally small; it validates mechanics, not production data volume.

---

## 6. Verdict

```text
PG local hardening batch: PASS
SQLite -> PostgreSQL populated migration drill: PASS
PostgreSQL populated backup/restore drill: PASS
ferrumd deep readiness against migrated/restored PostgreSQL: PASS
Production-ready: NO
Full G2: NOT COMPLETE
PostgreSQL production deployment: NO
HA/multi-node: NO
Block A closed: NO
```
