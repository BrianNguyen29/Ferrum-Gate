# FerrumGate in 10 Minutes

> **Status**: Scaffold. Not yet validated end-to-end.
> **Scope**: Local demo only. Not for production deployment.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Prerequisites

- Rust toolchain (stable)
- `cargo` and `make`
- A POSIX shell

## Quickstart flow

### 1. Build

```bash
cargo build --release
```

### 2. Start ferrumd locally

```bash
./target/release/ferrumd
```

Default: SQLite in-memory, auth disabled, loopback only.

### 3. Health check

```bash
curl http://localhost:8080/v1/healthz
```

Expected: `{"status":"ok"}`

### 4. Submit intent

```bash
curl -X POST http://localhost:8080/v1/intents \
  -H "Content-Type: application/json" \
  -d '{"action":"fs.write","target":"/tmp/ferrum-demo.txt","parameters":{"content":"hello"}}'
```

### 5. Evaluate

```bash
curl -X POST http://localhost:8080/v1/evaluate \
  -H "Content-Type: application/json" \
  -d '{"intent_id":"<INTENT_ID_FROM_STEP_4>"}'
```

### 6. Mint capability

```bash
curl -X POST http://localhost:8080/v1/capabilities \
  -H "Content-Type: application/json" \
  -d '{"intent_id":"<INTENT_ID>","proposal_id":"<PROPOSAL_ID>","requested_ttl_secs":300}'
```

### 7. Authorize

```bash
curl -X POST http://localhost:8080/v1/executions/authorize \
  -H "Content-Type: application/json" \
  -d '{"capability_id":"<CAPABILITY_ID>"}'
```

### 8. Prepare

```bash
curl -X POST http://localhost:8080/v1/executions/prepare \
  -H "Content-Type: application/json" \
  -d '{"execution_id":"<EXECUTION_ID>"}'
```

### 9. Execute

```bash
curl -X POST http://localhost:8080/v1/executions/execute \
  -H "Content-Type: application/json" \
  -d '{"execution_id":"<EXECUTION_ID>"}'
```

### 10. Verify

```bash
curl -X POST http://localhost:8080/v1/executions/verify \
  -H "Content-Type: application/json" \
  -d '{"execution_id":"<EXECUTION_ID>"}'
```

### 11. Query lineage

```bash
curl "http://localhost:8080/v1/lineage?execution_id=<EXECUTION_ID>"
```

## ferrumctl version

```bash
# Health
ferrumctl health

# List intents
ferrumctl list-intents

# Inspect execution
ferrumctl inspect-execution --execution-id <ID>
```

## MCP version

See [`mcp-integration.md`](./mcp-integration.md) for MCP client setup.

## Status caveat

> **production-ready = NO**. This quickstart runs with auth disabled and an in-memory SQLite store. Do not expose to the internet. For pilot deployment, see [`hosted-deployment.md`](./hosted-deployment.md).

## Related docs

- [`concepts.md`](./concepts.md) — Intent, proposal, capability, provenance explained.
- [`operator.md`](./operator.md) — Config, backup, monitoring.
- [`docs/ROADMAP.md`](../../ROADMAP.md) — What is still missing for production.
