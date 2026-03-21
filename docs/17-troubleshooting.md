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
- Capability leases still use the in-memory capability service, so active leases do not survive process restart even when the SQLite store is persistent.

### ferrumd not reachable
- Check the effective bind address from CLI/env/config (`--bind`, `FERRUMD_BIND_ADDR`, config file).
- Check that no other process is using the configured port.
- If startup fails on a non-loopback bind, verify that bearer auth is enabled or that `allow_insecure_nonlocal` was explicitly set.

### bearer auth returns 401
- All non-health routes require `Authorization: Bearer <token>` when `auth.mode = "bearer"`.
- Verify that `FERRUMD_BEARER_TOKEN` or `auth.bearer_token` is set on the server side.
- Verify that the client (`ferrumctl` or curl) is sending the same token.
- `ferrumctl` accepts `--bearer-token` or `FERRUMCTL_BEARER_TOKEN`.

### prod config fails at startup
- `configs/ferrumgate.prod.toml` enables `auth.mode = "bearer"`.
- Startup will fail until a bearer token is supplied via config or `FERRUMD_BEARER_TOKEN`.
- Startup also fails if a non-loopback bind is requested while auth is disabled and `allow_insecure_nonlocal` is not enabled.

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
