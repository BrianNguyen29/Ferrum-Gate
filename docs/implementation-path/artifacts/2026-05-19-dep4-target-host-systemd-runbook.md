# DEP-4 Target-Host Systemd Runbook — Prepared/Preflight Only

> **Status**: PREPARED — runbook/checklist ready for operator execution. DEP-4 remains OPEN until real target-host evidence is captured.
> **Date**: 2026-05-19
> **Scope**: Single-node systemd deployment of ferrumd (SQLite or PostgreSQL). NOT production-ready. NOT HA.
> **Owner**: Operator + Engineering

---

## Non-claims

- **DEP-4 is NOT complete**: This document is a prepared runbook only. It does not constitute evidence of a successful target-host deployment.
- **NOT production-ready**: Systemd packaging is a prerequisite, not a production claim.
- **NOT validated on a live target host**: These steps must be executed on a real systemd-managed host before DEP-4 can close.
- **NOT PostgreSQL production hardening**: If using PostgreSQL, complete `docs/production-readiness-v2/02-postgres-production-plan.md` prerequisites first.
- **NOT HA/multi-node**: Single-node only.
- **NO real secrets in this doc**: All tokens, DSNs, and hostnames are placeholders. Redact sensitive values in evidence artifacts.

---

## Prerequisites

| # | Item | Verification |
|---|------|-------------|
| P1 | Target host runs a systemd-based Linux distribution (Debian 12+, Ubuntu 22.04+, RHEL 9+, etc.) | `systemctl --version` |
| P2 | `ferrumd` binary is available at a known path (e.g., `/usr/local/bin/ferrumd`) | `file /usr/local/bin/ferrumd` |
| P3 | Config directory exists: `/etc/ferrumgate/` | `ls -ld /etc/ferrumgate` |
| P4 | Data directory exists with correct ownership: `/var/lib/ferrumgate` (owner: `ferrumgate:ferrumgate`) | `ls -ld /var/lib/ferrumgate` |
| P5 | Log directory exists with correct ownership: `/var/log/ferrumgate` (owner: `ferrumgate:ferrumgate`) | `ls -ld /var/log/ferrumgate` |
| P6 | Service user exists: `ferrumgate` (system user, no login shell) | `id ferrumgate` |
| P7 | Reverse proxy (nginx/Caddy) is configured and TLS is terminated upstream if exposing to a network | Check proxy config |
| P8 | If using PostgreSQL: `docs/production-readiness-v2/02-postgres-production-plan.md` PG-1 baseline is complete and `FERRUMD_STORE_DSN` is set | `psql $DSN -c "SELECT 1"` |

---

## Step-by-step operator checklist

### Step 1 — Install binary and verify permissions

```bash
# Example: copy from build artifact or package
sudo cp ferrumd /usr/local/bin/ferrumd
sudo chmod 755 /usr/local/bin/ferrumd
sudo chown root:root /usr/local/bin/ferrumd

# Verify
/usr/local/bin/ferrumd --version
```

**Evidence to capture:**
- Output of `ferrumd --version`
- `sha256sum /usr/local/bin/ferrumd`

### Step 2 — Create config files

Create `/etc/ferrumgate/ferrumgate.toml`:

```toml
# /etc/ferrumgate/ferrumgate.toml
# Replace placeholders before use. Do not commit real values.

[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "bearer"
log_filter = "info"

# SQLite example (single-node pilot)
store_dsn = "sqlite:///var/lib/ferrumgate/ferrumgate.db"

# PostgreSQL example (production foundation)
# store_dsn = "postgres://USER:PASSWORD@localhost:5432/ferrumgate"
# pg_max_connections = 10
# pg_min_idle = 2
# pg_acquire_timeout_secs = 5
```

Create `/etc/ferrumgate/ferrumd.env`:

```bash
# /etc/ferrumgate/ferrumd.env
# Generate token with: openssl rand -hex 32
# Permissions must be 600.

FERRUMD_BEARER_TOKEN=<REDACTED_GENERATE_WITH_OPENSSL>
```

Set permissions:

```bash
sudo chown -R root:ferrumgate /etc/ferrumgate
sudo chmod 750 /etc/ferrumgate
sudo chmod 600 /etc/ferrumgate/ferrumd.env
sudo chmod 644 /etc/ferrumgate/ferrumgate.toml
```

**Evidence to capture:**
- `ls -la /etc/ferrumgate/`
- `getfacl /etc/ferrumgate/ferrumd.env` (or `stat` output showing mode 600)

### Step 3 — Install systemd unit

Copy `configs/examples/ferrumd.service` to `/etc/systemd/system/ferrumd.service` (or `ferrumgate.service` if your naming convention differs).

Review and adapt the unit file:

```ini
[Unit]
Description=FerrumGate daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ferrumgate
Group=ferrumgate
EnvironmentFile=-/etc/ferrumgate/ferrumd.env
ExecStart=/usr/local/bin/ferrumd --config /etc/ferrumgate/ferrumgate.toml
Restart=on-failure
RestartSec=5s
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=/var/lib/ferrumgate /var/log/ferrumgate

[Install]
WantedBy=multi-user.target
```

**Evidence to capture:**
- `cat /etc/systemd/system/ferrumd.service`
- `systemd-analyze verify /etc/systemd/system/ferrumd.service` output

### Step 4 — Reload systemd and enable service

```bash
sudo systemctl daemon-reload
sudo systemctl enable ferrumd
```

**Evidence to capture:**
- `systemctl is-enabled ferrumd` output (should report `enabled`)

### Step 5 — Start service and capture runtime evidence

```bash
sudo systemctl start ferrumd
sleep 3
systemctl status ferrumd --no-pager
```

**Required evidence to capture:**
- Full `systemctl status ferrumd --no-pager` output (including Active state, PID, memory, logs)
- `journalctl -u ferrumd --no-pager -n 50` output (first 50 lines after start)

### Step 6 — Health and readiness probes

Run from the target host (or via reverse proxy if configured):

```bash
# Healthz
curl -s http://127.0.0.1:8080/v1/healthz

# Readyz (deep)
curl -s http://127.0.0.1:8080/v1/readyz/deep
```

**Required evidence to capture:**
- HTTP status code for each endpoint
- Full response body for each endpoint
- Timestamp of each request

### Step 7 — Auth probe (Bearer token validation)

```bash
# Expect 200 with valid token
curl -s -o /dev/null -w "%{http_code}" \
  -H "Authorization: Bearer <REDACTED_USE_REAL_TOKEN>" \
  http://127.0.0.1:8080/v1/healthz

# Expect 401 with missing/invalid token
curl -s -o /dev/null -w "%{http_code}" \
  http://127.0.0.1:8080/v1/healthz
```

**Required evidence to capture:**
- HTTP status for authenticated request (expected: 200)
- HTTP status for unauthenticated request (expected: 401)
- `curl` command with token redacted in the evidence artifact

### Step 8 — Restart resilience check

```bash
sudo systemctl restart ferrumd
sleep 3
systemctl status ferrumd --no-pager
curl -s http://127.0.0.1:8080/v1/healthz
```

**Required evidence to capture:**
- `systemctl status ferrumd --no-pager` after restart
- Healthz HTTP status after restart

### Step 9 — Stop service (preflight/rollback mode)

```bash
sudo systemctl stop ferrumd
systemctl status ferrumd --no-pager
```

**Required evidence to capture:**
- `systemctl status ferrumd --no-pager` showing `inactive (dead)`

---

## Rollback and cleanup

| Scenario | Action |
|----------|--------|
| Service fails to start | Check `journalctl -u ferrumd -n 100`; verify config syntax; verify binary path; verify permissions on `/etc/ferrumgate/ferrumd.env` |
| Wrong config deployed | `sudo systemctl stop ferrumd`; edit `/etc/ferrumgate/ferrumgate.toml`; `sudo systemctl start ferrumd` |
| Token compromised | `sudo systemctl stop ferrumd`; regenerate with `openssl rand -hex 32`; update `/etc/ferrumgate/ferrumd.env`; `sudo systemctl start ferrumd` |
| Full uninstall | `sudo systemctl disable --now ferrumd`; `sudo rm /etc/systemd/system/ferrumd.service`; `sudo systemctl daemon-reload`; `sudo rm -rf /etc/ferrumgate` (preserve backups first) |
| SQLite data corruption | Restore from `ferrumctl backup` artifact. See `docs/guides/operator.md` §Backup/Restore. |
| PostgreSQL connectivity failure | Verify `FERRUMD_STORE_DSN`; check PostgreSQL logs; verify network/firewall between ferrumd and PG host |

---

## Evidence artifact template

When DEP-4 is executed on a target host, create an evidence artifact named:

```
docs/implementation-path/artifacts/YYYY-MM-DD-dep4-target-host-systemd-evidence.md
```

Populate it with:

1. Target host OS/version (e.g., `cat /etc/os-release`).
2. `ferrumd --version` output.
3. `sha256sum /usr/local/bin/ferrumd`.
4. `ls -la /etc/ferrumgate/`.
5. `systemd-analyze verify` output.
6. `systemctl is-enabled ferrumd`.
7. `systemctl status ferrumd --no-pager` (start, restart, stop).
8. `journalctl -u ferrumd --no-pager -n 50` (startup logs).
9. Healthz/readyz HTTP status and body.
10. Auth probe results (token redacted).
11. Any deviations from this runbook and operator signoff.

---

## Gate closure criteria

DEP-4 can only be marked complete when ALL of the following are true:

- [ ] This runbook was followed on a real target host.
- [ ] Evidence artifact exists with all required captures above.
- [ ] `systemctl status ferrumd --no-pager` shows `active (running)`.
- [ ] `/v1/healthz` returns HTTP 200 from the target host.
- [ ] `/v1/readyz/deep` returns HTTP 200 with `healthy: true`.
- [ ] Bearer auth probe shows 200 with valid token and 401 without.
- [ ] Operator signoff is recorded in the evidence artifact.
- [ ] No production-ready or HA claims are introduced.

---

## Related docs

- [`docs/implementation-path/artifacts/2026-05-19-systemd-validation-evidence.md`](./2026-05-19-systemd-validation-evidence.md) — Local preflight validation (does not close DEP-4).
- [`configs/examples/ferrumd.service`](../../../configs/examples/ferrumd.service) — Systemd unit example.
- [`configs/examples/ferrumd.env.example`](../../../configs/examples/ferrumd.env.example) — Environment file example.
- [`docs/guides/hosted-deployment.md`](../../guides/hosted-deployment.md) — Deployment mode overview.
- [`docs/production-readiness-v2/08-hosted-deployment-plan.md`](../../production-readiness-v2/08-hosted-deployment-plan.md) — Hosted deployment plan.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PostgreSQL hardening prerequisites.
