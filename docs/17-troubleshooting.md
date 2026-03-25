# 17 — Troubleshooting

## Agent bị deny
Kiểm tra:
- scope mismatch
- args mismatch
- capability invalid
- manifest mismatch

## Action bị quarantine
Kiểm tra:
- taint score
- trust labels
- contradiction giữa intent và proposal

## Rollback không chạy
Kiểm tra:
- rollback class
- contract state
- adapter path
- compensation plan

## Gateway flow đứt đoạn
Kiểm tra:
- proposal
- policy evaluate
- capability mint
- rollback prepare
- verify
- provenance chain

## Common operational issues (current reality)

### State resets on restart
- If `ferrumd` auto-loads `configs/ferrumgate.dev.toml`, the store uses `sqlite://ferrumgate.dev.db` and execution/provenance state should survive restart.
- If state is still resetting, check whether `ferrumd` actually found a config file or fell back to `sqlite::memory:?cache=shared`.
- Capabilities are now persisted in SQLite via `SqliteCapabilityService`. On startup, `ferrumd` reconciles legacy active capabilities with execution history. Active capability leases survive process restart when the SQLite store is persistent.

### ferrumd not reachable
- Check the effective bind address from CLI/env/config (`--bind`, `FERRUMD_BIND_ADDR`, config file).
- Check that no other process is using the configured port.
- If startup fails on a non-loopback bind, verify that bearer auth is enabled or that `allow_insecure_nonlocal` was explicitly set.
- Run `cargo run -p ferrumd -- --print-effective-config` to confirm which config source won for bind/store/auth and whether the startup guard would pass.
- Run `cargo run -p ferrumd -- --check-startup-guard` when you want a preflight verdict without starting the listener.

### bearer auth returns 401
- All non-health routes require `Authorization: Bearer <token>` when `auth.mode = "bearer"`.
- Verify that `FERRUMD_BEARER_TOKEN` or `auth.bearer_token` is set on the server side.
- Verify that the client (`ferrumctl` or curl) is sending the same token.
- `ferrumctl` accepts `--bearer-token` or `FERRUMCTL_BEARER_TOKEN`.

### prod config fails at startup
- `configs/ferrumgate.prod.toml` enables `auth.mode = "bearer"`.
- Startup will fail until a bearer token is supplied via config or `FERRUMD_BEARER_TOKEN`.
- Startup also fails if a non-loopback bind is requested while auth is disabled and `allow_insecure_nonlocal` is not enabled.
- The effective-config output shows whether the bearer token came from CLI, env, or file, but only exposes presence/absence, not the raw token.

### readiness is unclear after startup
- Run `cargo run -p ferrumctl -- server ready` to hit `/v1/readyz` directly.
- If `/v1/healthz` is `ok` but operators still suspect config drift, compare it with `cargo run -p ferrumd -- --print-effective-config` from the same environment.

### HTTP adapter mutation has no automatic rollback
HTTP rollback is a **no-op by design**. If an HTTP adapter mutates remote state (e.g., a PUT or DELETE to an external API), the rollback adapter will not undo that mutation. Operators must manually compensate in this case. See `15-deployment-and-operations.md` for the open gap.

### HTTP auth / header allowlist mismatch
- HTTP adapter respects a configured header allowlist. Requests with headers not in the allowlist are rejected at the adapter layer.
- If an agent's proposal includes headers that are not in the allowlist, the action will fail at the adapter level before reaching the remote service.
- Check the adapter config and the `ferrumgate-integrator-contract.v1.yaml` for the current allowlist.

### SQLite locking or local filesystem path issues
- If using a filesystem adapter or SQLite store, ensure the process has write access to the target path.
- SQLite may lock the database file if multiple processes attempt concurrent access; ferrumd is single-process today.
- Git adapter operations require a valid git repository at the configured path.

### Policy evaluate returns deny with no obvious reason
- Verify the intent scope matches a policy rule.
- Check that the agent's manifest is present and pinned correctly.
- Verify the PDP engine has not been misconfigured (currently `StaticPdpEngine`).

## Approval issues

### Approval not found (404)
- Verify the approval ID is correct (UUID format).
- The approval may have already been resolved; use `ferrumctl server inspect-approvals` to check current pending approvals.
- If the approval expired (>15 minutes), it must be re-created by re-authorizing the execution.

### Approval resolves but execution does not advance (409 Conflict)
- Only `Pending` approvals can be resolved. If the approval is already `Granted`, `Denied`, or `Expired`, the server returns a 409 Conflict.
- The linked execution must be in `AwaitingApproval` state. If it has already transitioned (e.g., execution was cancelled or timed out), resolution fails.
- Check `ferrumctl server inspect-approval <id>` to see the current state.

### Resolution returns 401 Unauthorized
- All non-health routes require `Authorization: Bearer <token>` when `auth.mode = "bearer"`.
- Verify the server's `FERRUMD_BEARER_TOKEN` matches the client's `--bearer-token` or `FERRUMCTL_BEARER_TOKEN`.

### Approval expired (already past expires_at)
- Pending approvals expire after 15 minutes. Expired approvals cannot be resolved.
- Re-authorize the execution to create a new pending approval with a fresh `expires_at`.

### Execution in AwaitingApproval but no approval found
- Use `ferrumctl server inspect-approvals --execution-id <id>` to look up approvals linked to the specific execution.
- If no approval exists, the capability may have been consumed by another execution or the proposal was never in a state requiring approval.

### R3 (IrreversibleHighConsequence) execution stuck in AwaitingApproval
- These executions require explicit approval before the capability is consumed.
- Operators must actively poll for pending R3 approvals using `ferrumctl server inspect-approvals`.
- Set up monitoring or alerting on the pending-approval queue to avoid approvals expiring unnoticed.
