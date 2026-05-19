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
        "FERRUMGATE_BEARER_TOKEN": "your-token-here",
        "FERRUMGATE_GATEWAY_URL": "https://your-gateway.example.com"
      }
    }
  }
}
```

> **Security warning**: Do not commit bearer tokens to version control. Use environment variables or a secrets manager.

## Tools list

The server exposes 19 tools:

### Read-only tools (9)

| Tool | Purpose |
|------|---------|
| `health_check` | Check gateway health |
| `list_intents` | List submitted intents |
| `list_executions` | List executions |
| `inspect_execution` | Inspect a specific execution |
| `inspect_lineage` | Query lineage for an execution |
| `inspect_provenance` | Query provenance events |
| `list_approvals` | List pending approvals |
| `list_policy_bundles` | List policy bundles |
| `get_active_policy_bundle` | Get currently active policy bundle |

### Lifecycle tools (mutating, require auth)

| Tool | Purpose |
|------|---------|
| `submit_intent` | Submit a new intent |
| `evaluate_intent` | Evaluate an intent against policy |
| `mint_capability` | Mint a capability for an allowed proposal |
| `authorize_execution` | Authorize execution using a capability |
| `prepare_execution` | Prepare side effects (snapshots, etc.) |
| `execute_prepared` | Execute the prepared action |
| `verify_execution` | Verify execution outcome |
| `compensate_execution` | Roll back / compensate if needed |
| `query_lineage` | Query lineage chain |

### Approval tools

| Tool | Purpose |
|------|---------|
| `approve_proposal` | Approve a proposal requiring approval |
| `reject_proposal` | Reject a proposal with reason |

## Auth setup

1. Generate a bearer token:
   ```bash
   openssl rand -hex 32
   ```

2. Configure ferrumd with `auth_mode = "Bearer"` and the token.

3. Provide the token to the MCP server via `FERRUMGATE_BEARER_TOKEN`.

4. Verify: mutating tools should fail closed without the token.

## Lifecycle flow via MCP

```
submit_intent
→ evaluate_intent
→ mint_capability
→ authorize_execution
→ prepare_execution
→ execute_prepared
→ verify_execution
→ query_lineage
```

## Security warnings

- **Never print tokens in logs**: The MCP server and gateway redact tokens in output.
- **Mutating tools fail closed**: Without valid auth, all mutating tools return errors.
- **Capabilities are single-use**: Do not retry execution with the same capability.
- **TTL is 300s max**: Capabilities expire automatically.
- **Scope must not exceed intent**: The gateway enforces this; do not bypass.

## Status caveat

> **production-ready = NO**. MCP target-host smoke is planned but not yet executed. Local validation is complete for tested paths: connection (`ping`, `initialize`, `tools/list`), auth (mutating tools fail closed without token), lifecycle (`submit_intent` → `verify_execution`), read queries (`get_execution`, `list_intents`), and `query_lineage` pass locally after bugfix. See [`docs/implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md`](../../implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md) §MCP bugfix regression.

## Related docs

- [`quickstart.md`](./quickstart.md) — curl/CLI quickstart.
- [`concepts.md`](./concepts.md) — Intent, capability, provenance explained.
- [`adapter-reference.md`](./adapter-reference.md) — Per-adapter operations and rollback behavior.
