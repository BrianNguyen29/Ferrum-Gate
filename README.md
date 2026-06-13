# FerrumGate

> **Scoped, Auditable, Reversible.**

A governance gateway for AI agents and tool-using systems that replaces ambient authority with intent-scoped, single-use capabilities — so every action is policy-checked, rollback-prepared, and provenance-recorded.

[![ci](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml/badge.svg)](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](https://www.rust-lang.org/)

[Tiếng Việt](./README.vi.md) | [Docs](./docs/README.md) | [Quickstart](./docs/guides/quickstart.md) | [Operator Guide](./docs/guides/operator.md) | [Security Model](./docs/guides/security-model.md)

---

## Table of Contents

- [Why a Gateway?](#why-a-gateway)
- [Who Is This For?](#who-is-this-for)
- [What Makes It Different](#what-makes-it-different)
- [Trust Model](#trust-model)
- [Implementation Status](#implementation-status)
- [Comparison](#comparison)
- [Execution Lifecycle](#execution-lifecycle)
- [Architecture](#architecture)
- [Adapter Surface](#adapter-surface)
- [Quickstart](#quickstart)
- [Operating FerrumGate](#operating-ferrumgate)
- [Performance & Validation](#performance--validation)
- [Project Status](#project-status)
- [CLI and Binaries](#cli-and-binaries)
- [FAQ](#faq)
- [Roadmap](#roadmap)
- [Repository Map](#repository-map)
- [Documentation Path](#documentation-path)
- [Contributing](#contributing)
- [License](#license)

---

## Why a Gateway?

AI agents are becoming operators: they write files, change Git repositories, call APIs, mutate databases, and draft operational messages. Traditional API keys and broad tool permissions answer "who can call this API?", but they do not answer:

- What was the agent trying to accomplish?
- Was this action inside the user's declared scope?
- Which policy decision allowed it?
- Was rollback prepared before execution?
- Did verification prove the result matched intent?
- Can an operator reconstruct the exact lineage after a crash or incident?

A governance gateway sits between the agent and the tool, turning every side effect into a controlled lifecycle: compile intent, evaluate policy, mint a scoped capability, prepare rollback, execute, verify, and record provenance. **Agents should not receive direct, ambient authority over production systems.** They should receive narrow, short-lived, explainable permission to perform a specific action, with rollback and lineage built in before the side effect happens.

## Who Is This For?

| Audience | What FerrumGate Offers |
|----------|------------------------|
| **Platform teams** | A bounded control plane for agent tooling with policy bundles, auth modes, and audit scaffolding. |
| **Security engineers** | Intent-scoped capabilities, fail-closed lifecycle, provenance chains, and rollback contracts instead of ambient API keys. |
| **Operators** | Health/readiness probes, metrics, backup/restore drills, CLI workflows, and Helm deployment scaffolding. |
| **Integration designers** | Adapter contracts for filesystem, Git, HTTP, SQLite, and mail drafts with clear boundary controls. |

## What Makes It Different

| Capability | What FerrumGate Adds |
|------------|----------------------|
| Intent-first execution | Side effects are tied to a declared user/agent intent, not just a raw tool call. |
| Single-use capabilities | Execution requires a short-lived, scoped lease bound to intent, proposal, tool, and resources. |
| Rollback before action | Prepare and recovery contracts are created before the adapter performs the side effect. |
| Provenance-first lineage | Policy, capability, prepare, execute, verify, and terminal states are recorded as an auditable chain. |
| Fail-closed lifecycle | Missing lineage, invalid state transitions, stale leases, unknown mutating tools, and incomplete recovery stop the flow. |
| Operator-ready controls | Health, deep readiness, metrics, backup/restore, lifecycle outbox review, policy bundles, and CLI workflows are included. |

## Trust Model

| Guarantee | Responsibility |
|-----------|----------------|
| Intent-to-action binding and policy evaluation | **FerrumGate** — enforced before any adapter call. |
| Single-use capability minting with TTL enforcement | **FerrumGate** — max 300s, hardcoded in `ferrum-cap`. |
| Rollback prepare / verify / compensate contracts | **FerrumGate** — generated and validated before side effects. |
| Audit log append-only provenance chain | **FerrumGate** — evidence-oriented; not WORM/compliance certification. |
| TLS termination, secret management, network policy | **Operator** — outside the gateway boundary. |
| Database HA, backup policy, alert routing | **Operator** — SQLite is single-node; PostgreSQL runtime is supported, but production HA/multi-node topology is not managed by this repository. |
| Production acceptance testing for your environment | **Operator** — FerrumGate documents validation level and remaining operator responsibilities. |

## Implementation Status

| Tier | Definition | Current Items |
|------|------------|---------------|
| **Stable** | Core model implemented, CI-tested, and suitable for local evaluation and controlled pilot work. | Intent lifecycle, policy evaluation, capability minting, rollback prepare/verify/compensate, SQLite write queue, provenance chain, bearer/scoped/OIDC/agent auth. |
| **Implemented** | Feature-complete for standard use; local and CI-validated. | Filesystem, HTTP, Git, SQLite, mail draft adapters; `ferrumctl` CLI; `ferrum-stress` smoke tests; `ferrum-tui` dashboard; Prometheus metrics; rate limiting; Helm chart. |
| **Beta** | Functional but may require operator tuning or have known caveats. | PostgreSQL runtime — local and CI live-tested; production HA/multi-node topology is not managed by the repo. |
| **Experimental** | Skeleton or partial implementation; not ready for production use. | MCP Streamable HTTP / SSE transport and resumability. |
| **Not implemented** | Single-tenant by design; no roadmap commitment. | Multi-tenancy, managed service, email sending (maildraft manages drafts only), compliance certification. |

> **Honesty note**: FerrumGate is not a turnkey HA product or compliance certification. Operators still own deployment topology, TLS, secrets, backups, database HA, and production acceptance testing.

## Comparison

| Dimension | Raw API Keys | Policy-as-Code (static) | Audit Logging (post-hoc) | Raw MCP | FerrumGate |
|-----------|-------------|--------------------------|------------------------|---------|------------|
| Intent binding | No | Limited | No | No | Yes — every action tied to declared intent. |
| Single-use capability | No | No | No | No | Yes — short-lived, scoped lease per action. |
| Policy enforcement point | No | Config-time or admission | No | No | Yes — runtime policy evaluation before execution. |
| Rollback preparation | No | No | No | No | Yes — prepare/verify/compensate contracts. |
| Provenance lineage | No | No | Partial logs | No | Yes — full lifecycle chain: policy → capability → prepare → execute → verify → terminal. |
| Fail-closed unknown tools | No | No | No | No | Yes — unknown mutating tools are blocked unless explicitly bound. |
| Operator controls | No | Limited | No | No | Yes — health, readiness, metrics, CLI, backup/restore, Helm. |

## Execution Lifecycle

```text
Intent
  -> Proposal
  -> PolicyEvaluated
  -> CapabilityMinted
  -> ActionProposalSubmitted
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
- Additional first-party crates: `ferrum-firewall` and bounded adapters (`ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-http`, `ferrum-adapter-sqlite`, `ferrum-adapter-maildraft`).

## Adapter Surface

FerrumGate includes bounded adapter slices for common agent side effects:

| Adapter | Scope |
|---------|-------|
| Filesystem | Write, delete, move, copy, append, chmod, directory create/delete with sandboxing and snapshots. |
| Git | Commit, branch create/delete, tag create/delete with repository-root allowlists. |
| HTTP | HTTP mutation with rustls-backed client, no redirects, bounded timeout, SSRF guard, replay recovery contract. |
| SQLite | File-backed SQLite mutation with database-root allowlist and verification gates. |
| Mail draft | Draft create/update/delete lifecycle with recipient and content binding. **Does not send email.** |

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

> **Note:** The example intentionally overrides the dev config port via `FERRUMD_BIND_ADDR`; the dev config alone uses `8080`. For production-like deployments, build a release binary (`cargo build --release`) and run `./target/release/ferrumd` with a production config.

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

## Performance & Validation

See [`docs/PRODUCTION_NOTES.md`](./docs/PRODUCTION_NOTES.md) for detailed stress-test baselines, SQLite write-queue tuning, and PostgreSQL scaling guidance.

Highlights from local validation (release binary, post-write-queue):

| Scenario | Throughput | p50 Latency | Error Rate |
|----------|------------|-------------|------------|
| Health (50 workers) | ~33,000 req/s | 1.3ms | 0% |
| Execution pipeline (5 workers) | ~58 pipelines/s | 16ms | 0% |
| SQLite contention (50 workers) | ~289 req/s | 30ms | 0% |

> These are local engineering benchmarks, not production guarantees. Your results will depend on hardware, store choice, and workload shape.

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

## FAQ

**Q: Is FerrumGate a managed service or SaaS?**
> No. It is open-source software you run in your own infrastructure. Single-tenant by design.

**Q: Does FerrumGate send email?**
> No. The mail draft adapter manages draft create/update/delete with recipient and content binding. It does not send email.

**Q: Is MCP HTTP/SSE supported?**
> stdio MCP is implemented and locally validated. Streamable HTTP / SSE and resumability are experimental and not yet production-ready.

**Q: Does FerrumGate provide compliance certification (SOC 2, ISO 27001, etc.)?**
> No. FerrumGate provides audit-oriented provenance and evidence chains. Compliance certification is outside the scope of the open-source project.

**Q: Is PostgreSQL production-HA out of the box?**
> PostgreSQL runtime is supported and CI live-tested. Production HA/multi-node topology, replication, and failover are operator responsibilities and are not managed by this repository.

**Q: Can multiple tenants share one FerrumGate instance?**
> No. Multi-tenancy is not implemented; FerrumGate is single-tenant by design.

**Q: What is the difference between `cargo run` and the release binary in the quickstart?**
> `cargo run` compiles and runs in debug mode — fine for local development. For production-like or pilot deployments, use `cargo build --release` and run the resulting `./target/release/ferrumd` binary.

**Q: How do I report a security issue?**
> Please open a private security advisory via GitHub Security Advisories for this repository.

## Roadmap

Current direction and near-term priorities:

| Area | Direction | Status |
|------|-----------|--------|
| Core governance lifecycle | Stabilize intent → policy → capability → execution → verify → provenance | Stable |
| SQLite performance | Write queue + PRAGMA tuning validated; operator tuning guide available | Stable |
| PostgreSQL support | Runtime and CI live tests passing; HA topology remains operator-owned | Beta |
| MCP integration | stdio tools validated; HTTP/SSE deferred | Experimental |
| Operator experience | ferrumctl, ferrum-tui, Helm chart, monitoring rules, backup/restore drills | Implemented |
| Multi-tenancy | Not on current roadmap | Not implemented |
| Compliance certification | Out of scope for open-source project | Not implemented |

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

If you are an AI assistant working in this repository, see [AGENTS.md](./AGENTS.md) for the canonical workspace instructions, constraints, and tooling.

## License

[Apache-2.0](./LICENSE)
