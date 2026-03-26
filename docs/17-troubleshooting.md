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

## Provenance-specific operational issues

### Lineage gap or broken parent_event_id chain
- Export the full execution stream and inspect it as JSONL:
  `ferrumctl server inspect-provenance --execution-id <id> --all-pages > provenance.jsonl`
- Use `ferrumctl server inspect-event <event_id> --ancestry --descendants --json` on the suspicious event to inspect parent-edge continuity.
- Compare `inspect-lineage <execution_id>` with the raw JSONL export to distinguish a real graph gap from a query/export misunderstanding.
- External event ingest (`POST /v1/provenance/events/external`) is fail-closed: a 404/409 response means the referenced execution or parent event does not exist or does not belong to the same execution. Verify `execution_id` and `parent_event_id` before retrying.
- See [provenance-audit-runbook.md](runbooks/provenance-audit-runbook.md) — Scenario 2 for full diagnosis steps.

### External event not appearing in lineage
- Confirm the ingest call returned a successful response containing an `event` payload.
- Re-run `ferrumctl server inspect-provenance --execution-id <id> --all-pages | jq -c 'select(.kind == "ExternalEventObserved")'` to verify the event was actually recorded.
- Verify `parent_event_id` points to an event that exists in the same execution lineage.
- Inspect the returned event metadata and confirm `metadata.source_system` and `metadata.source_event_id` match what you intended to record.

### Compliance evidence export shows empty or partial data
- `inspect-provenance --all-pages` writes one JSON event per line to stdout; if the file is sparse or empty, check the shell redirection path first.
- Raw exports are an internal artifact. Prepare a redacted derivative before wider sharing; do not assume the CLI removes sensitive metadata for you.
- If the exported JSONL is unexpectedly sparse, check that the execution reached terminal state and that events were written to the persistent store (not lost to an in-memory fallback).
- For full audit steps, see [provenance-audit-runbook.md](runbooks/provenance-audit-runbook.md) — Scenario 4.

### Provenance CLI returns no events for a known execution
- Verify the execution_id is correct and the execution exists: `ferrumctl server inspect-execution <id>`.
- If `inspect-execution` returns not found, the execution was never started or was lost due to an in-memory store fallback.
- Check whether `ferrumd` started with a persistent SQLite store or fell back to `sqlite::memory:?cache=shared` (see `cargo run -p ferrumd -- --print-effective-config`).
