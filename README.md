# FerrumGate

FerrumGate is an **intent-scoped reversible execution plane** for MCP/tool-using AI agents and agentic runtimes.

It sits between an agent and its tools to enforce that every side-effecting action is: scoped to a declared intent, capability-bounded, reversible, and auditable via a provenance lineage chain.

## Status

**Single-node v1 — pilot-ready / production-candidate.** See
[docs/implementation-path/23-production-readiness-assessment.md](docs/implementation-path/23-production-readiness-assessment.md)
for the full supported surface and known gaps.

**In scope for v1**: SQLite-backed single-node gateway flow with filesystem,
SQLite, MailDraft (draft-only), Git, and HTTP adapters; firewall enforcement;
capability mint/authorize/execute; R0/R2/R3 governance paths; rollback and
compensation; provenance lineage chain.

**Explicitly out of scope for v1**: multi-node sync, HA/multi-leader, in-process
TLS termination, distributed trace context, alerting rules, and generic provenance
replay fabric tooling.

## Architecture

```
Intent
  |
  v
[Intent Compiler] --> [Semantic Firewall: trust/taint/DLP]
  |                     |
  v                     v
[Policy PDP] ------> [Allow | Deny | Quarantine | RequireApproval | AllowDraftOnly]
  |
  v
[Capability Mint] --> narrow, short-TTL, single-use lease
  |
  v
[Gateway] --> route to adapter
  |
  v
[Adapters: fs | sqlite | git | http | maildraft] --> [Rollback Contract]
  |                                                      |
  v                                                      v
[Verify] <--------------------------------------------[Commit | Rollback]
  |
  v
[Provenance Ledger] --> lineage chain persisted to SQLite
```

The gateway runs as `ferrumd`. Operators interact via `ferrumctl`.

## Features

- **Intent-scoped execution** — every mutation requires an explicit intent manifest
- **Capability leasing** — narrow, short-lived, single-use capability leases; no session-wide permissions
- **Semantic firewall** — trust labeling, taint scoring, contradiction checks, DLP redaction at evaluate time
- **Execution-time enforcement** — policy checks enforced at adapter execution time for File, Http, Sqlite, Git, EmailDraft
- **Adapter-backed rollback** — filesystem, SQLite, Git, and MailDraft adapters produce and execute rollback contracts; HTTP rollback is a no-op by design (manual compensation required)
- **Provenance lineage** — every meaningful side effect emits a lineage event; execution graph queryable via API and CLI
- **Governance paths** — R0 (auto-commit), R2 (explicit commit), R3 (require approval before execution)
- **SQLite-backed persistence** — all core domain objects (intents, proposals, capabilities, executions, rollback contracts, provenance edges) are durable across restarts

## Repository contents

This repository includes:

- a Rust workspace for the control plane, adapters, storage, and CLI
- architecture and implementation docs for contributors and operators
- machine-readable contracts, OpenAPI, and JSON schemas
- configs, scripts, and integration tests for local development and validation
- roadmap and implementation-path documents for post-v1 work

## Quickstart

```sh
# Build the control plane and CLI
cargo build -p ferrumd -p ferrumctl

# Run with the repo dev config (auto-loaded when present)
cargo run -p ferrumd

# Or point to a specific config explicitly
cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml

# Preflight: print resolved effective config and exit
cargo run -p ferrumd -- --print-effective-config

# Preflight: run startup guard checks without starting the server
cargo run -p ferrumd -- --check-startup-guard

# Operator CLI — health and readiness
cargo run -p ferrumctl -- server health
cargo run -p ferrumctl -- server ready

# Inspect an execution and its lineage
cargo run -p ferrumctl -- server inspect-execution <execution_id>
cargo run -p ferrumctl -- server inspect-lineage <execution_id>
cargo run -p ferrumctl -- server inspect-lineage <execution_id> --format json
cargo run -p ferrumctl -- server inspect-lineage <execution_id> --format dot --output lineage.dot

# Query provenance events
cargo run -p ferrumctl -- server inspect-provenance --limit 100
cargo run -p ferrumctl -- server inspect-event <event_id> --ancestry --descendants

# Resolve a pending approval (R3 governance path)
cargo run -p ferrumctl -- server resolve-approval <approval_id> \
  --approve --actor-id <operator_id> --actor-type Operator \
  --reason "approved after review"
```

Config precedence: CLI flags > environment variables > config file > defaults.
`ferrumd` auto-loads `configs/ferrumgate.dev.toml` from the repo root when present.
Without a config file it falls back to `sqlite::memory:?cache=shared`.

Auth: loopback binds (`127.0.0.1`) with `auth.mode = "disabled"` are accepted for local dev.
For non-loopback binds, either enable `auth.mode = "bearer"` (token from config or
`FERRUMD_BEARER_TOKEN` env var) or set `allow_insecure_nonlocal = true` explicitly.
TLS is terminated by an external reverse proxy or ingress layer; in-process TLS
is not implemented.

## Supported Adapters

| Adapter    | Execute | Verify | Rollback / Compensation | Notes                              |
|------------|---------|--------|-------------------------|-------------------------------------|
| Filesystem | Yes     | Yes    | Yes                     | Full parity                         |
| SQLite     | Yes     | Yes    | Yes                     | Full parity; row restore via tx    |
| Git        | Yes     | Yes    | Yes                     | Full parity; local ref restore     |
| HTTP       | Yes     | Yes    | No-op                   | Rollback is no-op; manual comp.     |
| MailDraft  | Yes     | Yes    | Yes (draft-only)        | `allow_send=true` denied at gateway prepare-time |

## Core crates

- `ferrum-proto` — domain shapes, intent/policy/capability/provenance types
- `ferrum-pdp` — Policy Decision Point (Allow/Deny/Quarantine/RequireApproval/AllowDraftOnly)
- `ferrum-cap` — capability mint and single-use authorization
- `ferrum-rollback` — rollback contract creation and execution
- `ferrum-firewall` — trust labeling, taint scoring, DLP, contradiction checks
- `ferrum-store` — SQLite-backed persistence for all core domain objects
- `ferrum-graph` — provenance graph read-model helpers
- `ferrum-ledger` — hash-chain ledger with live append verification
- `ferrum-sync` — Sync-3a read-only transport probe for cross-node diagnostics (read-only; write-path not implemented)
- `ferrum-gateway` — orchestration server (`ferrumd`); compile/evaluate/mint/authorize/prepare/execute/verify/commit-rollback pipeline
- `ferrumctl` — operator CLI
- `ferrum-integrations-mcp` — MCP runtime integration layer

Adapter crates: `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite`,
`ferrum-adapter-http`, `ferrum-adapter-maildraft`.

## Documentation

- [docs/00-project-canon.md](docs/00-project-canon.md) — project canon, product thesis, four hard invariants
- [docs/02-project-overview.md](docs/02-project-overview.md) — product goals, target users, how it differs from a normal gateway
- [docs/03-architecture.md](docs/03-architecture.md) — component and layer architecture
- [docs/01-quickstart.md](docs/01-quickstart.md) — full CLI reference and control-plane API lifecycle
- [docs/implementation-path/23-production-readiness-assessment.md](docs/implementation-path/23-production-readiness-assessment.md) — v1 scope freeze, supported surface, known gaps, phased hardening plan
- [docs/16-release-checklist.md](docs/16-release-checklist.md) — v1 RC release gates and evidence
- [docs/91-phase-success-criteria-and-kpis.md](docs/91-phase-success-criteria-and-kpis.md) — success criteria per phase and KPIs

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

See [SECURITY.md](SECURITY.md).

## License

Apache-2.0
