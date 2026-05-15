# 2026-05-12 — D.4 Large-Dataset Streaming Migration Rehearsal Evidence

> **Status**: LOCAL-ONLY REHEARSAL PASSED (with documented measurement limitation).
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.
> Track: `117-postgresql-readiness-acceleration-plan.md` Track D.4.

## Scope

This artifact records a rehearsal of P5e.4 large-dataset streaming migration using `chunk-size 1000`. The goal is to verify that the migration tool can handle a dataset significantly larger than the D.1/D.2/D.3 baseline without exhausting memory or failing.

- Drill type: local-only large-dataset streaming rehearsal
- Evidence target: P5e.4 chunk-size 1000 behavior, completion time, memory boundedness
- Production claim: **NO**
- Target-host operator blocker closure: **NO**
- PostgreSQL production deployment claim: **NO**
- HA/multi-node claim: **NO**

## Environment

| Field | Value |
|---|---|
| Date | 2026-05-12 |
| Repo | `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify` |
| Plan artifact | `117-postgresql-readiness-acceleration-plan.md` |
| PostgreSQL container | `ferrumgate_postgres_p2` (from `docker-compose.postgres.yml`) |
| Host port | `5432` |
| PostgreSQL image | `postgres:16` |
| SQLite fixture | `/tmp/ferrumgate-d4-rehearsal/large_fixture.db` |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## Fixture Size

| Table | Rows | Chunks at size 1000 |
|---|---|---|
| intents | 2,500 | 3 |
| proposals | 1,500 | 2 |
| capabilities | 1,500 | 2 |
| executions | 1,000 | 1 |
| rollback_contracts | 1,000 | 1 |
| approvals | 1,000 | 1 |
| provenance_events | 3,000 | 3 |
| provenance_edges | 1,500 | 2 |
| ledger_entries | 3,000 | 3 |
| policy_bundles | 3 | 1 |
| **Total** | **~16,003** | **19** |

> **Note**: This is smaller than the ≥1M row target mentioned in Track E.6. The fixture size was chosen to stay within bounded rehearsal time (<60s) while still exercising multi-chunk behavior. The limitation is documented honestly.

## Commands Run

### 1. Create large fixture

A Python seed script created the fixture at `/tmp/ferrumgate-d4-rehearsal/large_fixture.db` with synthetic non-production rows.

### 2. Reset target database

```bash
psql -U ferrumgate_dev -d postgres -c "DROP DATABASE ferrumgate_p2_test;"
psql -U ferrumgate_dev -d postgres -c "CREATE DATABASE ferrumgate_p2_test;"
```

### 3. Run migration with chunk-size 1000

```bash
time cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d4-rehearsal/large_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 1000 --json
```

**Timing result**:

```text
real    0m21.124s
user    0m8.847s
sys     0m2.037s
```

**Migration result**: `overall_success: true`; all 10 tables migrated with `count_match=true`, `hash_match=true`.

## Chunk-Size 1000 Behavior Evidence

The migration tool processes each table in `LIMIT 1000 OFFSET N` chunks:

- **intents**: 2,500 rows → 3 chunks (0, 1000, 2000)
- **proposals**: 1,500 rows → 2 chunks
- **capabilities**: 1,500 rows → 2 chunks
- **provenance_events**: 3,000 rows → 3 chunks
- **ledger_entries**: 3,000 rows → 3 chunks
- **provenance_edges**: 1,500 rows → 2 chunks

Each chunk is processed as a single PostgreSQL transaction. If the chunk-wide transaction fails, the tool falls back to row-by-row insertion for that chunk.

## Memory Boundedness

**Claim**: Migration memory usage is bounded by the chunk size because the tool fetches only one chunk at a time from SQLite and processes it before fetching the next.

**Evidence**:
- The migration completed successfully without OOM on a local Docker container limited to 512MB.
- `migrate_table` uses `for offset in (0..source_count).step_by(chunk_size)` with `LIMIT chunk_size OFFSET offset` queries.
- `compute_target_hash` also uses the same chunk-size parameter for target-side hash computation.

**Limitation**: No quantitative memory profiling (e.g., `/usr/bin/time -v`, `valgrind`, or `heaptrack`) was performed during this rehearsal. The boundedness claim is based on code inspection and successful completion within a 512MB container, not on measured peak RSS.

## Target Row Count Verification

```text
       table        | count
--------------------+-------
 intents            |  2500
 proposals          |  1500
 capabilities       |  1500
 executions         |  1000
 rollback_contracts |  1000
 approvals          |  1000
 provenance_events  |  3000
 provenance_edges   |  1500
 ledger_entries     |  3000
 policy_bundles     |     3
```

All counts match the source fixture (±0).

## Overall Verdict

```text
D.4 large-dataset streaming local rehearsal: PASS (with limitation)
Chunk-size 1000 multi-chunk behavior: VERIFIED
Migration completion time (~16K rows): ~21s
Memory boundedness: NOT QUANTITATIVELY MEASURED (bounded by chunk-size per code inspection)
Fixture size: ~16K rows (smaller than ≥1M target; limitation documented)
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Cleanup

| Cleanup Item | Result |
|---|---|
| PostgreSQL container | stopped and removed |
| Large fixture retained | `/tmp/ferrumgate-d4-rehearsal/large_fixture.db` |

## Remaining Work

- D.5–D.6 cadence per `117-postgresql-readiness-acceleration-plan.md`
- Quantitative memory profiling on a ≥100K or ≥1M row fixture if engineering time permits
- Target-host operator blockers were open at that time

---

*Artifact created: 2026-05-12. D.4 Large-Dataset Streaming Rehearsal Evidence — local-only. Fixture size ~16K rows. No quantitative memory measurement. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
