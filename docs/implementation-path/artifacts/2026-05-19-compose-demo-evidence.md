# Docker Compose Demo Evidence — 2026-05-19

## Status

- **Scope**: DEP-1 / DEP-2 local Docker Compose demo validation.
- **Verdict**: ✅ PASS for local demo.
- **Production-ready**: NO.
- **Full hosted deployment story**: NOT COMPLETE (Postgres, systemd, Helm remain open).
- **Target-host / cloud**: NOT CLAIMED.

This artifact records a local Docker Compose demo run of ferrumd using the in-memory SQLite store, disabled authentication, and loopback-only host port binding.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Compose file | `docker-compose.demo.yml` |
| Dockerfile | `Dockerfile` (multi-stage, local demo only) |
| Image | `ferrum-gate-verify-ferrumd:latest` (built locally) |
| Container | `ferrumgate_demo` |
| Store DSN | `sqlite::memory:` |
| Auth mode | `disabled` |
| Container bind | `0.0.0.0:8080` |
| Host bind | `127.0.0.1:8080` |

## DEP-1 — Docker Compose demo starts ferrumd

### Build

Command:

```bash
docker compose -f docker-compose.demo.yml build
```

Observed result:

- Build completed successfully on first attempt.
- Image tagged `ferrum-gate-verify-ferrumd:latest`.

Note: A subsequent `up -d --build` encountered a transient BuildKit frontend gRPC error. Re-running `up -d` (without `--build`) used the already-built image and succeeded. This is recorded transparently; the image is valid.

### Start

Command:

```bash
docker compose -f docker-compose.demo.yml up -d
```

Observed result:

```text
[+] Running 1/1
 ✔ Container ferrumgate_demo  Started
```

Command:

```bash
docker compose -f docker-compose.demo.yml ps
```

Observed result:

```text
NAME             IMAGE                           COMMAND                  SERVICE   CREATED          STATUS                    PORTS
ferrumgate_demo  ferrum-gate-verify-ferrumd:latest   "ferrumd"                ferrumd   ...            Up ... (healthy)   127.0.0.1:8080->8080/tcp
```

Result: ✅ PASS.

## DEP-2 — Healthz passes after compose up

Command:

```bash
curl -s -o /tmp/opencode/ferrum-demo-healthz.txt -w "%{http_code}\n" http://127.0.0.1:8080/v1/healthz
```

Observed result:

- HTTP status: `200`
- Response body: `{"status":"ok"}`

Result: ✅ PASS.

## Runtime logs (sanitized)

```text
ferrumd listening on 0.0.0.0:8080
auth_mode=disabled
store_dsn=sqlite::memory:
```

## Cleanup

After capturing evidence, the local demo container was stopped and removed:

```bash
docker compose -f docker-compose.demo.yml down
```

Observed result: `ferrumgate_demo` stopped/removed and the compose network was
removed. No persistent volume was used.

## Non-claims

- **NOT production-ready**: This is a local demo with auth disabled and in-memory storage.
- **NOT validated for Postgres**: PostgreSQL compose demo (DEP-3) remains open.
- **NOT validated for systemd / Helm / K8s**: DEP-4 through DEP-6 remain open.
- **NOT exposed beyond loopback**: Host port is bound to `127.0.0.1:8080` only.
