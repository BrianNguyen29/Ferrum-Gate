# 65 — Path 2 Target Questionnaire

> **Status**: Updated 09/05/2026 — filled from signed doc 99 worksheet. Operator-owned.
> **Purpose**: Capture required target environment and operator details needed to complete Path 2 execution.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. Conditional pilot only — NOT full production-ready.
> **Constraint**: Do not fill real secrets; use placeholders. Do not claim full production-ready.

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
| Operator name | PROVIDE | BrianNguyen |
| Operator role/title | PROVIDE | Owner/Operator |
| Operator email | PROVIDE | (withheld for security — operator-managed) |
| Supervisor/countersigner name (if required) | PROVIDE | N/A |
| Date of questionnaire completion | PROVIDE | 09/05/2026 |

---

## Section B — Target Host Environment

| Field | Marker | Value |
|-------|--------|-------|
| Target host FQDN or IP | PROVIDE | `34.158.51.8` |
| SSH access method (password/key) | PROVIDE | Key-based (`~/.ssh/google_compute_engine`) |
| SSH user for ferrumd deployment | PROVIDE | `ubuntu` |
| Operating system and version | PROVIDE | Ubuntu 24.04 LTS amd64 |
| Package manager available (apt/yum/dnf) | PROVIDE | apt |
| systemd available | PROVIDE | Yes |
| Target network zone (DMZ/internal) | PROVIDE | GCP custom VPC `ferrumgate-nonprod-vpc`, zone `asia-southeast1-a` |
| Existing firewall rules affecting port 80/443/8080 | PROVIDE | SSH (22) and app (19080) from `118.69.4.63/32`; HTTP (80) and HTTPS (443) public `0.0.0.0/0` |

---

## Section C — TLS / Domain

| Field | Marker | Value |
|-------|--------|-------|
| Public domain for ferrumgate | PROVIDE | `34-158-51-8.nip.io` (temporary — replace with real domain for production) |
| TLS certificate available (yes/using certbot/existing CA) | PROVIDE | Let's Encrypt via Caddy automatic HTTPS |
| TLS certificate path (if existing) | PROVIDE | Caddy-managed (automatic) |
| TLS private key path (if existing) | PROVIDE | Caddy-managed (automatic) |
| Certificate expiry date | PROVIDE | Let's Encrypt managed |
| DNS A record pointing to target host | PROVIDE (confirm) | `34-158-51-8.nip.io` → `34.158.51.8` (nip.io provides this) |

---

## Section D — Storage and Backup

| Field | Marker | Value |
|-------|--------|-------|
| SQLite store path (e.g., `/var/lib/ferrumgate/ferrumgate.db`) | PROVIDE | `/var/lib/ferrumgate/data/ferrumgate.db` |
| SQLite store parent directory (must be writable by ferrumgate user) | PROVIDE | `/var/lib/ferrumgate/data` |
| Backup output directory (e.g., `/var/backups/ferrumgate`) | PROVIDE | `/var/lib/ferrumgate/backups` |
| Backup retention policy (days) | PROVIDE | 7 days + offsite copy required before production |
| Backup schedule (daily at what time UTC) | PROVIDE | 15-minute systemd timer (`ferrumgate-backup.timer`, `OnUnitActiveSec=15min`) |
| Available disk space for store + backups | PROVIDE | 30GB pd-balanced (VM boot disk) |

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
| Bearer token generation method | PROVIDE (openssl/certutil/other) | Generated on-VM during bootstrap |
| Token generated (yes/no) | PROVIDE (confirm) | Yes |
| Token stored securely (env file, secrets manager) | PROVIDE (confirm) | Yes — `/etc/ferrumgate/env` (root-only) |
| Token env file path | PROVIDE | `/etc/ferrumgate/env` |

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

## Section H — Caddy / Reverse Proxy

| Field | Marker | Value |
|-------|--------|-------|
| Caddy installed (yes/no) | PROVIDE | Yes |
| Caddy config path | PROVIDE | `/etc/caddy/Caddyfile` |
| Caddy version | PROVIDE | v2.11.2 |
| `server_name` updated to actual FQDN | PROVIDE (confirm) | Yes — `34-158-51-8.nip.io` |
| TLS cert/key paths updated | PROVIDE (confirm) | Caddy-managed automatic |
| `proxy_set_header Authorization $http_authorization` used (not `Bearer`) | PROVIDE (confirm) | Caddy forwards Authorization header as-is |
| HTTP → HTTPS redirect configured | PROVIDE (confirm) | Caddy HTTP-01 challenge on 80 |
| Caddy tested with `caddy validate` | PROVIDE (confirm) | Yes |

---

## Section I — systemd Units

| Field | Marker | Value |
|-------|--------|-------|
| `ferrumd.service` deployed to `/etc/systemd/system/` | PROVIDE (confirm) | Yes |
| `ferrumgate-backup.service` deployed | PROVIDE (confirm) | Yes |
| `ferrumgate-backup.timer` deployed (or cron configured) | PROVIDE (confirm) | Yes — `ferrumgate-backup.timer` |
| `ferrumd.service` enabled and started | PROVIDE (confirm) | Yes — `active` |
| `ferrumgate-backup.timer` enabled (if using timer) | PROVIDE (confirm) | Yes — `enabled` |
| `journalctl -u ferrumd -f` shows healthy startup | PROVIDE (confirm) | Yes |

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

**Scope**: Conditional single-node SQLite pilot only. NOT full production-ready.

G2 gates have been signed by BrianNguyen on 09/05/2026 for conditional single-node pilot scope only
(via doc 99 worksheet, copied to docs 54/59). This questionnaire captures Path 2 target environment
values. No full production-ready claim. PostgreSQL/HA/multi-node not in scope.

---

*FerrumGate v1 RC-ready/conditional. Single-node SQLite only. Conditional pilot signed 09/05/2026. No full production-ready claim.*
