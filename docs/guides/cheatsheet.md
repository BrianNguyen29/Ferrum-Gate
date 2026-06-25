# FerrumGate in 5 Minutes — Cheat Sheet

Quick copy-paste commands for local evaluation. Not for production.

## Prerequisites

- Rust stable, `cargo`, `make`, `curl`
- See [`quickstart.md`](./quickstart.md) for the full 10-minute walkthrough.

## Start the gateway (dev mode)

```bash
# SQLite in-memory, auth disabled, loopback only
cargo run --bin ferrumd -- --config configs/ferrumgate.dev.toml
```

Or bind to a specific port:
```bash
FERRUMD_BIND_ADDR=127.0.0.1:18080 \
  cargo run --bin ferrumd -- --config configs/ferrumgate.dev.toml
```

## Health check

```bash
curl http://127.0.0.1:18080/v1/healthz
# → {"status":"ok"}
```

## Deep readiness (checks store/queue)

```bash
curl http://127.0.0.1:18080/v1/readyz/deep
# → 200 if healthy; 503 if degraded
```

## Run the MCP server

```bash
cargo run -p ferrum-integrations-mcp --bin ferrum-mcp-server
```

## CLI quick checks

```bash
# List intents
ferrumctl intents list

# Check audit log chain (remote)
ferrumctl admin audit verify

# Export portable audit bundle
ferrumctl admin audit export --bundle /tmp/audit-bundle

# Verify portable audit bundle locally
ferrumctl admin audit verify --bundle /tmp/audit-bundle

# Backup (SQLite)
ferrumctl backup --output /tmp/ferrumgate-backup.sql
```

## Common validation

```bash
make check   # cargo check --workspace
make fmt     # cargo fmt --all
make lint    # clippy with -D warnings
make test    # cargo test --workspace
make docs    # validate docs links
```

## PostgreSQL feature gate (local evaluation)

```bash
cargo check --bin ferrumd --features postgres
cargo test -p ferrum-store --features postgres
```

> **Note:** `cargo run` is debug mode. For release-like evaluation, use `cargo build --release`.

## Related docs

- [`quickstart.md`](./quickstart.md) — Full walkthrough with intent→capability→execute flow.
- [`concepts.md`](./concepts.md) — Intent, policy, provenance, lineage, adapters.
- [`faq.md`](./faq.md) — Scope, HA, MCP, compliance, security reporting.
