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
ferrumd uses `sqlite::memory:?cache=shared`. All state (capability leases, action history, rollback contracts) is lost when the process exits. **This is expected behavior in the current dev configuration.** If you need persistence, a persistent SQLite path or external database is required — not yet implemented.

### ferrumd not reachable
- ferrumd binds to `127.0.0.1:8080` (hardcoded). It is not reachable on other interfaces by default.
- Check that no other process is using port 8080.
- If you need remote access, you must proxy through the local interface or reconfigure the hardcoded bind address in `bins/ferrumd/src/main.rs`.

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
