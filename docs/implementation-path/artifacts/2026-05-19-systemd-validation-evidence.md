# Systemd Example Validation Evidence — 2026-05-19

## Status

- **Scope**: DEP-4 local preflight validation for `configs/examples/ferrumd.service` and `configs/examples/ferrumd.env.example`.
- **Verdict**: ⚠️ PARTIAL — local preflight passed; real `systemctl start/status ferrumd` on a target host remains pending.
- **Production-ready**: NO.
- **Target-host / cloud**: NOT CLAIMED.
- **PostgreSQL production deployment**: NOT CLAIMED.
- **HA/multi-node**: NOT CLAIMED.

This artifact records local validation of the systemd example files after DEP-3. It intentionally does **not** close DEP-4 because no real systemd-managed host run produced `systemctl status ferrumd` output.

## Files under validation

| File | Purpose |
|------|---------|
| `configs/examples/ferrumd.service` | Example systemd unit for ferrumd |
| `configs/examples/ferrumd.env.example` | Example environment file for bearer token |
| `configs/ferrumgate.dev.toml` | Local smoke-test config only |

## Systemd unit syntax preflight

Command:

```bash
systemd-analyze verify configs/examples/ferrumd.service
```

Initial result before documentation URL fix:

```text
configs/examples/ferrumd.service:3: Invalid URL, ignoring: ./docs/implementation-path/62-path-2-operator-runbook.md
ferrumd.service: Command /usr/local/bin/ferrumd is not executable: No such file or directory
```

Action taken:

- Changed `Documentation=` from a relative path to a valid GitHub HTTPS documentation URL.

Result after fix:

```text
ferrumd.service: Command /usr/local/bin/ferrumd is not executable: No such file or directory
```

Interpretation:

- The invalid `Documentation=` warning is resolved.
- The remaining warning is expected in this workstation checkout because `/usr/local/bin/ferrumd` is an installation path, not the repository build path.
- This is a syntax/preflight validation only; it is not a replacement for target-host `systemctl` runtime evidence.

## Service-equivalent local binary smoke

Command:

```bash
FERRUMD_BIND_ADDR=127.0.0.1:19082 \
FERRUMD_BEARER_TOKEN=fg_live_REPLACE_WITH_GENERATED_TOKEN \
target/release/ferrumd --config configs/ferrumgate.dev.toml
```

Observed logs:

```text
starting ferrumd with config: auth_mode=disabled, bind_addr=127.0.0.1:19082, store_dsn=sqlite::memory:
ferrumd listening on 127.0.0.1:19082
```

### Healthz

Command:

```bash
curl -s http://127.0.0.1:19082/v1/healthz
```

Observed result:

- HTTP status: `200`
- Response body: `{"status":"ok"}`

### Readyz (deep)

Command:

```bash
curl -s http://127.0.0.1:19082/v1/readyz/deep
```

Observed result:

- HTTP status: `200`
- Response body:

```json
{
  "status": "ok",
  "healthy": true,
  "components": [
    { "component": "store", "status": "ok", "healthy": true },
    { "component": "write_queue", "status": "ok: depth=0, threshold=100", "healthy": true }
  ]
}
```

## Open DEP-4 runtime evidence

The following evidence is still required before DEP-4 can be marked complete:

1. Install or stage `ferrumd` at `/usr/local/bin/ferrumd` on a systemd-managed host.
2. Install a real config at `/etc/ferrumgate/ferrumgate.toml` and an environment file at `/etc/ferrumgate/ferrumd.env`.
3. Run:

```bash
systemctl daemon-reload
systemctl start ferrumd
systemctl status ferrumd --no-pager
curl -s http://127.0.0.1:<configured-port>/v1/healthz
curl -s http://127.0.0.1:<configured-port>/v1/readyz/deep
systemctl stop ferrumd
```

## Non-claims

- **NOT production-ready**: This is local preflight validation only.
- **NOT a target-host systemd proof**: No real `systemctl status ferrumd` output was captured.
- **NOT PostgreSQL production validation**: The smoke used local in-memory SQLite.
- **NOT HA/multi-node**: Single local process only.
- **NO real token committed**: The bearer token value used is the documented placeholder `fg_live_REPLACE_WITH_GENERATED_TOKEN`.
