# 2026-05-12 — D.1 Populated Local SQLite → PostgreSQL Migration Rehearsal Evidence

> **Status**: LOCAL-ONLY REHEARSAL PASSED.
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.
> Track: `117-postgresql-readiness-acceleration-plan.md` Track D.1.

## Scope

This artifact records the engineering-owned re-run of a populated SQLite fixture migrating into a local Docker PostgreSQL instance using `ferrum-migrate --features postgres`.

- Drill type: local-only populated-data migration rehearsal
- Evidence target: P5e streaming migration, row-count preservation, content-hash validation, resume/idempotency
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
| SQLite fixture | `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## Populated SQLite Fixture

The fixture was created under `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` using deterministic synthetic non-production rows inserted via a Python seed script against the FerrumGate SQLite schema.

| Table | Source Rows |
|---|---|---:|
| intents | 50 |
| proposals | 30 |
| capabilities | 30 |
| executions | 20 |
| rollback_contracts | 20 |
| approvals | 20 |
| provenance_events | 60 |
| provenance_edges | 30 |
| ledger_entries | 60 |
| policy_bundles | 3 |

Synthetic data only; no real user data, production data, bearer token, or external credential was used.

## Commands Run

### 1. Start local PostgreSQL

```bash
docker compose -f docker-compose.postgres.yml up -d postgres_p2
```

### 2. Run migration (apply)

```bash
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d1-rehearsal/populated_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 100
```

Password in the actual command was provided via the CLI; it is redacted here.

### 3. Run migration with `--resume` (idempotency verification)

```bash
cargo run --package ferrum-migrate --features postgres -- \
  --from "sqlite:///tmp/ferrumgate-d1-rehearsal/populated_fixture.db" \
  --to "postgres://ferrumgate_dev@localhost:5432/ferrumgate_p2_test" \
  --apply --chunk-size 100 --resume
```

## Migration Validation — First Pass (Empty Target)

Exit code: `0`
Overall success: `true`

```text
intents              source=  50 target=  50 migrated=  50 ids=match hash=match [OK]
proposals            source=  30 target=  30 migrated=  30 ids=match hash=match [OK]
capabilities         source=  30 target=  30 migrated=  30 ids=match hash=match [OK]
executions           source=  20 target=  20 migrated=  20 ids=match hash=match [OK]
rollback_contracts   source=  20 target=  20 migrated=  20 ids=match hash=match [OK]
approvals            source=  20 target=  20 migrated=  20 ids=match hash=match [OK]
provenance_events    source=  60 target=  60 migrated=  60 ids=match hash=match [OK]
provenance_edges     source=  30 target=  30 migrated=  30 ids=match hash=match [OK]
ledger_entries       source=  60 target=  60 migrated=  60 ids=match hash=match [OK]
policy_bundles       source=   3 target=   3 migrated=   3 ids=match hash=match [OK]

Migration completed successfully.
```

### Content-Hash Evidence (First Pass)

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

## Resume / Idempotency Verification

Running the same migration with `--resume` on the already-populated target produced:

- All tables skipped via checkpoint (row counts matched).
- No re-insertion occurred (`migrated_count=0` for all tables).
- Count match: `true` for all tables.
- Hash match: `true` for all tables.

Resume verdict: **PASS**.

## Target Row Count Verification (psql)

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

All counts match the source fixture (±0).

## Safety Check — Non-Empty Target Rejection

Running the migration **without** `--resume` against a non-empty target correctly fails with:

```text
target table '<table>' is not empty; P4.4 MVP requires an empty target
```

This validates the empty-target guard intended to prevent accidental overwrites.

## Overall Verdict

```text
D.1 populated local migration rehearsal: PASS
SQLite→PostgreSQL migration (apply): PASS
Content-hash validation (source == target): PASS
Resume/idempotency check: PASS
Non-empty target safety guard: PASS
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Cleanup

| Cleanup Item | Result |
|---|---|
| Temp fixture retained for local evidence | `/tmp/ferrumgate-d1-rehearsal/populated_fixture.db` |
| PostgreSQL container | left running for further readiness work; not production |

## Remaining Work

This rehearsal strengthens local engineering evidence for P5e migration tooling, but it does not close operator-owned target-host blockers.

Remaining evidence needed:
- Operator path selection reaffirmation (SQLite remains selected per doc113)
- D.2–D.6 continued cadence per `117-postgresql-readiness-acceleration-plan.md`
- Target-host P5c.V1/V2 drills if operator ever revisits Option B
- G3.6 real workload validation
- P5b pool tuning against real workload

---

*Artifact created: 2026-05-12. D.1 Migration Rehearsal Evidence — local-only. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
