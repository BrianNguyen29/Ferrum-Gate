# 17 — Troubleshooting

## ferrumd Startup Failures

This section covers common `ferrumd` daemon startup errors and how to resolve them.

### "binding to non-loopback address requires --allow-insecure-nonlocal-bind when auth is disabled"

**Cause:** Binding to a non-loopback IP (e.g., `0.0.0.0`) with `auth_mode = "disabled"` is insecure.

**Fixes (choose one):**
- Set `auth_mode = "bearer"` in config to enable token authentication
- Set `allow_insecure_nonlocal_bind = true` in config (development only)
- Bind to loopback address (e.g., `127.0.0.1:8080`)

**Config precedence:** CLI flags > environment variables > config file > defaults

### "bearer token cannot be empty when auth mode is bearer"

**Cause:** Using `auth_mode = "bearer"` without providing a valid token.

**Fix:** Set a non-empty `bearer_token` in your config file or via `FERRUMD_BEARER_TOKEN` env var.

### "failed to connect to sqlite" / "failed to apply migrations"

**Cause:** Database connectivity or initialization failure.

**Diagnostics:**
- Verify the store DSN is valid (e.g., `sqlite::memory:`, `sqlite://path/to/db.sqlite`)
- For SQLite files: ensure the parent directory exists and is writable
- Check available disk space
- Verify file permissions on the store path

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

## API Errors

### 401 Unauthorized
- Missing or invalid Bearer token
- Set `Authorization: Bearer <token>` header
- Check that `FERRUMCTL_BEARER_TOKEN` or CLI argument is correct

### 404 Not Found
- Check that the resource exists (execution_id, approval_id, etc.)
- Verify the correct endpoint path

## CLI Issues

### "connection refused"
- Server may not be running
- Check `FERRUMCTL_SERVER_URL` is correct
- Try `ferrumctl server health` to verify server is up

## Related

- Single-node operations runbook: `docs/ferrumgate-roadmap-v1/18-single-node-operations-runbook.md`
- v1 single-node operator checks: `./20-v1-single-node-operator-checks.md`
- v1 single-node observability minimums: `./21-v1-single-node-observability-minimums.md`
