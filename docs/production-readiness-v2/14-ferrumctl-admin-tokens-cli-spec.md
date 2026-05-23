# 14 — ferrumctl Admin Tokens CLI Surface Spec

> **Status**: Implemented — `ferrumctl admin tokens` CLI (list/create/revoke/rotate) implemented 2026-05-21. Conditional on operator review boundaries. See [`10-evidence-checklist.md`](./10-evidence-checklist.md) §Phase 4.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **Depends on**: [`06-admin-operator-ux-plan.md`](06-admin-operator-ux-plan.md), [`13-token-api-contract.md`](13-token-api-contract.md)

---

## Goal

Specify the `ferrumctl admin tokens` subcommand surface so that:
1. Engineering can implement it once BLK-UX-4 is unblocked.
2. The operator can review the CLI ergonomics before implementation.
3. The CLI aligns with the existing `ferrumctl admin` patterns (status, approvals, executions, backup).

## Current `ferrumctl admin` pattern

The `AdminCommand` enum in `bins/ferrumctl/src/main.rs` currently has:

- `admin status` — Aggregated health/readiness/metrics (local CLI, no new server API)
- `admin approvals {list, get, resolve}` — Wired to existing `/v1/approvals` endpoints
- `admin executions {list, get, cancel}` — Wired to existing execution endpoints
- `admin backup {create, verify, restore}` — Offline/local, no server API

The `tokens` subcommand will follow the same pattern: a `Tokens` variant under `AdminCommand` with nested subcommands, each mapping to a server API call.

## Proposed enum additions

```rust
// In bins/ferrumctl/src/main.rs, extend AdminCommand:
enum AdminCommand {
    Status,
    Approvals { sub: AdminApprovalsCommand },
    Executions { sub: AdminExecutionsCommand },
    Backup { sub: AdminBackupCommand },
    /// Manage scoped tokens (list, create, revoke, rotate).
    /// Requires server-side token admin APIs (Phase 4).
    Tokens {
        #[command(subcommand)]
        sub: AdminTokensCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AdminTokensCommand {
    /// List scoped tokens (metadata only; no secret values).
    List {
        /// Filter by actor ID (exact match).
        #[arg(long, value_name = "ID")]
        actor_id: Option<String>,

        /// Filter by role.
        #[arg(long, value_name = "ROLE")]
        role: Option<String>,

        /// Show only active tokens (exclude revoked and expired).
        #[arg(long)]
        active_only: bool,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },

    /// Create a new scoped token.
    /// The token value is printed exactly once and never retrievable again.
    Create {
        /// Actor ID (username, service name, etc.).
        #[arg(long, value_name = "ID")]
        actor_id: String,

        /// Role to assign.
        #[arg(long, value_name = "ROLE")]
        role: String,

        /// Explicit scope list (repeatable). If omitted, uses role defaults.
        #[arg(long, value_name = "SCOPE")]
        scope: Vec<String>,

        /// Token description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,

        /// Expiration in days from now. Alternative to --expires-at.
        #[arg(long, value_name = "N", group = "expiry")]
        expires_in_days: Option<u32>,

        /// Absolute expiration timestamp (ISO 8601). Alternative to --expires-in-days.
        #[arg(long, value_name = "TIMESTAMP", group = "expiry")]
        expires_at: Option<String>,

        /// Output the created token as JSON (includes the secret token_value).
        #[arg(long)]
        json: bool,
    },

    /// Revoke a scoped token.
    Revoke {
        /// Token ID to revoke.
        token_id: String,

        /// Reason for revocation.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Skip interactive confirmation.
        #[arg(long)]
        force: bool,
    },

    /// Rotate a scoped token (revoke old, create new with same actor/role/scopes).
    Rotate {
        /// Token ID to rotate.
        token_id: String,

        /// New expiration in days from now.
        #[arg(long, value_name = "N", group = "expiry")]
        expires_in_days: Option<u32>,

        /// New absolute expiration timestamp (ISO 8601).
        #[arg(long, value_name = "TIMESTAMP", group = "expiry")]
        expires_at: Option<String>,

        /// Reason for rotation.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Output the new token as JSON (includes the secret token_value).
        #[arg(long)]
        json: bool,

        /// Skip interactive confirmation.
        #[arg(long)]
        force: bool,
    },
}
```

## Command reference

### `ferrumctl admin tokens list`

List scoped tokens with optional filters.

**Examples:**

```bash
# List all active tokens
ferrumctl admin tokens list --active-only

# List tokens for a specific actor
ferrumctl admin tokens list --actor-id operator-alice

# List tokens for a role, JSON output
ferrumctl admin tokens list --role operator --format json

# Paginate
ferrumctl admin tokens list --limit 10
```

**Text output format:**

```
TOKEN_ID                ACTOR_ID        ROLE       EXPIRES_AT           STATUS
 tok_2vPqN5L8wX9mK3J    operator-alice  operator   2026-08-20T00:00:00Z active
 tok_7xYzA1B2cD3eF4G    agent-beta      agent      2026-06-20T00:00:00Z active
 tok_AbC123...           auditor-gamma   auditor    2026-05-01T00:00:00Z expired
```

### `ferrumctl admin tokens create`

Create a new scoped token. The `token_value` is printed **exactly once**.

**Examples:**

```bash
# Create an operator token with default scopes, expires in 90 days
ferrumctl admin tokens create \
  --actor-id operator-alice \
  --role operator \
  --expires-in-days 90 \
  --description "On-call operator token"

# Create an agent token with explicit scopes
ferrumctl admin tokens create \
  --actor-id agent-beta \
  --role agent \
  --scope intent:submit \
  --scope proposal:evaluate \
  --scope capability:mint \
  --expires-in-days 30

# Create and output as JSON for scripting
ferrumctl admin tokens create \
  --actor-id service-logger \
  --role read_only \
  --expires-in-days 365 \
  --json
```

**Expected output (text):**

```
Token created successfully.

Token ID:    tok_2vPqN5L8wX9mK3J
Token Value: fgt_aB3x9...zK7m2
Actor ID:    operator-alice
Role:        operator
Scopes:      approval:resolve, provenance:read, policy:read
Expires At:  2026-08-20T00:00:00Z

IMPORTANT: Save the token value now. It will never be shown again.
```

### `ferrumctl admin tokens revoke`

Revoke a token. Requires confirmation unless `--force` is used.

**Examples:**

```bash
# Revoke with confirmation prompt
ferrumctl admin tokens revoke tok_2vPqN5L8wX9mK3J --reason "Compromised"

# Revoke without prompt (scripting)
ferrumctl admin tokens revoke tok_2vPqN5L8wX9mK3J --force
```

**Expected output:**

```
Token tok_2vPqN5L8wX9mK3J revoked successfully.
Reason: Compromised
```

### `ferrumctl admin tokens rotate`

Rotate a token (revoke old, create new with same actor/role/scopes). The new `token_value` is printed exactly once.

**Examples:**

```bash
# Rotate with default new expiry (max TTL)
ferrumctl admin tokens rotate tok_2vPqN5L8wX9mK3J --reason "Scheduled rotation"

# Rotate with explicit new expiry
ferrumctl admin tokens rotate tok_2vPqN5L8wX9mK3J \
  --expires-in-days 90 \
  --reason "Quarterly rotation"

# Rotate for scripting (JSON output, no confirmation)
ferrumctl admin tokens rotate tok_2vPqN5L8wX9mK3J --json --force
```

**Expected output (text):**

```
Token rotated successfully.

Old Token ID: tok_2vPqN5L8wX9mK3J (revoked)
New Token ID: tok_7xYzA1B2cD3eF4G
New Token Value: fgt_def456...uvw012

IMPORTANT: Save the new token value now. It will never be shown again.
```

## Error handling

All commands follow the existing ferrumctl pattern:

1. Parse CLI args with clap.
2. Call the server API via the internal HTTP client (`client` module).
3. On `4xx`/`5xx`, print a clear error message and exit non-zero.
4. On success, print formatted output.

**Example error:**

```
Error: Token creation failed: 403 Forbidden
  required scope admin:tokens
```

## Wiring to server APIs

| CLI command | HTTP method | Route | Notes |
|-------------|-------------|-------|-------|
| `tokens list` | `GET` | `/v1/admin/tokens?{filters}` | Maps query params from CLI flags |
| `tokens create` | `POST` | `/v1/admin/tokens` | Sends JSON body; prints `token_value` once |
| `tokens revoke` | `DELETE` | `/v1/admin/tokens/{token_id}` | Sends optional `reason` in body |
| `tokens rotate` | `POST` | `/v1/admin/tokens/{token_id}/rotate` | Sends `expires_at`/`reason`; prints new `token_value` once |

## Tests

When implemented, the following test categories should be added:

1. **CLI parse tests**: Verify clap parses all flag combinations correctly.
2. **Client mock tests**: Verify the client module constructs the correct HTTP requests.
3. **Integration tests**: Verify the full CLI → server → store → response round-trip.

## Non-claims

- **NOT production-ready**: Scoped-token enforcement requires explicit operator enablement; bearer-only remains the production pilot auth mode until then.
- **NOT final**: Operator review may request UX changes.
- **Engineering evidence only**: Implementation evidence compiled 2026-05-22; full operator signoff and Phase 4 signoff remaining.

## Related docs

- [`06-admin-operator-ux-plan.md`](06-admin-operator-ux-plan.md) — Admin/operator UX plan
- [`13-token-api-contract.md`](13-token-api-contract.md) — Token API contract
- [`12-endpoint-to-scope-mapping.md`](12-endpoint-to-scope-mapping.md) — Endpoint-to-scope mapping

---

*End of file — ferrumctl Admin Tokens CLI Surface Spec (planning artifact only).*
