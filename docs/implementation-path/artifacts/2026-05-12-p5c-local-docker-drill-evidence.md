# 2026-05-12 — P5c Local Docker PostgreSQL Backup/Restore Drill Evidence

> **Status**: LOCAL-ONLY SMOKE DRILL PASSED.
> This evidence does **not** claim production readiness, PostgreSQL production deployment, HA, multi-node readiness, or target-host operator completion.

## Scope

This artifact records a local Docker PostgreSQL P5c.V1/P5c.V2 smoke drill performed after `docs/implementation-path/111-p5c-local-docker-drill-plan.md` was committed.

- Drill type: local-only Docker PostgreSQL smoke drill
- Evidence target: backup/restore mechanics for PostgreSQL schema
- Data scope: empty FerrumGate schema; row counts are expected to be `0`
- Production claim: **NO**
- Target-host operator blocker closure: **NO**
- PostgreSQL production deployment claim: **NO**
- HA/multi-node claim: **NO**

## Environment

| Field | Value |
|---|---|
| Date | 2026-05-12 |
| Repo | `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify` |
| Base commit before drill plan | `c0cf51c` |
| Drill plan commit | `b9cd756 docs: add local Docker P5c drill plan` |
| Docker | Available (`Docker version 29.3.1`) |
| Docker Compose | Available (`Docker Compose version v5.1.1`) |
| Host PostgreSQL clients | Not available (`pg_dump`, `pg_restore`, `psql` not found on host) |
| PostgreSQL client strategy | Use PostgreSQL client tools inside the Docker container |

## Deviations from Plan 111

| Deviation | Reason | Outcome |
|---|---|---|
| `docker compose -f docker-compose.postgres.yml up -d postgres_p2` could not bind host port `5432` | Existing unrelated container was already bound to host `5432` | Did **not** stop unrelated container; used isolated local-only PostgreSQL container on host port `55432` |
| Host `pg_dump` / `pg_restore` / `psql` unavailable | PostgreSQL client packages are not installed on host | Used client tools inside `postgres:16` container |
| Schema setup used empty SQLite source | No local `.db` fixture was present in repo | PostgreSQL schema was initialized; migration command then failed when reading missing source table from `sqlite::memory:`. This setup caveat did not affect backup/restore drill; schema tables were verified before backup. |

Sanitized connection description:

```text
postgres://ferrumgate_dev@localhost:55432/ferrumgate_p2_test
```

No password, token, full DSN, or production credential is recorded in this artifact.

## Readiness Checks

| Check | Result |
|---|---|
| Docker CLI available | PASS |
| Docker Compose available | PASS |
| Host PostgreSQL clients available | FAIL — not installed |
| PostgreSQL clients available in container | PASS |
| Local PostgreSQL accepting connections | PASS: `/var/run/postgresql:5432 - accepting connections` |
| Schema tables present before backup | PASS: 11 tables listed |

Tables verified before backup:

```text
_migration_checkpoints
approvals
capabilities
executions
intents
ledger_entries
policy_bundles
proposals
provenance_edges
provenance_events
rollback_contracts
```

## P5c.V1 — Backup Drill Evidence

| Evidence Field | Result |
|---|---|
| Backup command | `pg_dump` inside local Docker PostgreSQL container |
| Format | Custom archive (`-Fc`) |
| Exit status | PASS |
| Backup artifact inside container | `/tmp/ferrumgate_p5c_local_drill.dump` |
| Backup artifact copied to host temp path | `/tmp/ferrumgate-pg-drill/ferrumgate_p5c_local_drill.dump` |
| Size | `20K` |
| SHA-256 | `8c30893a392f71f138f6dd0dcccfbf414318c4d2b1342cbd3d6450cb9b811f0c` |
| `pg_restore -l` | PASS; archive lists 57 TOC entries |

Selected `pg_restore -l` evidence:

```text
Archive created at 2026-05-12 08:23:14 UTC
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

P5c.V1 local smoke verdict: **PASS**.

## P5c.V2 — Restore Drill Evidence

| Evidence Field | Result |
|---|---|
| Drill database | `ferrumgate_drill_20260512_0823` |
| Create drill database | PASS |
| Restore command | `pg_restore` inside local Docker PostgreSQL container |
| Restore exit status | PASS |
| Table listing after restore | PASS; 11 tables listed |
| `intents` row count | `0` |
| `proposals` row count | `0` |
| `executions` row count | `0` |
| Drill database cleanup | PASS; drill database dropped |

The row counts are expected to be `0` because this was a schema-only local smoke drill, not a populated workload restore drill.

P5c.V2 local smoke verdict: **PASS**.

## Cleanup

| Cleanup Item | Result |
|---|---|
| Drill database dropped | PASS |
| Drill container stopped | PASS |
| Drill container removed | PASS |
| Backup artifact retained for local evidence | `/tmp/ferrumgate-pg-drill/ferrumgate_p5c_local_drill.dump` |

## Overall Verdict

```text
P5c local Docker smoke drill: PASS
P5c.V1 local backup mechanics: PASS
P5c.V2 local restore mechanics: PASS
Production-ready: NO
PostgreSQL production deployment: NO
HA/multi-node: NO
Target-host operator blocker closure: NO
```

## Remaining Work

This local smoke drill does not close the operator-owned target-host blockers in `66-path-2-operator-handoff.md`.

Remaining evidence needed before those blockers can be checked off:

- P5c.V1 backup drill on the operator-approved PostgreSQL target, if PostgreSQL path remains active
- P5c.V2 restore drill on the operator-approved PostgreSQL target, if PostgreSQL path remains active
- Populated-data restore drill or real workload dataset restore, if required by operator acceptance
- G3.6 real workload / post-deploy monitoring
- Target-host D1–D6 evidence
- SQLite restore drill, backup automation, TLS/reverse proxy, and bearer token setup for Path 2 pilot
