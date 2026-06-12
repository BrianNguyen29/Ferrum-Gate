# FerrumGate

**Intent-scoped reversible execution for AI agents and tool-using systems.**

[Tiếng Việt](./README.vi.md) | [Docs](./docs/README.md) | [Quickstart](./docs/guides/quickstart.md) | [Operator Guide](./docs/guides/operator.md) | [Security Model](./docs/guides/security-model.md)

FerrumGate is a governance gateway that sits between an autonomous agent and the tools it wants to use. It turns "run this tool" into a controlled lifecycle: compile intent, evaluate policy, mint a scoped capability, prepare rollback, execute the side effect, verify the result, and record provenance that can be audited later.

The product idea is simple: **agents should not receive direct, ambient authority over production systems**. They should receive narrow, short-lived, explainable permission to perform a specific action, with rollback and lineage built in before the side effect happens.

## Why FerrumGate Exists

AI agents are becoming operators: they write files, change Git repositories, call APIs, mutate databases, and draft operational messages. Traditional API keys and broad tool permissions are a poor fit for that world. They answer "who can call this API?", but not:

- What was the agent trying to accomplish?
- Was this action inside the user's declared scope?
- Which policy decision allowed it?
- Was rollback prepared before execution?
- Did verification prove the result matched intent?
- Can an operator reconstruct the exact lineage after a crash or incident?

FerrumGate is built around those questions.

## What Makes It Different

| Capability | What FerrumGate adds |
|------------|----------------------|
| Intent-first execution | Side effects are tied to a declared user/agent intent, not just a raw tool call. |
| Single-use capabilities | Execution requires a short-lived, scoped lease bound to intent, proposal, tool, and resources. |
| Rollback before action | Prepare and recovery contracts are created before the adapter performs the side effect. |
| Provenance-first lineage | Policy, capability, prepare, execute, verify, and terminal states are recorded as an auditable chain. |
| Fail-closed lifecycle | Missing lineage, invalid state transitions, stale leases, unknown mutating tools, and incomplete recovery stop the flow. |
| Operator-ready controls | Health, deep readiness, metrics, backup/restore, lifecycle outbox review, policy bundles, and CLI workflows are included. |

## Core Values

- **Least authority**: no broad ambient agent power; every action is scoped and time-bound.
- **Reversibility by design**: rollback class and recovery path are part of the execution contract.
- **Traceability over trust**: every critical decision becomes provenance, not tribal knowledge.
- **Fail closed under ambiguity**: unknown, stale, incomplete, or unverified states are treated as unsafe.
- **Operational honesty**: FerrumGate is production-oriented, but the repository documents the exact validation level and remaining operator responsibilities.

## Execution Lifecycle

```text
Intent
  -> Proposal
  -> PolicyEvaluated
  -> CapabilityMinted
  -> SideEffectPrepared
  -> ToolCallPrepared
  -> ToolCallExecuted
  -> SideEffectVerified
  -> Terminal state
```

Terminal states include committed, compensated, rolled back, failed, or recovery-incomplete outcomes. The lifecycle is guarded by store-backed transitions, outbox reconciliation, fencing tokens, and lineage gates.

## Architecture

```text
Agent / Client
    |
    v
Ferrum Gateway
    |-- policy evaluation
    |-- capability minting
    |-- authorization
    |-- rollback prepare
    |-- adapter execution
    |-- verification
    |-- provenance and lineage
    |
    v
Store: SQLite or PostgreSQL
```

First-party crates include:

- `ferrum-gateway`: HTTP API, auth, routes, lifecycle orchestration, metrics.
- `ferrum-proto`: shared domain types and API models.
- `ferrum-pdp`: policy decision point and policy bundle evaluation.
- `ferrum-cap`: capability minting, single-use leases, TTL enforcement.
- `ferrum-rollback`: prepare, execute, verify, rollback, compensate contracts.
- `ferrum-store`: SQLite/PostgreSQL persistence, migrations, reconciliation, audit state.
- `ferrum-ledger` and `ferrum-graph`: tamper-evident and lineage-oriented building blocks.
- `ferrum-sync` and `ferrum-integrations-mcp`: MCP and runtime integration paths.

## Adapter Surface

FerrumGate includes bounded adapter slices for common agent side effects:

| Adapter | Scope |
|---------|-------|
| Filesystem | Write, delete, move, copy, append, chmod, directory create/delete with sandboxing and snapshots. |
| Git | Commit, branch create/delete, tag create/delete with repository-root allowlists. |
| HTTP | HTTP mutation with rustls-backed client, no redirects, bounded timeout, SSRF guard, replay recovery contract. |
| SQLite | File-backed SQLite mutation with database-root allowlist and verification gates. |
| Mail draft | Draft create/update/delete lifecycle with recipient and content binding. |

Unknown mutating tools fail closed unless they provide an explicit typed adapter/action binding.

## Quickstart

Prerequisites:

- Rust stable
- `cargo`
- `make`
- `curl`

Start a local development gateway:

```bash
FERRUMD_BIND_ADDR=127.0.0.1:18080 \
cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml
```

Check liveness:

```bash
curl http://127.0.0.1:18080/v1/healthz
```

Expected response:

```json
{"status":"ok"}
```

Run the full local flow:

- [FerrumGate in 10 Minutes](./docs/guides/quickstart.md)
- [API guide](./docs/guides/api.md)
- [MCP integration](./docs/guides/mcp-integration.md)
- [Demo flows](./docs/guides/demo-flows.md)

## Operating FerrumGate

For a production-like deployment, do not expose the development config. Use bearer auth, a real store, explicit adapter allowlists, deep readiness, metrics, and backup/restore drills.

Important entry points:

- [Operator Guide](./docs/guides/operator.md)
- [Runtime Configuration Notes](./docs/PRODUCTION_NOTES.md)
- [Hosted Deployment](./docs/guides/hosted-deployment.md)
- [Zero-Downtime Upgrade](./docs/guides/zero-downtime-upgrade.md)
- [Troubleshooting](./docs/guides/troubleshooting.md)
- [Monitoring Config](./configs/monitoring/README.md)
- [Helm Chart](./deploy/helm/ferrumgate/README.md)

Production-oriented defaults include:

- bearer-auth mode for exposed deployments
- deep readiness endpoint with store, queue, and pool checks
- Prometheus metrics
- release governance CI gate
- hardcoded secret scan
- dependency audit gate
- PostgreSQL live tests
- filesystem, Git, and SQLite adapter boundary controls

## Project Status

FerrumGate is an active engineering project with a working gateway, adapters, persistence, CLI tooling, tests, docs, and deployment scaffolding. It is suitable for local evaluation, integration design, security review, and controlled pilot work.

It is **not a blanket compliance certification or turnkey HA product**. Operators still own deployment topology, TLS termination, secret management, backup policy, alert routing, database HA, and production acceptance testing for their environment.

## CLI and Binaries

| Binary | Purpose |
|--------|---------|
| `ferrumd` | Gateway daemon. |
| `ferrumctl` | CLI for health, readiness, audit, policy, approvals, lifecycle outbox, backup/restore. |
| `ferrum-migrate` | SQLite-to-PostgreSQL migration support. |
| `ferrum-stress` | Machine-readable stress/smoke scenarios. |
| `ferrum-tui` | Terminal operator dashboard. |

## Validation

Common local gates:

```bash
make fmt
make check
make lint
make test
make docs
make validate
make audit
make secret-scan
```

CI runs layout validation, contract consistency checks, formatting, workspace check, clippy, tests, release governance, and PostgreSQL live tests.

## Repository Map

```text
bins/                 ferrumd, ferrumctl, ferrum-migrate, ferrum-stress, ferrum-tui
crates/               Rust workspace crates
configs/              dev/prod/example runtime configuration
contracts/            machine-readable agent and integrator contracts
docs/                 guides, architecture, security, operations, diagrams
openapi/              control API specification
schemas/              JSON Schemas for core contracts
deploy/helm/          Kubernetes chart
scripts/              validation, drills, backup, governance, smoke tests
site/                 static documentation site scaffold
```

## Documentation Path

If you are new to the project, read in this order:

1. [Concepts](./docs/guides/concepts.md)
2. [Quickstart](./docs/guides/quickstart.md)
3. [Adapter Reference](./docs/guides/adapter-reference.md)
4. [Security Model](./docs/guides/security-model.md)
5. [Operator Guide](./docs/guides/operator.md)
6. [Production Notes](./docs/PRODUCTION_NOTES.md)

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Keep changes scoped, preserve the intent/capability/provenance/rollback invariants, and update docs/tests when contracts or schemas change.

## License

See [LICENSE](./LICENSE).
