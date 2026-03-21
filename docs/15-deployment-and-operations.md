# 15 — Deployment and operations

## Current runtime shape (as of today)

Ferrumd runs as a **single process** with the following hardcoded behaviors:

| Item | Value | Notes |
|------|-------|-------|
| Database | `sqlite::memory:?cache=shared` | In-memory; state resets on every restart |
| Bind address | `127.0.0.1:8080` | Fixed; not configurable via env/cli today |
| Rollback adapters | fs, git, sqlite, maildraft, http, noop | Registered at startup |
| Capability service | In-memory `InMemoryCapabilityService` | No persistence across restarts |

**Suitable for local development and smoke-testing only.**  
Do not deploy this configuration to staging or production — there is no externalized configuration, no persistence across restarts, and no built-in HA.

## Development

- single process
- sqlite local
- memory ledger chấp nhận được

## Staging / production-like

- persistent store
- provenance bật
- rollback bật
- strict manifest pinning nên bật
- logs không lộ secrets

## Operations checklist

- policy bundle đúng environment
- rollback không bị tắt
- sanitize/DLP bật
- TTL hợp lý
- lineage query usable

## Open gaps (operator-facing)

- **No documented externalized config knobs**: bind address, database URL, adapter paths, and feature flags are currently hardcoded in `bins/ferrumd/src/main.rs`. There is no env-variable or CLI-based runtime configuration surface yet.
- **HTTP remote mutation recovery not automated**: HTTP rollback/compensation on remote mutation is a **no-op by design** today. The system stays conservative; operators must manually compensate if an HTTP adapter mutates state and the operation needs rollback.
- **State does not survive restarts**: because the SQLite store uses `sqlite::memory:?cache=shared`, all state (capability leases, action history, rollback contracts) is lost on process restart. There is no persistent volume by default.
- **No built-in auth layer on the HTTP control plane**: the gateway binds to `127.0.0.1:8080` without a documented TLS or auth handshake at the server level. Network exposure must be controlled at the infrastructure layer.
