# PostgreSQL Container Restart Recovery Drill Evidence — 2026-05-21

## Status

- **Scope**: PG-2.3b B.2 — local Docker-based PostgreSQL container restart recovery drill.
- **Verdict**: ✅ PASS — full drill 9/9 checks passed, recovery 14s <= 30s.
- **Production-ready**: NO.
- **Full G2**: NOT COMPLETE.
- **Block A**: WAIVED/CONDITIONAL, not closed.
- **HA/PostgreSQL production**: NOT CLAIMED.

This artifact records a local Docker-based drill because no real/staging PostgreSQL DSN is stored in the repository. The DSN used is the documented placeholder from `docker-compose.postgres.yml`.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-21 |
| Host scope | Local development workstation |
| PostgreSQL mode | Docker Compose local staging fallback |
| Docker service | `postgres_p2` |
| PostgreSQL image | `postgres:16` |
| PostgreSQL container | `ferrumgate_postgres_p2` |
| Sanitized PG DSN | `postgres://ferrumgate_dev:[REDACTED]@localhost:5432/ferrumgate_p2_test` |
| ferrumd bind address | `127.0.0.1:19084` |
| Drill script | `scripts/run_pg_container_restart_drill.sh` |

## Execution

Command:

```bash
bash scripts/run_pg_container_restart_drill.sh
```

The drill performs the following steps automatically:

1. Preflight checks (docker, docker compose, cargo, curl).
2. Starts the PostgreSQL container from `docker-compose.postgres.yml`.
3. Builds `ferrumd` with the `postgres` feature.
4. Starts `ferrumd` with the local placeholder PostgreSQL DSN.
5. Polls `/v1/readyz/deep` until healthy (initial readiness).
6. Restarts the PostgreSQL container.
7. Measures the time until `/v1/readyz/deep` returns healthy again.
8. Cleans up the `ferrumd` process and the PostgreSQL container.

## Results

### Preflight

| Check | Result |
|-------|--------|
| docker available | ✅ PASS |
| docker compose available | ✅ PASS |
| cargo available | ✅ PASS |
| curl available | ✅ PASS |

### Build

| Check | Result |
|-------|--------|
| ferrumd built with `--features postgres` | ✅ PASS (1m 46s) |

### Initial readiness

| Check | Result |
|-------|--------|
| PostgreSQL container healthy | ✅ PASS |
| ferrumd ready before restart | ✅ PASS |

### Restart and recovery

| Check | Result |
|-------|--------|
| PostgreSQL container healthy after restart | ✅ PASS |
| ferrumd recovered after restart | ✅ PASS |
| Recovery time | **14s** (target <= 30s) |

### Deep readiness output (post-recovery)

```json
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true}]}
```

## Known gaps and limits

- This is a **local Docker staging fallback**, not a production PostgreSQL target.
- The drill uses an empty database; production data volume and connection-pool saturation are not tested.
- Recovery time depends on Docker healthcheck interval (10s), PostgreSQL startup time, and `sqlx::PgPool` reconnection behavior.
- The drill is **manual/optional** and is **not executed in CI**.
- Target-host PostgreSQL, managed PostgreSQL, TLS/SSL DSN enforcement, HA, and failover remain pending.
- This evidence does not close Block A and does not complete full G2.

## Non-claims

- `production-ready = NO`.
- `full G2 = NOT COMPLETE`.
- `Block A = WAIVED/CONDITIONAL`.
- `PostgreSQL production deployment = NO`.
- `HA/multi-node = NO`.
- `CI automated test = NO`.

## Verdict

PG-2.3b B.2 local Docker container restart drill: ✅ PASS (full drill 9/9 passed, recovery 14s <= 30s).

This is sufficient to mark the B.2 drill script prepared and locally validated, but it is not sufficient to claim PostgreSQL production readiness or HA.
