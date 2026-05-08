# 71 — Path 2 Target Values Intake Packet

> **Status**: Documentation-only. Intake artifact.
> **Purpose**: Concise operator target-values checklist, synthesized from docs 63 and 65.
> **Scope**: Single-node SQLite Path 2 only. Not evidence of a target run.
> **Constraint**: Do not fill real secrets here. Do not claim G2, pilot, or production readiness.

---

## Purpose

Use this packet to collect the real target values needed before executing Path 2.
It is intentionally shorter than the full questionnaire/spec and groups required inputs by:

- severity: Critical / High / Medium
- owner: operator-provided / operator-generated / derived during deploy
- blocking effect: deployment-blocking vs evidence-hardening

Complete all **Critical** and **High** fields before target deployment begins. Values marked
**Derived During Deploy** must be captured from the real target run, not invented in advance.

---

## Severity Key

| Severity | Meaning |
| --- | --- |
| **Critical** | Deployment blocks without this value. |
| **High** | Required for full target evidence / G2 evaluation. |
| **Medium** | Required for hardening, observability, or operator review completeness. |

---

## Critical — Operator Provides Before Deployment

| Field | Source | Owner | Notes |
| --- | --- | --- | --- |
| Operator name + role | doc 65 §A | Operator | Required for accountability and later signoff. |
| Operator contact/email | doc 65 §A | Operator | Required for handoff loop. |
| Target host FQDN or IP | doc 65 §B | Infra/operator | Must be real target, not local dummy. |
| SSH host/user/access method | doc 65 §B | Infra/operator | Include key path or bastion process if applicable. |
| Operating system + version | doc 65 §B | Infra/operator | Must support systemd for provided examples. |
| systemd available | doc 65 §B | Infra/operator | Required for `ferrumd.service` path. |
| Service user/group | doc 63 §Service | Infra/operator | e.g. `ferrumgate`; do not assume. |
| Install directory | doc 63 §Service | Infra/operator | Target path for binary/config ownership. |
| Config file path | docs 63/65 | Operator | Example: `/etc/ferrumgate/ferrumgate.toml`. |
| SQLite store path | docs 63/65 | Operator | Must be writable by service user. |
| SQLite store parent directory | docs 63/65 | Operator | Include ownership and permissions. |
| `bind_addr` | docs 63/65 | Operator | Recommended local bind behind TLS proxy. |
| `auth_mode` | docs 63/65 | Operator/security | Must be bearer for target/prod-like run. |
| Bearer token storage path | docs 63/65 | Security/operator | Example env file or secrets manager path. |
| Backup output directory | docs 63/65 | Operator | Required before backup/restore evidence. |
| Evidence output directory | docs 63/65 | Operator | Where target-run artifacts will be stored. |

---

## Critical — Operator Generates Before Deployment

| Field | Source | Owner | Required Handling |
| --- | --- | --- | --- |
| Bearer token | docs 63/65 | Security/operator | Generate with `openssl rand -hex 32`; never commit. |
| Token file permissions | docs 63/65 | Security/operator | Record owner/mode, e.g. service-readable only. |
| Initial config file | docs 63/65 | Operator | Adapt examples to target values. |
| systemd unit installation | docs 62/66 | Operator | Install/reload/enable/start on target. |

---

## High — Operator Provides Before Target Evidence

| Field | Source | Owner | Notes |
| --- | --- | --- | --- |
| Public domain | doc 65 §C | Infra/operator | Must match TLS certificate SAN/CN. |
| DNS A/AAAA record status | doc 65 §C | Infra/operator | Required for external HTTPS probe. |
| TLS certificate path | docs 63/65 | Security/operator | Existing cert or certbot output. |
| TLS private key path | docs 63/65 | Security/operator | Do not copy into docs. |
| TLS expiry date | docs 63/65 | Security/operator | Record for operator review. |
| nginx installed | doc 65 §H | Operator | Required for provided proxy model. |
| nginx config path | docs 63/65 | Operator | Include target file path. |
| nginx upstream address | docs 63/65 | Operator | Should match ferrumd local bind. |
| Authorization header forwarding | docs 63/65 | Operator | Must forward `$http_authorization`, not hardcode token. |
| HTTP-to-HTTPS redirect | docs 63/65 | Operator | Required for public endpoint hardening. |
| Backup schedule mechanism | docs 63/65 | Operator | cron or systemd timer. |
| Backup retention policy | docs 63/65 | Operator | Days/count plus disk capacity owner. |
| RPO target | docs 63/65 | Operator/business | Required for pilot acceptance. |
| RTO target | docs 63/65 | Operator/business | Required for restore acceptance. |
| Workload model | docs 63/65 | Product/operator | Expected and peak writes/sec; SQLite ceiling check. |
| Monitoring owner/contact | docs 63/65 | Operator | Who receives alerts. |

---

## High — Derived During Deploy / Evidence Capture

| Field | Source | Capture Command / Evidence |
| --- | --- | --- |
| `ferrumd.service` status | doc 62 | `systemctl status ferrumd` / journal snippet. |
| Startup logs | doc 62 | `journalctl -u ferrumd -n 50`. |
| `/v1/healthz` status | docs 62/65 | Local and external probe output. |
| `/v1/readyz` status | docs 62/65 | Local and external probe output. |
| `/v1/readyz/deep` status | docs 62/65 | Must reflect store health. |
| `/v1/metrics` output | docs 62/65 | Capture store health + counters. |
| Auth smoke result | docs 62/66 | no token = 401, wrong token = 401, correct token = expected success. |
| D1–D6 target drill outputs | docs 58/62 | Must be run on/against target context. |
| Backup create/verify evidence | docs 62/66 | Include command, exit code, artifact path. |
| Restore drill evidence | docs 62/66 | Include integrity check and restored DB path. |
| G2 evidence packet updates | doc 59 | Only after real target evidence exists. |
| Operator signoff | doc 54 | Only after G2 packet review. |

---

## Medium — Hardening / Review Inputs

| Field | Source | Owner | Notes |
| --- | --- | --- | --- |
| Network zone | doc 65 §B | Infra/operator | DMZ/internal/VPC/subnet. |
| Firewall rules | doc 65 §B | Infra/operator | Ports 80/443/8080 reachability. |
| Package manager | doc 65 §B | Infra/operator | apt/yum/dnf/etc. |
| Prometheus availability | doc 65 §E | Observability/operator | If absent, note manual metrics capture. |
| AlertManager/Grafana availability | doc 65 §E | Observability/operator | Optional but recommended. |
| Log retention policy | docs 63/65 | Operator | journald/log aggregation retention. |
| Token rotation owner/process | doc 70 | Security/operator | Local/manual process acceptable for pilot if documented. |
| Incident contact | docs 63/65 | Operator | Required before pilot operation. |

---

## Pre-Deployment Intake Checklist

- [ ] All Critical fields collected from real operator/infra source.
- [ ] Bearer token generated outside repo and never committed.
- [ ] Target host access confirmed.
- [ ] SQLite store path and backup path confirmed writable.
- [ ] TLS/domain/proxy plan confirmed.
- [ ] Workload model confirms single-node SQLite is acceptable.
- [ ] RPO/RTO targets agreed.
- [ ] Evidence output directory agreed.
- [ ] Operator understands docs 54 and 59 cannot be signed until real evidence exists.

---

## Explicit Non-Claims

- This packet is not G2 evidence.
- This packet is not operator signoff.
- This packet does not authorize a production pilot.
- This packet does not make FerrumGate production-ready.
- Dummy/local values must not be copied into docs 54, 59, 63, or 65 as real values.

FerrumGate v1 remains **RC-ready / conditional single-node SQLite** until target evidence and operator signoff exist.
