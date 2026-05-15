# 2026-05-12 — D.2 Partial-Resume Local Migration Rehearsal Evidence

> **Status**: LOCAL-ONLY REHEARSAL PASSED (with documented limitation).
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.
> Track: `117-postgresql-readiness-acceleration-plan.md` Track D.2.

## Scope

This artifact records a rehearsal of the `--resume` migration path for `ferrum-migrate --features postgres`. The goal is to verify that checkpoint-based resume correctly skips already-migrated tables and re-migrates tables that lack a valid checkpoint.

- Drill type: local-only partial-resume migration rehearsal
- Evidence target: P5e.1 checkpoint/resume idempotency, stale-checkpoint handling
- Production claim: **NO**
- Target-host operator blocker closure: **NO**
- PostgreSQL production deployment claim: **NO**
- HA/multi-node claim: **NO**

## Method & Limitation

**Ideal D.2**: Start a live migration, kill the OS process after roughly half the tables complete, then resume and verify the remaining tables are migrated while completed tables are skipped.

**Actual D.2**: The existing CLI completes a populated migration of 323 rows in well under one second, making deterministic live-process interruption impractical without code changes (e.g., artificial per-table delays). Instead, this rehearsal **deterministically simulates** the post-interruption state by:

1. Running a full migration (all tables + checkpoints created).
2. Deleting checkpoints for three tables (`approvals`, `ledger_entries`, `policy_bundles`).
3. Truncating those same three tables in the target.
4. Running `--resume`.
5. Verifying that tables with valid checkpoints are skipped and the three "lost" tables are re-migrated.

**Limitation**: This does **not** test a true OS-level process kill mid-migration. It tests the checkpoint/resume logic that would be exercised after such a kill. The limitation is recorded honestly; no claim is made about live-interruption behavior.

## Environment

| Field | Value |
|---|---|
| Date | 2026-05-12 |
| Repo | `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify` |
| Plan artifact | `117-postgresql-readiness-acceleration-plan.md` |
| PostgreSQL container | `ferrumgate_postgres_p2` (from `docker-compose.postgres.yml`) |
| Host port | `5432` |
| PostgreSQL image | `postgres:16` |
| SQLite fixture | `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` (reused from D.1) |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## Commands Run

### 1. Start local PostgreSQL

```bash
docker compose -f docker-compose.postgres.yml up -d postgres_p2
```

### 2. Full baseline migration (creates checkpoints)

```bash
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d1-rehearsal/populated_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 100 --json
```

**Baseline result**: `overall_success: true`; all 10 tables migrated with `count_match=true`, `hash_match=true`.

### 3. Simulate interruption state

Delete checkpoints for three tables and truncate their data:

```bash
psql -U ferrumgate_dev -d ferrumgate_p2_test -c \
  "DELETE FROM _migration_checkpoints WHERE table_name IN ('approvals', 'ledger_entries', 'policy_bundles');"

psql -U ferrumgate_dev -d ferrumgate_p2_test -c \
  "TRUNCATE TABLE approvals, ledger_entries, policy_bundles;"
```

**State after simulation**:

| Checkpoints remaining | 7 (`intents`, `proposals`, `capabilities`, `executions`, `rollback_contracts`, `provenance_events`, `provenance_edges`) |
| Tables truncated | 3 (`approvals`, `ledger_entries`, `policy_bundles`) |
| Tables with data intact | 7 |

### 4. Resume migration

```bash
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d1-rehearsal/populated_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 100 --resume --json
```

**Resume result**: `overall_success: true`.

### 5. Post-resume verification

All 10 tables verified via `psql`:

```text
       table        | count
--------------------+-------
 intents            |    50
 proposals          |    30
 capabilities       |    30
 executions         |    20
 rollback_contracts |    20
 approvals          |    20
 provenance_events  |    60
 provenance_edges   |    30
 ledger_entries     |    60
 policy_bundles     |     3
```

All 10 checkpoints present in `_migration_checkpoints`.

## Resume Result Detail

| Table | Source | Target | Migrated | Status |
|---|---|---|---|---|
| intents | 50 | 50 | 0 | skipped (checkpoint matched) |
| proposals | 30 | 30 | 0 | skipped (checkpoint matched) |
| capabilities | 30 | 30 | 0 | skipped (checkpoint matched) |
| executions | 20 | 20 | 0 | skipped (checkpoint matched) |
| rollback_contracts | 20 | 20 | 0 | skipped (checkpoint matched) |
| **approvals** | 20 | 20 | **20** | **re-migrated** |
| provenance_events | 60 | 60 | 0 | skipped (checkpoint matched) |
| provenance_edges | 30 | 30 | 0 | skipped (checkpoint matched) |
| **ledger_entries** | 60 | 60 | **60** | **re-migrated** |
| **policy_bundles** | 3 | 3 | **3** | **re-migrated** |

- Count match: `true` for all 10 tables.
- Hash match: `true` for all 10 tables (full hash recomputed for re-migrated tables).
- ID match: `true` for all tables with an `id_column`.

## Overall Verdict

```text
D.2 partial-resume local migration rehearsal: PASS (with limitation)
Checkpoint skip behavior: PASS
Stale-checkpoint re-migration: PASS
Content-hash validation after resume: PASS
True live-process interruption: NOT TESTED (deterministic simulation used)
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Cleanup

| Cleanup Item | Result |
|---|---|
| PostgreSQL container | stopped and removed after rehearsal |
| Temp fixture | retained at `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` |

## Remaining Work

- D.3–D.6 cadence per `117-postgresql-readiness-acceleration-plan.md`
- If future engineering prioritizes true live-interruption testing, consider adding a `--delay-ms-per-table` debug flag or using a much larger fixture so that migration duration exceeds human/PTY reaction time.
- Operator-owned target-host blockers were open at that time.

---

*Artifact created: 2026-05-12. D.2 Partial-Resume Rehearsal Evidence — local-only. True live-process interruption was not tested; checkpoint/resume logic was exercised via deterministic simulation. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
