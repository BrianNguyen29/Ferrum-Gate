# MCP Integration Guide

> **Status**: Locally validated. Connection, auth, lifecycle, read queries, and query_lineage pass after bugfix. Target-host guide is pending validation.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## What is MCP in FerrumGate

FerrumGate provides an MCP stdio server (`ferrum-mcp-server`) that exposes governance lifecycle tools to MCP clients (e.g., Claude Desktop, Cursor, or custom agents).

The MCP server does **not** implement its own policy or rollback logic. It calls the gateway governance API and returns the result.

## Running the MCP server

```bash
cargo run --bin ferrum-mcp-server
```

Or use the release binary:

```bash
./target/release/ferrum-mcp-server
```

## Sample MCP client config

For Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "ferrumgate": {
      "command": "/path/to/ferrum-mcp-server",
      "env": {
        "FERRUM_GATEWAY_BEARER_TOKEN": "placeholder-token-for-local-dev",
        "FERRUM_GATEWAY_URL": "http://127.0.0.1:18080"
      }
    }
  }
}
```

> **Security warning**: Do not commit bearer tokens to version control. Use environment variables or a secrets manager. For local development only, use a placeholder token as shown above.

## Transport

The server uses **line-delimited JSON** over stdio. Each line is a single JSON-RPC request or response. Ensure your MCP client config uses stdio transport.

## Tools list

The server exposes 19 tools:

### Read-only tools (9)

| Tool | Purpose |
|------|---------|
| `ferrum_gate_health` | Check gateway health |
| `ferrum_gate_readyz_deep` | Deep readiness probe |
| `ferrum_gate_list_intents` | List submitted intents |
| `ferrum_gate_get_execution` | Get execution status by ID |
| `ferrum_gate_query_lineage` | Query lineage for an execution |
| `ferrum_gate_list_approvals` | List pending approvals |
| `ferrum_gate_list_policy_bundles` | List policy bundles |
| `ferrum_gate_list_bridges` | List registered runtime bridges |
| `ferrum_gate_list_bridge_tools` | List tools for a specific bridge |

### Lifecycle tools (8, mutating, require auth)

| Tool | Purpose |
|------|---------|
| `ferrum_gate_submit_intent` | Submit a new intent |
| `ferrum_gate_evaluate_intent` | Evaluate an intent against policy |
| `ferrum_gate_mint_capability` | Mint a capability for an allowed proposal |
| `ferrum_gate_authorize_execution` | Authorize execution using a capability |
| `ferrum_gate_prepare_execution` | Prepare side effects (snapshots, etc.) |
| `ferrum_gate_execute_prepared` | Execute the prepared action |
| `ferrum_gate_verify` | Verify execution outcome |
| `ferrum_gate_compensate` | Roll back / compensate if needed |

### Approval tools (2)

| Tool | Purpose |
|------|---------|
| `ferrum_gate_approve_intent` | Approve a proposal requiring approval |
| `ferrum_gate_reject_intent` | Reject a proposal with reason |

## Auth setup

1. Generate a bearer token:
   ```bash
   openssl rand -hex 32
   ```

2. Configure ferrumd with `auth_mode = "Bearer"` and the token.

3. Provide the token to the MCP server via `FERRUM_GATEWAY_BEARER_TOKEN`.

4. Verify: mutating tools should fail closed without the token.

## Lifecycle flow via MCP

```
ferrum_gate_submit_intent
→ ferrum_gate_evaluate_intent
→ ferrum_gate_mint_capability
→ ferrum_gate_authorize_execution
→ ferrum_gate_prepare_execution
→ ferrum_gate_execute_prepared
→ ferrum_gate_verify
→ ferrum_gate_query_lineage
```

## Security warnings

- **Never print tokens in logs**: The MCP server and gateway redact tokens in output.
- **Mutating tools fail closed**: Without valid auth, all mutating tools return errors.
- **Capabilities are single-use**: Do not retry execution with the same capability.
- **TTL is 300s max**: Capabilities expire automatically.
- **Scope must not exceed intent**: The gateway enforces this; do not bypass.

## Status caveat

> **production-ready = NO**. MCP target-host smoke is planned but not yet executed. Local validation is complete for tested paths: connection (`ping`, `initialize`, `tools/list`), auth (mutating tools fail closed without token), lifecycle (`ferrum_gate_submit_intent` → `ferrum_gate_verify`), read queries (`ferrum_gate_get_execution`, `ferrum_gate_list_intents`), and `ferrum_gate_query_lineage` pass locally after bugfix. See [`docs/implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md`](../../implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md) §MCP bugfix regression.

## Related docs

- [`quickstart.md`](./quickstart.md) — curl/CLI quickstart.
- [`concepts.md`](./concepts.md) — Intent, capability, provenance explained.
- [`adapter-reference.md`](./adapter-reference.md) — Per-adapter operations and rollback behavior.
