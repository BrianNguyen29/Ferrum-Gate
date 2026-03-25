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
- `cargo run -p ferrumctl -- server inspect-provenance --limit <n>` limits results per page (server default is 100, max 1000).
- `cargo run -p ferrumctl -- server inspect-provenance --cursor <token>` resumes from a previous page's `next_cursor`.
- `cargo run -p ferrumctl -- server inspect-provenance --all-pages` exports all events across all pages as JSONL (newline-delimited JSON), one event per line. Follows cursors automatically until exhaustion.
- `cargo run -p ferrumctl -- server inspect-event <event_id> [--ancestry] [--descendants]` inspects a single provenance event, optionally including its ancestor chain and/or descendant chain.
- `cargo run -p ferrumctl -- server inspect-lineage <id> --format dot --output <path>` exports the event lineage as a Graphviz DOT file for visualization.
- `POST /v1/provenance/events/external` is the first P3 runtime boundary: it records a vendor-neutral externally observed event against an existing execution lineage and fails closed if the execution or parent event is missing/mismatched. Available as `ferrumctl server ingest-external-event`.

## Operations checklist
- policy bundle đúng environment
- rollback không bị tắt
- sanitize/DLP bật
- TTL hợp lý
- lineage query usable

## Pending approvals

R3 (IrreversibleHighConsequence) executions require explicit approval before the capability is consumed. While a capability is awaiting approval it is NOT consumed — the execution remains in AwaitingApproval state.

**Discover pending approvals:**
```
GET /v1/approvals[?limit=N][&cursor=CURSOR][&proposal_id=UUID][&execution_id=UUID]
```
Returns a response envelope:

- `items`: pending approvals, most recent first
- `next_cursor`: cursor for the next page, or `null` when there is no next page

Cursor pagination is stable for operator workflows while the pending set changes.
Ordering: created_at DESC, approval_id DESC (stable tiebreaker).

- `limit` defaults to 50, maximum 100
- `cursor` selects the next page
- `proposal_id` narrows the list to a single proposal
- `execution_id` narrows the list to approvals linked to a specific execution

Filter by proposal_id: when `proposal_id` is provided, returns only pending approvals for that specific proposal.

Filter by execution_id: when `execution_id` is provided, returns only pending approvals linked to this execution.

Combined filters: when both `proposal_id` and `execution_id` are provided, both filters apply (AND semantics).

**Act on a pending approval:**
```
POST /v1/approvals/{approval_id}/resolve
{"actor": {...}, "approve": true, "reason": "..."}
```
Granting (approve=true) consumes the capability and advances the execution to Prepared. Denying (approve=false) leaves the execution in AwaitingApproval and does NOT consume the capability.

Pending approvals expire after 15 minutes (expires_at). Expired approvals must be re-created by re-authorizing the execution.
