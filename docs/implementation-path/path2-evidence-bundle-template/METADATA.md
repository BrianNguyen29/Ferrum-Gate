# METADATA — Required Operator-Filled Fields

> **Status**: TEMPLATE ONLY — NOT EVIDENCE. Operator must fill these fields before use.
> **Purpose**: Documents required metadata that operator must provide for a real target run.
> **Constraint**: Do NOT use placeholder/placeholder values as real target values.

---

## Operator Identity

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Operator name | Yes | _______________________________ |
| Operator role | Yes | _______________________________ |
| Operator email | Yes | _______________________________ |
| Date of target run | Yes | _______________________________ |

---

## Target Environment

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Target host FQDN | Yes | _______________________________ |
| Target host IP | Yes | _______________________________ |
| SSH user | Yes | _______________________________ |
| SSH key path | Yes | _______________________________ |
| OS / distribution | Yes | _______________________________ |
| Firewall ports (80/443/8080) | Yes | _______________________________ |

---

## Storage

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Store path (SQLite db) | Yes | _______________________________ |
| Backup directory | Yes | _______________________________ |
| Store DSN | Yes | _______________________________ |

---

## Authentication

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Auth mode | Yes | bearer |
| Bearer token | Yes | [GENERATE: `openssl rand -hex 32`] |
| Token env var | Yes | FERRUMD_BEARER_TOKEN |
| Config file path | Yes | _______________________________ |

---

## Network / TLS

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Bind address | Yes | _______________________________ |
| Domain | Yes | _______________________________ |
| TLS certificate path | Yes | _______________________________ |
| TLS private key path | Yes | _______________________________ |
| Reverse proxy | Yes | nginx / other |

---

## Backup / Restore

| Field | Required | Operator Fill |
|-------|----------|---------------|
| Backup schedule | Yes | cron / systemd / manual |
| Backup retention | Yes | _______________________________ |
| RPO target | Yes | _______________________________ |
| RTO target | Yes | _______________________________ |

---

## Run Metadata

| Field | Required | Operator Fill |
|-------|----------|---------------|
| ferrumd version | Yes | _______________________________ |
| ferrumd commit | Yes | _______________________________ |
| Evidence collection date | Yes | _______________________________ |
| Evidence collected by | Yes | _______________________________ |
| Target run directory | Yes | _______________________________ |

---

## Explicit Attestation

> **Operator Attestation**: I confirm all fields above contain real operator-provided values specific to the target deployment environment, and not placeholder or dummy values.

Operator signature: _________________ Date: _________

---

*Template field: 2026-05-06. Required metadata for evidence bundle — NOT EVIDENCE itself.*
