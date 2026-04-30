# 65 — Path 2 Target Questionnaire

> **Status**: Documentation-only. Operator-owned.
> **Purpose**: Capture required target environment and operator details needed to complete Path 2 execution.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.
> **Constraint**: Do not fill real secrets; use placeholders. Do not sign G2 or claim pilot accepted.

---

## Purpose

This questionnaire captures the information the operator must provide before or during Path 2
execution. Fields marked **PROVIDE** require operator input. Fields marked **OPERATOR-GENERATED**
are produced by a command the operator runs. Fields marked **DERIVED DURING DEPLOY** are
recorded after deployment commands execute.

All sensitive fields (tokens, credentials) must be handled out-of-band. Do not store
real secrets in documentation or version control.

---

## Section A — Operator Information

| Field | Marker | Value |
|-------|--------|-------|
| Operator name | PROVIDE | |
| Operator role/title | PROVIDE | |
| Operator email | PROVIDE | |
| Supervisor/countersigner name (if required) | PROVIDE | |
| Date of questionnaire completion | PROVIDE | |

---

## Section B — Target Host Environment

| Field | Marker | Value |
|-------|--------|-------|
| Target host FQDN or IP | PROVIDE | |
| SSH access method (password/key) | PROVIDE | |
| SSH user for ferrumd deployment | PROVIDE | |
| Operating system and version | PROVIDE | |
| Package manager available (apt/yum/dnf) | PROVIDE | |
| systemd available | PROVIDE | |
| Target network zone (DMZ/internal) | PROVIDE | |
| Existing firewall rules affecting port 80/443/8080 | PROVIDE | |

---

## Section C — TLS / Domain

| Field | Marker | Value |
|-------|--------|-------|
| Public domain for ferrumgate | PROVIDE | |
| TLS certificate available (yes/using certbot/existing CA) | PROVIDE | |
| TLS certificate path (if existing) | PROVIDE | |
| TLS private key path (if existing) | PROVIDE | |
| Certificate expiry date | PROVIDE | |
| DNS A record pointing to target host | PROVIDE (confirm) | |

---

## Section D — Storage and Backup

| Field | Marker | Value |
|-------|--------|-------|
| SQLite store path (e.g., `/var/lib/ferrumgate/ferrumgate.db`) | PROVIDE | |
| SQLite store parent directory (must be writable by ferrumgate user) | PROVIDE | |
| Backup output directory (e.g., `/var/backups/ferrumgate`) | PROVIDE | |
| Backup retention policy (days) | PROVIDE | |
| Backup schedule (daily at what time UTC) | PROVIDE | |
| Available disk space for store + backups | PROVIDE | |

---

## Section E — Monitoring

| Field | Marker | Value |
|-------|--------|-------|
| Prometheus instance available (yes/no) | PROVIDE | |
| Prometheus scrape target reachable | PROVIDE | |
| AlertManager available (yes/no) | PROVIDE | |
| Existing Grafana instance (yes/no) | PROVIDE | |
| Contact email for monitoring alerts | PROVIDE | |

---

## Section F — Authentication

| Field | Marker | Value |
|-------|--------|-------|
| Bearer token generation method | PROVIDE (openssl/certutil/other) | |
| Token generated (yes/no) | PROVIDE (confirm) | |
| Token stored securely (env file, secrets manager) | PROVIDE (confirm) | |
| Token env file path | PROVIDE | `/etc/ferrumgate/ferrumd.env` |

---

## Section G — ferrumd Configuration

| Field | Marker | Value |
|-------|--------|-------|
| Config file path | PROVIDE | `/etc/ferrumgate/ferrumgate.toml` |
| Config adapted from `configs/ferrumgate.prod.toml` | PROVIDE (confirm) | |
| `auth_mode` set to `Bearer` | PROVIDE (confirm) | |
| `bind_addr` set to `127.0.0.1:8080` | PROVIDE (confirm) | |
| `store.path` set to actual store path | PROVIDE (confirm) | |
| `store.synchronous` (recommended: `NORMAL`) | PROVIDE | |
| WAL enabled (recommended: `true`) | PROVIDE | |

---

## Section H — nginx / Reverse Proxy

| Field | Marker | Value |
|-------|--------|-------|
| nginx installed (yes/no) | PROVIDE | |
| nginx config path | PROVIDE | |
| nginx config adapted from `configs/examples/nginx-ferrumgate.conf` | PROVIDE (confirm) | |
| `server_name` updated to actual FQDN | PROVIDE (confirm) | |
| TLS cert/key paths updated | PROVIDE (confirm) | |
| `proxy_set_header Authorization $http_authorization` used (not `Bearer`) | PROVIDE (confirm) | |
| HTTP → HTTPS redirect configured | PROVIDE (confirm) | |
| nginx tested with `nginx -t` | PROVIDE (confirm) | |

---

## Section I — systemd Units

| Field | Marker | Value |
|-------|--------|-------|
| `ferrumd.service` deployed to `/etc/systemd/system/` | PROVIDE (confirm) | |
| `ferrumgate-backup.service` deployed | PROVIDE (confirm) | |
| `ferrumgate-backup.timer` deployed (or cron configured) | PROVIDE (confirm) | |
| `ferrumd.service` enabled and started | PROVIDE (confirm) | |
| `ferrumgate-backup.timer` enabled (if using timer) | PROVIDE (confirm) | |
| `journalctl -u ferrumd -f` shows healthy startup | PROVIDE (confirm) | |

---

## Section J — Pre-Pilot Probe Verification

Complete after all services are deployed. Record actual probe responses.

| Probe | Expected | Observed | Pass/Fail |
|-------|----------|----------|-----------|
| `curl http://127.0.0.1:8080/v1/healthz` | 200 | | |
| `curl http://127.0.0.1:8080/v1/readyz` | 200 | | |
| `curl http://127.0.0.1:8080/v1/readyz/deep` | 200 | | |
| `curl http://127.0.0.1:8080/v1/metrics` | 200 + prometheus | | |
| `curl -s -o /dev/null -w "%{http_code}" https://<domain>/v1/healthz` | 200 | | |
| `curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer <token>" https://<domain>/v1/readyz/deep` | 200 | | |

---

## Section K — Pre-Drill Evidence Review

Complete before running compensation drills.

| Item | Status |
|------|--------|
| Doc 63 (Target Environment Spec) reviewed | ☐ |
| All PROVIDE fields completed with actual values | ☐ |
| No real secrets stored in docs | ☐ |
| Backup scheduler tested manually | ☐ |
| TLS proxy verified with test certificate | ☐ |
| ferrumd logs reviewed for startup warnings | ☐ |

---

## Section L — Handoff Readiness Checklist

Before handing off to the operator signoff gate, confirm:

- [ ] All Section J probes return expected results
- [ ] All Section K items checked
- [ ] No real secrets in any documentation
- [ ] Backup scheduler has run at least once successfully
- [ ] Monitoring scrape target confirmed reachable

---

## Disclaimer

**No G2 complete claim. No pilot accepted. No production-ready claim.**

This questionnaire captures information for Path 2 preparation only. G2 gates remain pending
until the operator executes drills, reviews evidence, and signs doc 59 §G2.8 and doc 54.

---

*FerrumGate v1 RC-ready/conditional. Single-node SQLite only. No production-ready claim.*
