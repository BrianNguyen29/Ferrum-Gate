# DEP-4 Target-Host Systemd Evidence — 2026-05-19

## Status

- **Scope**: DEP-4 target-host systemd runtime validation on `ferrumgate-nonprod`.
- **Verdict**: ✅ PASS for target-host systemd runtime validation.
- **Production-ready**: NO.
- **Full G2**: NOT COMPLETE.
- **PostgreSQL production deployment**: NO.
- **HA/multi-node**: NO.

This artifact records target-host evidence for the already-deployed VM-specific systemd unit `ferrumgate.service`. The generic DEP-4 runbook refers to `ferrumd.service`, but the actual target host uses `ferrumgate.service` with the `ferrumd` binary.

## Temporary access note

SSH was initially blocked by source-IP allowlisting. With operator authorization, the SSH firewall rule was temporarily updated to include the current `/32` source IPs needed for this run. After DEP-4/DEP-6 evidence capture, the SSH firewall rule was restored to its prior allowlist:

```text
sourceRanges:
- 118.69.4.63/32
```

No `0.0.0.0/0` SSH exposure was used.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Project | `fairy-b13f4` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| OS | Ubuntu 24.04.4 LTS |
| Service | `ferrumgate.service` |
| Binary | `/opt/ferrumgate/ferrumd` |
| Config | `/etc/ferrumgate/ferrumgate.toml` |
| Env file | `/etc/ferrumgate/env` |
| Bind | `0.0.0.0:19080` |
| Store | SQLite at `/var/lib/ferrumgate/ferrumgate.db` |

## Systemd service evidence

Commands executed on target:

```bash
systemctl is-enabled ferrumgate
systemctl is-active ferrumgate
systemctl status ferrumgate --no-pager
systemctl cat ferrumgate --no-pager
systemd-analyze verify /etc/systemd/system/ferrumgate.service
```

Observed results:

- `systemctl is-enabled ferrumgate`: `enabled`
- `systemctl is-active ferrumgate`: `active`
- `systemctl status ferrumgate --no-pager`: `active (running)` with main process `/opt/ferrumgate/ferrumd --config /etc/ferrumgate/ferrumgate.toml`
- `systemd-analyze verify /etc/systemd/system/ferrumgate.service`: no output, indicating no validation errors.

Sanitized status excerpt:

```text
ferrumgate.service - FerrumGate ferrumd Phase 3A non-prod
Loaded: loaded (/etc/systemd/system/ferrumgate.service; enabled)
Active: active (running)
Main PID: ferrumd
CGroup: /system.slice/ferrumgate.service
        /opt/ferrumgate/ferrumd --config /etc/ferrumgate/ferrumgate.toml
```

## Binary and filesystem evidence

Observed:

- `/opt/ferrumgate/ferrumd` exists and is executable.
- `/opt/ferrumgate/ferrumctl` exists and is executable.
- `sha256sum /opt/ferrumgate/ferrumd`:

```text
ad4648387e87727b49c296a5d474825131508241488fc9f6c94a9ff40baf1de5  /opt/ferrumgate/ferrumd
```

Relevant directories exist:

```text
/etc/ferrumgate
/var/lib/ferrumgate
/var/lib/ferrumgate/backups
/var/log/ferrumgate
```

The active env file is `/etc/ferrumgate/env` with mode `640` and owner `root:ferrumgate`. The token value was not printed or recorded.

## Health and readiness probes

Commands executed from the target host:

```bash
curl -s -o /tmp/dep4-healthz-body.txt -w "healthz_http=%{http_code}\n" http://127.0.0.1:19080/v1/healthz
curl -s -o /tmp/dep4-readyz-body.txt -w "readyz_http=%{http_code}\n" http://127.0.0.1:19080/v1/readyz/deep
```

Observed results:

| Endpoint | HTTP | Body |
|----------|------|------|
| `/v1/healthz` | 200 | `{"status":"ok"}` |
| `/v1/readyz/deep` | 200 | `{"status":"ok","healthy":true,...}` with store and write queue healthy |

## Bearer auth probe

Token handling:

- Token was sourced from `/etc/ferrumgate/env` into a shell variable.
- Token value was never printed.
- Evidence records only HTTP status and sanitized response bodies.

Protected endpoint used:

```bash
POST http://127.0.0.1:19080/v1/intents/compile
```

Observed results:

| Probe | HTTP | Meaning |
|-------|------|---------|
| Valid bearer token | 200 | Authenticated request accepted |
| Missing bearer token | 401 | Unauthenticated request rejected |

Sanitized valid response included an intent envelope. UUIDs were redacted in captured output.

## Restart resilience check

Command:

```bash
systemctl restart ferrumgate
sleep 3
systemctl status ferrumgate --no-pager
curl -s http://127.0.0.1:19080/v1/healthz
```

Observed results:

- `systemctl is-active ferrumgate`: `active`
- Restarted service status: `active (running)`
- `healthz_after_restart_http=200`
- Body: `{"status":"ok"}`

## Non-claims

- **NOT production-ready**: This validates DEP-4 runtime behavior only.
- **NOT full G2**: Real-domain/full-G2 requirements remain separate.
- **NOT PostgreSQL production**: Target is SQLite-backed.
- **NOT HA/multi-node**: Single VM only.

## Gate result

DEP-4 target-host systemd runtime validation is complete for the current single-node target host:

- [x] Real target host used.
- [x] Systemd service enabled and active.
- [x] `systemd-analyze verify` passed.
- [x] Healthz and deep readiness returned HTTP 200.
- [x] Bearer auth probe returned 200 with valid token and 401 without token.
- [x] Restart resilience check passed.
- [x] No production-ready or HA claim introduced.
