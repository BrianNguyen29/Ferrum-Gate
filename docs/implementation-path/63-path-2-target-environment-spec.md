# 63 — Path 2 Target Environment Specification

> **Status**: Updated 09/05/2026 — values filled from signed doc 99 worksheet. Doc 54 signed by BrianNguyen 09/05/2026 for conditional single-node SQLite pilot scope.
> **Purpose**: Structured spec template for capturing target non-prod/production environment details required for Path 2 pilot preparation.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. Conditional pilot only — NOT full production-ready.
> **Constraint**: Do not claim full production-ready, do not start PostgreSQL, do not add real secrets.

---

## Field Provenance Markers

Each field in this document carries one of three provenance markers indicating who provides the value:

| Marker | Meaning | When Filled |
|--------|---------|-------------|
| **PROVIDE** | Operator supplies from their infrastructure | Before deployment |
| **OPERATOR-GENERATED** | Operator generates via command | Before deployment |
| **DERIVED DURING DEPLOY** | Produced by a deployment command; recorded here | During/after deployment |

**Real secrets** (tokens, credentials) must be generated out-of-band. Use safe placeholders in documentation.
Do NOT store real values in this spec.

---

## Purpose

This document provides a fillable template for capturing the target non-prod or production-like
environment specification before beginning Path 2 pilot preparation. Fields have been populated with
GCP non-prod evidence from the signed doc 99 worksheet.

**Scope**: Conditional single-node SQLite pilot only. NOT full production-ready. PostgreSQL/HA/multi-node not in scope.
Doc 54 was signed by BrianNguyen on 09/05/2026 for conditional single-node pilot scope only.

---

## Environment Classification

| Field | Value | Notes |
|-------|-------|-------|
| Environment type | [x] Non-prod  [ ] Staging  [ ] Prod-like  [ ] Production | Select one |
| Path 2 preparation mode | [x] Option 2 / target environment spec  [ ] Option 3 / local-only simulation | Option 2 is required before target G2 evidence; Option 3 is practice only |
| Pilot scope | Single-node SQLite only | PostgreSQL blocked until Phase 3 |

**Explicit Non-Claims**:
- No full production-ready claim (conditional single-node pilot only)
- No G2 complete beyond conditional pilot scope
- No PostgreSQL/HA/multi-node
- Operator signature populated from doc 99 worksheet (BrianNguyen, 09/05/2026)

---

## Section 1 — Host and Network

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Target host / IP | `<target-host>` | `34.158.51.8` | GCP non-prod host for ferrumd |
| SSH host | `<ssh-host>` | `ferrumgate-nonprod` | For remote management |
| SSH user | `<ssh-user>` | `ubuntu` | Operator account on target |
| SSH key path | `<ssh-key-path>` | `/home/uong_guyen/.ssh/google_compute_engine` | Private key for target host |
| FQDN / domain | `<domain>` | `34-158-51-8.nip.io` (temporary) | TLS domain for reverse proxy; replace with real domain for production |
| HTTP port | `<http-port>` | 19080 | ferrumd HTTP bind port (localhost only) |
| HTTPS port (proxy) | `<https-port>` | 443 | Reverse proxy frontend |
| Network zone | `<network-zone>` | GCP custom VPC `ferrumgate-nonprod-vpc`, zone `asia-southeast1-a` | e.g., DMZ, internal |
| Firewall required | [x] Yes  [ ] No | SSH (22) and app (19080) from `118.69.4.63/32`; HTTP (80) and HTTPS (443) public | External firewall controls |

### Firewall / Network Checklist

| Check | Status | Notes |
|-------|--------|-------|
| ferrumd port 8080 not directly exposed to internet | [x] Confirmed | ferrumgate binds localhost only (127.0.0.1:19080); accessed via Caddy reverse proxy |
| Health endpoints (`/v1/healthz`, `/v1/readyz`) reachable through proxy | [x] Confirmed | Unauthenticated; via Caddy on 443 |
| All other endpoints require bearer auth | [x] Confirmed | Auth required |
| Outbound allowed for git adapter | [ ] Not confirmed | If git remote used |
| Outbound allowed for HTTP adapter | [ ] Not confirmed | If external APIs used |

---

## Section 2 — ferrumd Service

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| ferrumd binary path | `<ferrumd-binary>` | _________________ | e.g., `/usr/local/bin/ferrumd` |
| ferrumd config path | `<ferrumd-config>` | _________________ | Adapted from `configs/examples/nonprod-ferrumgate.toml` for Path 2 target practice, or `configs/ferrumgate.prod.toml` if mirroring production posture |
| ferrumd service name | `<ferrumd-service-name>` | _________________ | e.g., `ferrumd` |
| ferrumd startup method | [ ] systemd  [ ] manual  [ ] container | _________________ | Service management |
| ferrumd log path | `<ferrumd-log-path>` | _________________ | For evidence capture |

### Service Configuration Checklist

| Check | Status | Notes |
|-------|--------|-------|
| `ferrumd` binary available on target host | [ ] Confirmed | |
| Config file exists and readable by ferrumd service | [ ] Confirmed | |
| Service starts successfully | [ ] Confirmed | |
| Service survives restart | [ ] Confirmed | |
| Logs readable for evidence capture | [ ] Confirmed | |

---

## Section 3 — Storage

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Store type | `sqlite` | `sqlite` | PostgreSQL NOT implemented |
| Store DSN / path | `<store-path>` | `/var/lib/ferrumgate/data/ferrumgate.db` | e.g., `/var/lib/ferrumgate/ferrumgate.db` |
| Store parent directory | `<store-dir>` | `/var/lib/ferrumgate/data` | Must exist and be writable |
| Store file owner | `<store-owner>` | `ferrumgate` | Owner of store file |
| Store file permissions | `<store-perms>` | `0600` | e.g., `0600` |

### Storage Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Store parent directory exists | [x] Confirmed | |
| Store file path writable by ferrumd | [x] Confirmed | |
| SQLite WAL mode enabled (default) | [x] Confirmed | Default setting |
| `PRAGMA integrity_check` passes on existing store | [x] Confirmed | Passed on restore drill |

---

## Section 4 — Backup

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Backup base directory | `<backup-dir>` | `/var/lib/ferrumgate/backups` | e.g., `/var/backups/ferrumgate` |
| Backup tool | `ferrumctl backup create` | `ferrumctl backup create` | Built-in backup command |
| Backup schedule method | [ ] cron  [x] systemd timer  [ ] CI  [ ] manual | `ferrumgate-backup.timer` | External scheduling |
| Backup schedule | `<backup-schedule>` | `OnUnitActiveSec=15min` | 15-minute interval |
| Backup retention | `<backup-retention>` | 7 days + offsite copy | e.g., `7` for 7 daily snapshots |
| Backup owner | `<backup-owner>` | `ferrumgate` | Owner of backup files |

### Backup Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Backup directory exists | [x] Confirmed | |
| Backup directory writable | [x] Confirmed | |
| `ferrumctl backup create` runs successfully | [x] Confirmed | Manual test passed |
| `ferrumctl backup verify` passes after backup | [x] Confirmed | |
| Scheduler executes backup on schedule | [x] Confirmed | Timer `enabled` and `active` |
| Retention policy enforced | [x] Confirmed | 7 days + offsite copy required before production |

---

## Section 5 — Authentication and Transport Security

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| auth_mode | `bearer` | `bearer` | Do NOT use `disabled` on non-loopback |
| Bearer token source | [x] env var  [ ] config file | env var | Do NOT hardcode real tokens |
| Bearer token env var name | `FERRUMD_BEARER_TOKEN` | `FERRUMD_BEARER_TOKEN` | Environment variable name |
| Bearer token generation | `openssl rand -hex 32` | Generated on-VM during bootstrap | Command to generate token |
| TLS termination | Reverse proxy (external) | Caddy (external) | NOT handled by ferrumd |
| TLS cert path | `<tls-cert-path>` | Let's Encrypt via Caddy automatic | e.g., `/etc/ssl/certs/ferrumgate.crt` |
| TLS key path | `<tls-key-path>` | Caddy-managed | e.g., `/etc/ssl/private/ferrumgate.key` |
| TLS version minimum | TLS 1.2 | TLS 1.2+ | |

### Auth / TLS Checklist

| Check | Status | Notes |
|-------|--------|-------|
| `auth_mode = "bearer"` in config | [x] Confirmed | |
| Bearer token not hardcoded in config | [x] Confirmed | Use env var |
| Token generated with `openssl rand -hex 32` or equivalent | [x] Confirmed | |
| TLS terminates at reverse proxy (not ferrumd) | [x] Confirmed | Caddy on 443 |
| Health endpoints intentionally unauthenticated | [x] Confirmed | `/v1/healthz`, `/v1/readyz` |
| All other endpoints require auth | [x] Confirmed | |

---

## Section 6 — Reverse Proxy (External)

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Proxy type | [ ] nginx  [x] caddy  [ ] other | `caddy` | Caddy v2.11.2 |
| Proxy config path | `<proxy-config>` | `/etc/caddy/Caddyfile` | |
| Proxy upstream address | `<upstream-addr>` | `127.0.0.1:19080` | ferrumd bind (localhost) |
| Proxy config example | `configs/examples/nginx-ferrumgate.conf` | Reference | |

### Proxy Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Proxy configured for TLS 443 | [x] Confirmed | |
| `proxy_set_header Authorization $http_authorization` | [x] Confirmed | Caddy forwards Authorization header as-is |
| Health endpoints accessible through proxy | [x] Confirmed | Via Caddy on 443 |
| HTTP → HTTPS redirect configured | [x] Confirmed | Caddy HTTP-01 challenge on 80 |
| Certificate valid and not expired | [x] Confirmed | Let's Encrypt via Caddy |

**Production note**: Current TLS domain is `34-158-51-8.nip.io` (temporary). Replace with real domain before production deployment.

---

## Section 7 — Workload Model (G2.1)

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Expected sustained write rate | _____ writes/s | ≤300 writes/s | Must be ≤300 for Phase 1 |
| Expected peak write rate | _____ writes/s | ≤300 writes/s | |
| Expected daily write volume | _____ writes/day | ≤1M writes/day | |
| Execution history size at steady state | _____ records | Bounded by file size | Bounded by SQLite file size |

**Evidence**: Workload model accepted by BrianNguyen on 09/05/2026 (doc 99). Single-node SQLite confirmed fit.
| Workload fit for single-node SQLite | [ ] YES  [ ] NO | _________________ | If NO, defer to Path 3 |

---

## Section 8 — RPO / RTO (G2.5)

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Backup interval | _____ minutes/hours | _________________ | Frequency of `ferrumctl backup create` |
| RPO (Recovery Point Objective) | _____ minutes | _________________ | Time since last backup |
| Estimated restore time | _____ minutes | _________________ | `ferrumctl backup restore` duration |
| Estimated restart time | _____ minutes | _________________ | Service restart |
| Estimated verification time | _____ minutes | _________________ | `readyz/deep` probe |
| Total RTO | _____ minutes | _________________ | Sum of above |
| FerrumGate automated recovery | [ ] NO | Operator-driven only | |

### RPO/RTO Checklist

| Check | Status | Notes |
|-------|--------|-------|
| RPO acceptable for target workload SLA | [ ] YES  [ ] NO | |
| RTO acceptable for target workload SLA | [ ] YES  [ ] NO | |
| Backup retention covers RPO | [ ] Confirmed | |

---

## Section 9 — Operators and Ownership

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Primary operator | `<operator-name>` | _________________ | Owner of pilot execution |
| Backup operator | `<backup-operator>` | _________________ | Secondary contact |
| Engineering lead | `<eng-lead>` | _________________ | For Phase 3 decisions |
| Pilot start date | `<pilot-start>` | _________________ | Planned start |
| Evidence directory | `<evidence-dir>` | _________________ | For drill logs and evidence |

---

## Section 10 — Evidence Paths (Operator-Filled After Drills)

| Evidence | Path | Status |
|----------|------|--------|
| Adapted non-prod config | `<ferrumd-config>` | ☐ Operator pending |
| Restore drill log | `<evidence-dir>/restore_drill_<date>.log` | ☐ Operator pending |
| D1-D6 drill logs | `<evidence-dir>/d{1,2,3,4,5,6}_drill_output.txt` | ☐ Operator pending |
| Evidence skeleton | `<evidence-dir>/d1_d6_skeleton.md` | ☐ Operator pending |
| G2 readiness skeleton | `<evidence-dir>/g2_skeleton.md` | ☐ Operator pending |
| Backup scheduler config | External (cron/systemd) | ☐ Operator pending |
| TLS proxy config | External (proxy config) | ☐ Operator pending |

---

## Section 11 — Stop Conditions

Complete before beginning pilot. If any trigger fires, abort pilot preparation.

| Trigger | Action |
|---------|--------|
| Sustained write rate >300 writes/s | Abort Path 2; proceed to Path 3 |
| Any G2 signoff item declined | Abort Path 2; resolve or formally accept risk |
| Compensate noop risk unacceptable for target adapters | Abort Path 2; adapter implementation required |
| `ferrumctl backup verify` fails | Do not restore; take new backup; investigate |
| `readyz/deep` returns non-200 | Investigate; do not proceed |
| TLS not configured | Do not expose non-loopback without TLS |

---

## Section 12 — Readiness Checklist

Complete all items before beginning pilot execution per `61-path-2-execution-plan.md`.

### Pre-Flight

| Check | Status | Notes |
|-------|--------|-------|
| Target host accessible via SSH | [ ] Done | |
| ferrumd binary available on target | [ ] Done | |
| Config file adapted and deployed | [ ] Done | |
| Store path exists and is writable | [ ] Done | |
| Backup directory exists and is writable | [ ] Done | |
| Bearer token generated and secured | [ ] Done | Not hardcoded |
| Reverse proxy configured | [ ] Done | |
| TLS certificates valid | [ ] Done | |

### Pilot Preparation

| Check | Status | Notes |
|-------|--------|-------|
| G2.1 Workload model confirmed fit | [ ] Done | |
| G2.2 Auth/TLS configured | [ ] Done | |
| G2.3 Backup scheduler operational | [ ] Done | |
| G2.4 Restore drill successful | [ ] Done | |
| G2.5 RPO/RTO accepted | [ ] Done | |
| G2.6 Production evaluation complete | [ ] Done | |
| G2.7 Accepted-risk review done | [ ] Done | |
| G2.8 Compensate noop accepted | [ ] Done | |

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `63-path-2-target-environment-spec.md` | [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Execution plan context |
| `63-path-2-target-environment-spec.md` | [`62-path-2-operator-runbook.md`](./62-path-2-operator-runbook.md) | Operator command sequences |
| `63-path-2-target-environment-spec.md` | [`64-local-staging-simulation-guide.md`](./64-local-staging-simulation-guide.md) | Option 3 local simulation |
| `63-path-2-target-environment-spec.md` | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence |
| `63-path-2-target-environment-spec.md` | [`58-workload-compensation-drill-evidence-template.md`](./58-workload-compensation-drill-evidence-template.md) | D1–D6 drill template |
| `63-path-2-target-environment-spec.md` | [`configs/examples/nonprod-ferrumgate.toml`](../../configs/examples/nonprod-ferrumgate.toml) | Non-prod config template |

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- This spec records operator-completed target environment details copied from signed doc 99
- G2 gates are signed for conditional single-node SQLite pilot scope only
- No full production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- All operator signoff gates require explicit operator action and signature

---

*Template created: 2026-04-30. Updated 2026-05-09 from signed doc 99 worksheet — conditional single-node SQLite pilot scope only; no full production-ready, no PostgreSQL start, no HA/multi-node claim.*
