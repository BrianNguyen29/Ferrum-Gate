# 2026-05-12 — D.3 Content-Hash Validation Local Rehearsal Evidence

> **Status**: LOCAL-ONLY REHEARSAL PASSED.
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.
> Track: `117-postgresql-readiness-acceleration-plan.md` Track D.3.

## Scope

This artifact records a focused rehearsal of P5e.3 content-hash validation: verifying that `source_content_hash == target_content_hash` for every core governance table after migration.

- Drill type: local-only content-hash validation cadence
- Evidence target: P5e.3 SHA-256 aggregate content-hash correctness and determinism
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
| SQLite fixture | `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` (reused from D.1) |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## How Content-Hash Validation Works

`ferrum-migrate` computes an aggregate SHA-256 content hash for each table:

1. **Canonicalize each row**: `column1=value1;column2=value2;...` ordered by select columns. NULL renders as `NULL`. Booleans (`auto_commit`, `active`) and integers (`step_index`, `entry_id`) are normalized for SQLite/PostgreSQL type differences.
2. **Per-row hash**: SHA-256 of the canonical string.
3. **Aggregate hash**: Sort all per-row hashes, join with `\n`, SHA-256 again.

This makes the aggregate hash **order-independent** and **type-canonicalized** across SQLite source and PostgreSQL target.

## Commands Run

### 1. Reset target database

```bash
psql -U ferrumgate_dev -d postgres -c "DROP DATABASE ferrumgate_p2_test;"
psql -U ferrumgate_dev -d postgres -c "CREATE DATABASE ferrumgate_p2_test;"
```

### 2. Run migration with hash output

```bash
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d1-rehearsal/populated_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 100 --json
```

## Content-Hash Evidence

| Table | Source Hash | Target Hash | Match |
|---|---|---|---|
| intents | `a6c440a1a3eed4b2dd58cff73ba6de44102512c7117e44396f97763884454de4` | `a6c440a1a3eed4b2dd58cff73ba6de44102512c7117e44396f97763884454de4` | ✅ |
| proposals | `bd23e49734ebc414bf4eed58a0c4df97c481603168356f4c1e4ea5044dcd5946` | `bd23e49734ebc414bf4eed58a0c4df97c481603168356f4c1e4ea5044dcd5946` | ✅ |
| capabilities | `68b1fb2c95d409b7a3f2fd4651fd564649f8ec09d80129a845de0bf401ea38a8` | `68b1fb2c95d409b7a3f2fd4651fd564649f8ec09d80129a845de0bf401ea38a8` | ✅ |
| executions | `ae273d8a1eac87b0d26f3c567224b76e5b15fb13d2ddffd26628aee59692b647` | `ae273d8a1eac87b0d26f3c567224b76e5b15fb13d2ddffd26628aee59692b647` | ✅ |
| rollback_contracts | `641ae6a175a78c6e914d82b0f88b02ab42a31ddfc5b7bb6a8ff64ac17fc36823` | `641ae6a175a78c6e914d82b0f88b02ab42a31ddfc5b7bb6a8ff64ac17fc36823` | ✅ |
| approvals | `bd3acf869770045f967cd9c7f9433044e126956a78c0a917e01f64641810d2fe` | `bd3acf869770045f967cd9c7f9433044e126956a78c0a917e01f64641810d2fe` | ✅ |
| provenance_events | `6d68b2740126241184e90e19d66664b8a4f0eb9e3ba560800f5c2339927a031a` | `6d68b2740126241184e90e19d66664b8a4f0eb9e3ba560800f5c2339927a031a` | ✅ |
| provenance_edges | `a6d2cc2e1d09d6978728daa27475a1528eead15063460533139b8dd29e3164fc` | `a6d2cc2e1d09d6978728daa27475a1528eead15063460533139b8dd29e3164fc` | ✅ |
| ledger_entries | `282494b4beb8889d17b3ee47143118c2ad9ca85959df44702f1eae1d13a2a281` | `282494b4beb8889d17b3ee47143118c2ad9ca85959df44702f1eae1d13a2a281` | ✅ |
| policy_bundles | `2535e179b9cea3cf5cecd4ed1f9b523f7fe265ebf8d1221603885e39a377af2e` | `2535e179b9cea3cf5cecd4ed1f9b523f7fe265ebf8d1221603885e39a377af2e` | ✅ |

## Overall Verdict

```text
D.3 content-hash validation local rehearsal: PASS
Source vs target content-hash match: 10/10 tables
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Cleanup

PostgreSQL container stopped and removed after rehearsal.

## Remaining Work

- D.4–D.6 cadence per `117-postgresql-readiness-acceleration-plan.md`
- Target-host operator blockers remain open
- Large-dataset hash validation (≥1M rows) deferred to future readiness work

---

*Artifact created: 2026-05-12. D.3 Content-Hash Validation Rehearsal Evidence — local-only. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
