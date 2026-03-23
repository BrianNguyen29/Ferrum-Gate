# 15 — Deployment and operations

## Current runtime shape (as of today)

Ferrumd still runs as a **single process**, but it no longer depends on hardcoded bind/store values.

| Item | Current behavior | Notes |
|------|------------------|-------|
| Bind address | config-driven | CLI/env/config file; default loopback is `127.0.0.1:8080` |
| Store DSN | config-driven | repo dev config uses `sqlite://ferrumgate.dev.db`; fallback is in-memory SQLite |
| Rollback adapters | fs, git, sqlite, maildraft, http, noop | Registered at startup |
| Capability service | durable `SqliteCapabilityService` | Capabilities are persisted in SQLite; restart does not invalidate active leases |
| Control-plane auth | `disabled` or `bearer` | Health endpoints stay unauthenticated |
| TLS termination | external only | No in-process TLS listener today |

## Runtime config surface

`ferrumd` resolves config in this order:

1. CLI flags
2. env vars
3. config file
4. built-in defaults

Supported inputs today:

| Purpose | CLI | Env | Config |
|---------|-----|-----|--------|
| Config path | `--config` | `FERRUMD_CONFIG` | n/a |
| Bind address | `--bind` | `FERRUMD_BIND_ADDR` | `[server] host`, `port` |
| Store DSN | `--store-dsn` | `FERRUMD_STORE_DSN` | `[store] dsn` |
| Auth mode | `--auth-mode` | `FERRUMD_AUTH_MODE` | `[auth] mode` |
| Bearer token | `--bearer-token` | `FERRUMD_BEARER_TOKEN` | `[auth] bearer_token` |
| Insecure nonlocal bind override | `--allow-insecure-nonlocal` | `FERRUMD_ALLOW_INSECURE_NONLOCAL` | `[server] allow_insecure_nonlocal` |
| Log filter | `--log-filter` | `FERRUMD_LOG_FILTER` | top-level `log_filter` |

Repo-shipped examples:

- `configs/ferrumgate.dev.toml`
- `configs/ferrumgate.prod.toml`

## Development

- local/dev startup auto-loads `configs/ferrumgate.dev.toml` when run from the repo root
- dev config binds to loopback and persists state in `ferrumgate.dev.db`
- if no config file is present, ferrumd falls back to `sqlite::memory:?cache=shared`
- memory ledger remains acceptable for local/dev debugging

## Control-plane auth and network exposure

- `auth.mode = "disabled"` is intended for loopback-only local development.
- all non-health routes require `Authorization: Bearer <token>` when `auth.mode = "bearer"`.
- `/v1/healthz` and `/v1/readyz` remain unauthenticated for liveness/readiness checks.
- fail-closed startup guard: non-loopback bind with auth disabled is rejected unless `allow_insecure_nonlocal = true` is set explicitly.
- there is still **no in-process TLS**; if the control plane leaves loopback, terminate TLS and restrict network exposure at an external proxy/load balancer.

## Example commands

```sh
# Default repo-local dev startup
cargo run -p ferrumd

# Explicit dev config
cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml

# Bearer-authenticated non-loopback startup
FERRUMD_BEARER_TOKEN="replace-me" \
cargo run -p ferrumd -- \
  --config configs/ferrumgate.prod.toml \
  --bind 0.0.0.0:8080

# Print resolved config sources and startup-guard verdict, then exit
cargo run -p ferrumd -- --print-effective-config

# Validate startup guard only, without binding the listener
cargo run -p ferrumd -- --check-startup-guard
```

## Operator diagnostics

- `cargo run -p ferrumd -- --print-effective-config` prints the effective bind/store/auth/log settings, which source won (`cli`, `env`, `file`, `default`, or auto dev config), and whether the startup guard would pass.
- `cargo run -p ferrumd -- --check-startup-guard` is the fastest preflight when operators want to confirm non-loopback/auth settings before rollout.
- `cargo run -p ferrumctl -- server ready` hits `/v1/readyz` for a lightweight readiness check after startup.
- `cargo run -p ferrumctl -- server inspect-provenance --execution-id <id> --terminal-only` returns only terminal provenance events for a governed execution.
- `cargo run -p ferrumctl -- server inspect-lineage <id> --format dot --output <path>` exports the event lineage as a Graphviz DOT file for visualization.
- `POST /v1/provenance/events/external` is the first P3 runtime boundary: it records a vendor-neutral externally observed event against an existing execution lineage and fails closed if the execution or parent event is missing/mismatched. Available as `ferrumctl server ingest-external-event`.

## Operations checklist

- policy bundle matches the environment
- rollback remains enabled
- sanitize/DLP stays enabled in the selected policy/runtime path
- TTL stays conservative for the capability window
- lineage query remains usable (`ferrumctl` or API)
- bearer auth is enabled before binding non-loopback unless there is a deliberate local exception
- TLS terminates at a reverse proxy or other ingress layer for any non-loopback exposure

## Operator-facing gaps and notes

- **HTTP remote mutation recovery is not automated**: HTTP rollback/compensation on remote mutation is a **no-op by design** today. Operators must still compensate manually when remote HTTP state was mutated.
- **Capability persistence is now durable**: capabilities are stored in SQLite via `SqliteCapabilityService`. On startup, `ferrumd` reconciles legacy active capabilities with execution history. Gateway no longer dual-writes capabilities; the capability service handles durable persistence. If capability persistence fails at runtime, the gateway fails closed and does not silently continue.
- **No built-in TLS listener**: bearer auth exists at the app layer, but certificate lifecycle and TLS termination still belong to external infrastructure.
- **No HA / multi-node story yet**: the daemon is still a single-process control plane, not a replicated service.
