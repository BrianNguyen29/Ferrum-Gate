# Docker Compose PostgreSQL Demo Evidence — 2026-05-19

## Status

- **Scope**: DEP-3 local Docker Compose PostgreSQL demo validation.
- **Verdict**: ✅ PASS for local demo.
- **Production-ready**: NO.
- **Full hosted deployment story**: NOT COMPLETE (systemd, Helm, backup remain open).
- **Target-host / cloud**: NOT CLAIMED.

This artifact records a local Docker Compose demo run of ferrumd backed by a PostgreSQL container, using placeholder credentials and disabled authentication.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Compose file | `docker-compose.postgres-demo.yml` |
| Dockerfile | `Dockerfile` (multi-stage, local demo only; `rust:1.95-bookworm` builder) |
| Image | `ferrum-gate-verify-ferrumd:latest` (built locally) |
| ferrumd container | `ferrumgate_demo_pg` |
| PostgreSQL container | `ferrumgate_postgres_demo` |
| Store DSN | `postgres://ferrumgate_dev:ferrumgate_dev_password@postgres:5432/ferrumgate_demo` (placeholder) |
| Auth mode | `disabled` |
| Container bind | `0.0.0.0:8080` |
| Host bind | `127.0.0.1:19081` |

## Build notes

### BuildKit transient error

An initial `docker compose -f docker-compose.postgres-demo.yml up -d --build` failed with:

```
failed to solve: frontend grpc server closed unexpectedly
```

This is a transient BuildKit issue. A subsequent run using the already-built image succeeded.

### rust-toolchain.toml / builder image mismatch (documented, now fixed)

When `DOCKER_BUILDKIT=0 docker compose -f docker-compose.postgres-demo.yml build` was attempted with the earlier `rust:1.85-bookworm` builder, the build first exceeded 900s while rustup attempted to download the latest stable toolchain because `rust-toolchain.toml` specifies `channel = "stable"`.

An intermediate attempt to bypass `rust-toolchain.toml` exposed that the current resolved dependency graph requires newer Rust than 1.85.1 for several transitive packages (for example `home@0.5.12`, `icu_*`, and `tonic@0.14.6`).

**Fix applied**: The `Dockerfile` now uses the current stable `rust:1.95-bookworm` builder and leaves `rust-toolchain.toml` intact, matching the repository's `stable` toolchain intent for the local demo container build.

Validation command:

```bash
docker build -t ferrum-gate-verify-ferrumd --build-arg FEATURES=postgres .
```

Observed result:

- Build completed successfully.
- Image exported as `docker.io/library/ferrum-gate-verify-ferrumd:latest`.
- Build log included `Finished release profile` for `ferrumd` with the `postgres` feature path.

## DEP-3 — Postgres deployment mode starts and health checks pass

### Start

Command:

```bash
docker compose -f docker-compose.postgres-demo.yml up -d ferrumd
```

Observed result:

- `ferrumgate_demo_pg` started successfully.
- `ferrumgate_postgres_demo` was already running/healthy.

Command:

```bash
docker compose -f docker-compose.postgres-demo.yml ps
```

Observed result:

```text
NAME                      IMAGE                           COMMAND                  SERVICE   CREATED          STATUS                    PORTS
ferrumgate_demo_pg        ferrum-gate-verify-ferrumd:latest   "ferrumd"                ferrumd   ...            Up ... (healthy)   127.0.0.1:19081->8080/tcp
ferrumgate_postgres_demo  postgres:16                     "docker-entrypoint.s…"   postgres  ...            Up ... (healthy)   5432/tcp
```

### Healthz

Command:

```bash
curl -s http://127.0.0.1:19081/v1/healthz
```

Observed result:

- HTTP status: `200`
- Response body: `{"status":"ok"}`

### Readyz (deep)

Command:

```bash
curl -s http://127.0.0.1:19081/v1/readyz/deep
```

Observed result:

- HTTP status: `200`
- Response body:

```json
{
  "status": "ok",
  "healthy": true,
  "components": [
    { "component": "store", "status": "ok", "healthy": true },
    { "component": "write_queue", "status": "ok: depth=0, threshold=100", "healthy": true },
    { "component": "pool", "status": "ok: idle=1/total=2/max=10", "healthy": true }
  ]
}
```

### Metrics

Command:

```bash
curl -s http://127.0.0.1:19081/v1/metrics
```

Observed result:

- HTTP status: `200`
- Relevant metrics included:
  - `ferrumgate_store_health_up 1`
  - `ferrumgate_store_pg_pool_size 2`
  - `ferrumgate_store_pg_pool_idle 1`
  - `ferrumgate_store_pg_pool_max 10`
  - `ferrumgate_store_pg_acquire_timeouts_total 0`

### Runtime logs (sanitized)

```text
ferrumd startup with store_dsn=postgres://ferrumgate_dev:[REDACTED]@postgres:5432/ferrumgate_demo
_schema_version already exists — skipping initialization
ferrumd listening on 0.0.0.0:8080
```

Result: ✅ PASS.

## Non-claims

- **NOT production-ready**: This is a local demo with auth disabled and placeholder credentials.
- **NOT validated for systemd / Helm / K8s**: DEP-4 through DEP-6 remain open.
- **NOT exposed beyond loopback**: Host port is bound to `127.0.0.1:19081` only.
- **NOT HA/multi-node**: Single-container PostgreSQL with no replication or failover.
