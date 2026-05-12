# 2026-05-12 — P5c Populated Local PostgreSQL Backup/Restore Drill Evidence

> **Status**: LOCAL-ONLY POPULATED-DATA DRILL PASSED.
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.

## Scope

This artifact records the engineering-owned populated-data local PostgreSQL drill from `112-post-p5c-completion-execution-plan.md` Track 2.

- Drill type: local-only Docker PostgreSQL populated-data drill
- Evidence target: SQLite→PostgreSQL migration, PostgreSQL backup, PostgreSQL restore, row-count preservation
- Production claim: **NO**
- Target-host operator blocker closure: **NO**
- PostgreSQL production deployment claim: **NO**
- HA/multi-node claim: **NO**

## Environment

| Field | Value |
|---|---|
| Date | 2026-05-12 |
| Repo | `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify` |
| Baseline evidence commit | `1e7adca docs: add P5c local Docker drill evidence` |
| Plan artifact | `112-post-p5c-completion-execution-plan.md` |
| PostgreSQL container | `ferrumgate_postgres_p5c_populated_drill` |
| Host port | `55432` |
| PostgreSQL image | `postgres:16` |
| PostgreSQL client strategy | Use PostgreSQL client tools inside the Docker container |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:55432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## Populated SQLite Fixture

The fixture was created under `/tmp/ferrumgate-pg-populated-drill/populated_fixture.db` using the local SQLite migrations and synthetic non-production rows.

| Table | Source Rows |
|---|---:|
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

## Migration Validation

The populated fixture exposed two real P5e validation defects before the final pass:

1. PostgreSQL `INT4`/`BOOL` canonical content-hash decoding was not handled correctly.
2. Nullable text columns were not preserved consistently during insert binding.

Both defects were fixed in `bins/ferrum-migrate/src/main.rs` before the final migration pass. The final populated migration result:

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

Migration verdict: **PASS**.

## P5c.V1 — Populated Backup Drill Evidence

| Evidence Field | Result |
|---|---|
| Backup command | `pg_dump` inside local Docker PostgreSQL container |
| Format | Custom archive (`-Fc`) |
| Exit status | PASS |
| Backup artifact inside container | `/tmp/ferrumgate_p5c_populated_drill.dump` |
| Backup artifact copied to host temp path | `/tmp/ferrumgate-pg-populated-drill/ferrumgate_p5c_populated_drill.dump` |
| Size | `24K` |
| SHA-256 | `59f09e7f3bb09868d9a4944002754207df697c9080ccf65140703ecc14ab7afd` |
| `pg_restore -l` | PASS; archive lists 57 TOC entries |

Selected `pg_restore -l` evidence:

```text
Archive created at 2026-05-12 09:25:24 UTC
dbname: ferrumgate_p2_test
TOC Entries: 57
Format: CUSTOM
Dumped from database version: 16.13
Dumped by pg_dump version: 16.13
TABLE public intents
TABLE public proposals
TABLE public executions
TABLE public provenance_events
TABLE public rollback_contracts
```

P5c.V1 populated local verdict: **PASS**.

## P5c.V2 — Populated Restore Drill Evidence

| Evidence Field | Result |
|---|---|
| Drill database | `ferrumgate_populated_drill_20260512_0925` |
| Create drill database | PASS |
| Restore command | `pg_restore` inside local Docker PostgreSQL container |
| Restore exit status | PASS |
| Table listing after restore | PASS; 11 tables listed |
| `intents` row count | `50` |
| `proposals` row count | `30` |
| `executions` row count | `20` |
| `capabilities` row count | `30` |
| `provenance_events` row count | `60` |
| `ledger_entries` row count | `60` |
| Drill database cleanup | PASS; drill database dropped |

The restored row counts match the populated fixture and migrated PostgreSQL target for the key tables checked.

P5c.V2 populated local verdict: **PASS**.

## Cleanup

| Cleanup Item | Result |
|---|---|
| Drill database dropped | PASS |
| Drill container stopped | PASS |
| Drill container removed | PASS |
| Backup artifact retained for local evidence | `/tmp/ferrumgate-pg-populated-drill/ferrumgate_p5c_populated_drill.dump` |

## Overall Verdict

```text
P5c populated local Docker drill: PASS
SQLite→PostgreSQL populated migration: PASS
P5c.V1 populated backup mechanics: PASS
P5c.V2 populated restore mechanics: PASS
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Remaining Work

This populated local drill strengthens local engineering evidence, but it does not close the operator-owned target-host blockers in `66-path-2-operator-handoff.md`.

Remaining evidence needed before those blockers can be checked off:

- Operator path selection: SQLite vs PostgreSQL
- P5c.V1 backup drill on the operator-approved PostgreSQL target, if PostgreSQL path remains active
- P5c.V2 restore drill on the operator-approved PostgreSQL target, if PostgreSQL path remains active
- G3.6 real workload / post-deploy monitoring
- Target-host D1–D6 evidence
- SQLite restore drill, backup automation, TLS/reverse proxy, and bearer token setup for Path 2 pilot
