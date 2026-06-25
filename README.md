# FerrumGate

> **Scoped, Auditable, Reversible.**

[![ci](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml/badge.svg)](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](https://www.rust-lang.org/)

 [Tiếng Việt](./README.vi.md) · [Docs](./docs/README.md) · [Quickstart](./docs/guides/quickstart.md) · [Cheat Sheet](./docs/guides/cheatsheet.md) · [Operator Guide](./docs/guides/operator.md) · [Security Model](./docs/guides/security-model.md)

<p align="center">
  <img src="./assets/banner.svg" alt="FerrumGate — Scoped. Auditable. Reversible. Governance gateway for AI agents." width="100%">
</p>

FerrumGate is a governance gateway for AI agents that replaces ambient tool access with intent-scoped, single-use capabilities — so every action is policy-checked, rollback-prepared, and provenance-recorded.

---

## Why a gateway?

Traditional API keys answer *“who can call this?”* but not *“what was the agent trying to do?”*, *“was this in scope?”*, or *“can we roll it back?”*.

FerrumGate sits between the agent and the tool, turning every side effect into a controlled lifecycle: compile intent, evaluate policy, mint a scoped capability, prepare rollback, execute, verify, and record provenance. **Agents do not receive direct, ambient authority over production systems.** They receive narrow, short-lived, explainable permission to perform a specific action, with rollback and lineage built in before the side effect happens.

---

## Three pillars

**Scoped** — No ambient authority. Every capability is bound to a declared intent, specific resources, and a TTL ≤ 300 s. Once used or expired, the lease is gone.

**Auditable** — Policy decisions, capability grants, execution attempts, and terminal states are recorded as a chain of provenance. Evidence is append-only by design.

**Reversible** — Rollback and recovery contracts are prepared before any adapter performs a side effect. If verification fails, the system can compensate or roll back instead of leaving a partial mutation.

---

## Quickstart

Prerequisites: Rust stable, `cargo`, `make`, `curl`.

```bash
# Build and run the development gateway
FERRUMD_BIND_ADDR=127.0.0.1:18080 \
  cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml

# Check liveness
curl http://127.0.0.1:18080/v1/healthz

# Full walkthrough in 10 minutes
cat docs/guides/quickstart.md
```

> **Note:** `cargo run` is debug mode — fine for local work. For production-like deployments, use `cargo build --release` and run `./target/release/ferrumd` with a production config and bearer auth.

---

## Execution lifecycle

![Execution lifecycle: Compile, Authorize, Execute, Verify and Record](./assets/lifecycle-flow.svg)

Every mutating action follows a minimum lineage chain: **Intent → PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal state** (committed, compensated, rolled back, or failed). Store-backed transitions, fencing tokens, and lineage gates enforce the order. Unknown mutating tools fail closed unless explicitly bound.

---

## Adapters and entrypoints

FerrumGate provides bounded adapters for common agent side effects, each with sandboxing, allowlists, and rollback contracts:

- **Filesystem** — write, delete, move, copy, append, chmod, directory create/delete with sandboxing and snapshots.
- **Git** — commit, branch create/delete, tag create/delete with repository-root allowlists.
- **HTTP** — mutation with rustls-backed client, no redirects, bounded timeout, SSRF guard, and replay recovery contract.
- **SQLite** — file-backed mutation with database-root allowlist and verification gates.
- **Mail draft** — draft create/update/delete with recipient and content binding. **Does not send email.**

Entrypoints and tooling:

- `ferrumd` — gateway daemon
- `ferrumctl` — CLI for health, readiness, audit, policy, approvals, lifecycle outbox, backup/restore
- `ferrum-mcp-server` — MCP stdio server (exposes gateway tools to MCP clients)
- `ferrum-migrate` — SQLite-to-PostgreSQL migration
- `ferrum-stress` — machine-readable stress/smoke scenarios
- `ferrum-tui` — terminal operator dashboard

---

## Project status

- **Stable** — intent lifecycle, policy evaluation, capability minting, rollback prepare/verify/compensate, SQLite write queue, provenance chain, bearer/scoped/OIDC/agent auth.
- **Implemented** — filesystem, HTTP, Git, SQLite, mail draft adapters; S3 adapter (experimental); `ferrumctl` CLI; `ferrum-stress`; `ferrum-tui`; Prometheus metrics; rate limiting; Helm chart.
- **Beta** — PostgreSQL runtime (local and CI live-tested). Production HA/multi-node topology is operator-managed, not provided by this repo.
- **Experimental** — MCP Streamable HTTP / SSE transport.
- **Not implemented / out of scope** — multi-tenancy, managed service, email sending, compliance certification, MCP resumability.

> **Honesty note:** FerrumGate is not a turnkey HA product or compliance certification. Operators still own deployment topology, TLS, secrets, backups, database HA, and production acceptance testing.

---

## Documentation

If you are new to the project, read in this order:

1. [Concepts](./docs/guides/concepts.md)
2. [Quickstart](./docs/guides/quickstart.md)
3. [Adapter Reference](./docs/guides/adapter-reference.md)
4. [Security Model](./docs/guides/security-model.md)
5. [Operator Guide](./docs/guides/operator.md)
6. [Production Notes](./docs/PRODUCTION_NOTES.md)

Other references:

- [API Guide](./docs/guides/api.md) · [MCP Integration](./docs/guides/mcp-integration.md) · [Demo Flows](./docs/guides/demo-flows.md)
- [Policy Authoring](./docs/guides/policy-authoring.md) · [Troubleshooting](./docs/guides/troubleshooting.md)
- [FAQ](./docs/guides/faq.md) · [Roadmap](./docs/ROADMAP.md)
- [Helm Chart](./deploy/helm/ferrumgate/README.md) · [Monitoring Config](./configs/monitoring/README.md)

---

## Development and validation

```bash
make fmt      # formatting
make check    # cargo check
make lint     # clippy
make test     # tests
make docs     # link validation
make validate # expanded gate
make audit    # dependency audit
make secret-scan
```

See [CONTRIBUTING.md](./CONTRIBUTING.md) for conventions. If you are an AI assistant, see [AGENTS.md](./AGENTS.md) for workspace constraints and tooling.

---

## License

[Apache-2.0](./LICENSE)
