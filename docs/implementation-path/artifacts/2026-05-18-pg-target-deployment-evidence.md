# PostgreSQL Target/Staging Baseline Evidence — 2026-05-18

## Status

- **Scope**: PG-1 local Docker staging fallback baseline.
- **Verdict**: ✅ PASS for local Docker PostgreSQL baseline.
- **Production-ready**: NO.
- **Full G2**: NOT COMPLETE.
- **Block A**: WAIVED/CONDITIONAL, not closed.
- **HA/PostgreSQL production**: NOT CLAIMED.

This artifact records a local Docker PostgreSQL baseline run because no real/staging PostgreSQL DSN is stored in the repository. The DSN below is sanitized; no credential-bearing production DSN is recorded.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-18 |
| Host scope | Local development workstation |
| PostgreSQL mode | Docker Compose local staging fallback |
| Docker service | `postgres_p2` |
| PostgreSQL image | `postgres:16` |
| PostgreSQL container | `ferrumgate_postgres_p2` |
| Sanitized PG DSN | `postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test` |
| Source SQLite snapshot | `/tmp/opencode/ferrumgate-pg1-source.db` |
| ferrumd bind address | `127.0.0.1:19083` |

## PG-1.1 — PostgreSQL target/staging provisioned

Command:

```bash
docker compose -f docker-compose.postgres.yml up -d postgres_p2
docker compose -f docker-compose.postgres.yml ps
```

Observed result:

```text
ferrumgate_postgres_p2   postgres:16   ...   Up ... (healthy)   0.0.0.0:5432->5432/tcp
```

Result: ✅ PASS.

## PG-1.2 — ferrumd starts with PostgreSQL DSN

Command, sanitized:

```bash
FERRUMD_STORE_DSN="postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test" \
FERRUMD_BIND_ADDR="127.0.0.1:19083" \
cargo run --features postgres --package ferrumd
```

Observed result, sanitized:

```text
Finished `dev` profile [unoptimized + debuginfo]
Running `target/debug/ferrumd`
starting ferrumd with config: auth_mode=disabled, bind_addr=127.0.0.1:19083, store_dsn=postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test, log_format=text, rate_limit_per_second=2, rate_limit_burst=50
ferrumd listening on 127.0.0.1:19083
```

Result: ✅ PASS.

## PG-1.3 — `/v1/readyz/deep` returns healthy

Command:

```bash
curl -sf http://127.0.0.1:19083/v1/readyz/deep
```

Observed result:

```json
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true}]}
```

Post-migration recheck returned the same healthy result.

Result: ✅ PASS.

## PG-1.4 — `ferrum-migrate` completes

### Source SQLite snapshot preparation

The source SQLite snapshot was initialized by starting `ferrumd` with:

```bash
FERRUMD_STORE_DSN="sqlite:///tmp/opencode/ferrumgate-pg1-source.db?mode=rwc" \
FERRUMD_BIND_ADDR="127.0.0.1:19082" \
cargo run --package ferrumd
```

The source readiness check passed:

```json
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true}]}
```

Note: an initial attempt without `?mode=rwc` failed with SQLite `unable to open database file`; the corrected source DSN above was used for the successful snapshot.

### Build commands

```bash
cargo build --features postgres --package ferrumd
cargo build --features postgres --package ferrum-migrate
```

Observed results:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 17s
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 47s
```

### Dry-run migration

Command, sanitized:

```bash
cargo run --features postgres --package ferrum-migrate -- \
  --from sqlite:///tmp/opencode/ferrumgate-pg1-source.db \
  --to "postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test" \
  --json
```

Observed result summary:

```text
dry_run=true
applied=false
overall_success=true
tables=10
all count_match=true
all hash_match=true
all errors=[]
```

### Apply migration

Command, sanitized:

```bash
cargo run --features postgres --package ferrum-migrate -- \
  --from sqlite:///tmp/opencode/ferrumgate-pg1-source.db \
  --to "postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test" \
  --apply --json
```

Observed result summary:

```text
dry_run=false
applied=true
overall_success=true
tables=10
all id_match=true
all count_match=true
all hash_match=true
all errors=[]
```

Result: ✅ PASS.

## PG-1.5 — Row counts match

The source snapshot was intentionally empty except for schema initialization, so all migrated table counts are zero. This still verifies schema access, table enumeration, count comparison, ID comparison, and migration success against PostgreSQL.

| Table | Source count | Target count | Migrated count | Count match | ID match |
|-------|-------------:|-------------:|---------------:|-------------|----------|
| `intents` | 0 | 0 | 0 | true | true |
| `proposals` | 0 | 0 | 0 | true | true |
| `capabilities` | 0 | 0 | 0 | true | true |
| `executions` | 0 | 0 | 0 | true | true |
| `rollback_contracts` | 0 | 0 | 0 | true | true |
| `approvals` | 0 | 0 | 0 | true | true |
| `provenance_events` | 0 | 0 | 0 | true | true |
| `provenance_edges` | 0 | 0 | 0 | true | true |
| `ledger_entries` | 0 | 0 | 0 | true | true |
| `policy_bundles` | 0 | 0 | 0 | true | true |

Result: ✅ PASS.

## PG-1.6 — Content hash validation passes

The apply run reported matching hashes for all ten tables. Because the source was empty, the expected SHA-256 empty-content hash was used consistently.

| Table group | Source hash | Target hash | Hash match |
|-------------|-------------|-------------|------------|
| All 10 migrated tables | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | true |

Result: ✅ PASS.

## PG-1.7 — Evidence artifact created

This file was created from the PG-1 evidence template and filled with sanitized outputs.

Result: ✅ PASS.

## PG-1.8 — Docs/checklist update

Tracking updates are recorded in:

- `docs/production-readiness-v2/02-postgres-production-plan.md`
- `docs/production-readiness-v2/10-evidence-checklist.md`
- `docs/PRODUCTION_NOTES.md`

Result: ✅ PASS after companion doc updates in the same change batch.

## Known gaps and limits

- This is a **local Docker staging fallback**, not a production PostgreSQL target.
- The migration source snapshot was empty; this validates schema/readiness/migration plumbing, not production data volume.
- Target-host PostgreSQL, managed PostgreSQL, TLS/SSL DSN enforcement, backup/restore drills, connection hardening, PG metrics, HA, and failover remain pending.
- This evidence does not close Block A and does not complete full G2.

## Non-claims

- `production-ready = NO`.
- `full G2 = NOT COMPLETE`.
- `Block A = WAIVED/CONDITIONAL`.
- `PostgreSQL production deployment = NO`.
- `HA/multi-node = NO`.

## Verdict

PG-1 local Docker PostgreSQL baseline: ✅ PASS.

This is sufficient to mark PG-1 local/staging fallback evidence complete, but it is not sufficient to claim PostgreSQL production readiness or HA.
