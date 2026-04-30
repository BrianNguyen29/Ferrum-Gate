# 63 — Path 2 Target Environment Specification

> **Status**: Documentation-only. Operator-owned template.
> **Purpose**: Structured spec template for capturing target non-prod/production environment details required for Path 2 pilot preparation.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. Not production-ready.
> **Constraint**: Do not sign doc 54, do not claim G2 complete, do not start PostgreSQL, do not add real secrets.

---

## Purpose

This document provides a fillable template for capturing the target non-prod or production-like
environment specification before beginning Path 2 pilot preparation. It is a **pre-exercise worksheet**
— completing it does not execute any deployment, does not claim G2 complete, and does not authorize
any production pilot.

**Operator-owned**: All fields require operator verification and population. Do not fill on behalf of the operator.

---

## Environment Classification

| Field | Value | Notes |
|-------|-------|-------|
| Environment type | [ ] Non-prod  [ ] Staging  [ ] Prod-like  [ ] Production | Select one |
| Path 2 preparation mode | [ ] Option 2 / target environment spec  [ ] Option 3 / local-only simulation | Option 2 is required before target G2 evidence; Option 3 is practice only |
| Pilot scope | Single-node SQLite only | PostgreSQL blocked until Phase 3 |

**Explicit Non-Claims**:
- No G2 complete claim
- No production-ready claim
- No PostgreSQL start
- No operator signature pre-populated

---

## Section 1 — Host and Network

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Target host / IP | `<target-host>` | _________________ | Non-prod host for ferrumd |
| SSH host | `<ssh-host>` | _________________ | For remote management |
| SSH user | `<ssh-user>` | _________________ | Operator account on target |
| SSH key path | `<ssh-key-path>` | _________________ | Private key for target host |
| FQDN / domain | `<domain>` | _________________ | TLS domain for reverse proxy |
| HTTP port | `<http-port>` | 8080 | ferrumd HTTP bind port |
| HTTPS port (proxy) | `<https-port>` | 443 | Reverse proxy frontend |
| Network zone | `<network-zone>` | _________________ | e.g., DMZ, internal |
| Firewall required | [ ] Yes  [ ] No | _________________ | External firewall controls |

### Firewall / Network Checklist

| Check | Status | Notes |
|-------|--------|-------|
| ferrumd port 8080 not directly exposed to internet | [ ] Confirmed | Use reverse proxy |
| Health endpoints (`/v1/healthz`, `/v1/readyz`) reachable through proxy | [ ] Confirmed | Unauthenticated |
| All other endpoints require bearer auth | [ ] Confirmed | Auth required |
| Outbound allowed for git adapter | [ ] Confirmed | If git remote used |
| Outbound allowed for HTTP adapter | [ ] Confirmed | If external APIs used |

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
| Store DSN / path | `<store-path>` | _________________ | e.g., `/var/lib/ferrumgate/ferrumgate.db` |
| Store parent directory | `<store-dir>` | _________________ | Must exist and be writable |
| Store file owner | `<store-owner>` | _________________ | Owner of store file |
| Store file permissions | `<store-perms>` | _________________ | e.g., `0600` |

### Storage Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Store parent directory exists | [ ] Confirmed | |
| Store file path writable by ferrumd | [ ] Confirmed | |
| SQLite WAL mode enabled (default) | [ ] Confirmed | |
| `PRAGMA integrity_check` passes on existing store | [ ] Confirmed | If store exists |

---

## Section 4 — Backup

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Backup base directory | `<backup-dir>` | _________________ | e.g., `/var/backups/ferrumgate` |
| Backup tool | `ferrumctl backup create` | _________________ | Built-in backup command |
| Backup schedule method | [ ] cron  [ ] systemd timer  [ ] CI  [ ] manual | _________________ | External scheduling |
| Backup schedule | `<backup-schedule>` | _________________ | e.g., `0 2 * * *` for daily 2am |
| Backup retention | `<backup-retention>` | _________________ | e.g., `7` for 7 daily snapshots |
| Backup owner | `<backup-owner>` | _________________ | Owner of backup files |

### Backup Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Backup directory exists | [ ] Confirmed | |
| Backup directory writable | [ ] Confirmed | |
| `ferrumctl backup create` runs successfully | [ ] Confirmed | Manual test |
| `ferrumctl backup verify` passes after backup | [ ] Confirmed | |
| Scheduler executes backup on schedule | [ ] Confirmed | Or scheduled |
| Retention policy enforced | [ ] Confirmed | Old backups pruned |

---

## Section 5 — Authentication and Transport Security

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| auth_mode | `bearer` | `bearer` | Do NOT use `disabled` on non-loopback |
| Bearer token source | [ ] env var  [ ] config file | _________________ | Do NOT hardcode real tokens |
| Bearer token env var name | `FERRUMD_BEARER_TOKEN` | _________________ | Environment variable name |
| Bearer token generation | `openssl rand -hex 32` | _________________ | Command to generate token |
| TLS termination | Reverse proxy (external) | _________________ | NOT handled by ferrumd |
| TLS cert path | `<tls-cert-path>` | _________________ | e.g., `/etc/ssl/certs/ferrumgate.crt` |
| TLS key path | `<tls-key-path>` | _________________ | e.g., `/etc/ssl/private/ferrumgate.key` |
| TLS version minimum | TLS 1.2 | _________________ | |

### Auth / TLS Checklist

| Check | Status | Notes |
|-------|--------|-------|
| `auth_mode = "bearer"` in config | [ ] Confirmed | |
| Bearer token not hardcoded in config | [ ] Confirmed | Use env var |
| Token generated with `openssl rand -hex 32` or equivalent | [ ] Confirmed | |
| TLS terminates at reverse proxy (not ferrumd) | [ ] Confirmed | |
| Health endpoints intentionally unauthenticated | [ ] Confirmed | `/v1/healthz`, `/v1/readyz` |
| All other endpoints require auth | [ ] Confirmed | |

---

## Section 6 — Reverse Proxy (External)

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Proxy type | [ ] nginx  [ ] caddy  [ ] other: _____ | _________________ | |
| Proxy config path | `<proxy-config>` | _________________ | |
| Proxy upstream address | `<upstream-addr>` | `127.0.0.1:8080` | ferrumd bind |
| Proxy config example | `configs/examples/nginx-ferrumgate.conf` | Reference | |

### Proxy Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Proxy configured for TLS 443 | [ ] Confirmed | |
| `proxy_set_header Authorization "Bearer $http_authorization"` | [ ] Confirmed | Token forwarding |
| Health endpoints accessible through proxy | [ ] Confirmed | |
| HTTP → HTTPS redirect configured | [ ] Confirmed | |
| Certificate valid and not expired | [ ] Confirmed | |

---

## Section 7 — Workload Model (G2.1)

| Field | Placeholder | Operator Fills | Notes |
|-------|-------------|----------------|-------|
| Expected sustained write rate | _____ writes/s | _________________ | Must be ≤300 for Phase 1 |
| Expected peak write rate | _____ writes/s | _________________ | |
| Expected daily write volume | _____ writes/day | _________________ | |
| Execution history size at steady state | _____ records | _________________ | Bounded by file size |
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

- This spec is a worksheet for operator-completed environment details
- No G2 complete claim is made by filling this template
- No production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- All operator signoff gates require explicit operator action and signature

---

*Template created: 2026-04-30. Documentation-only worksheet — no G2 complete, no production-ready, no PostgreSQL start, no operator signature pre-populated.*
