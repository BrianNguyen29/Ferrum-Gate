# Local-to-Real-Target Gap Checklist

> **Status**: Documentation-only. G2 bridging aid.
> **Purpose**: Map each LOCAL-TEST-GENERATED placeholder from [`local-nonprod-target-profile.md`](./local-nonprod-target-profile.md) to the exact real-target value or operator action required for actual Path 2 deployment.
> **Scope**: Single-node SQLite only. No production-ready claim.
> **Constraint**: Do not modify [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) or [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md). Do not commit real secrets. Do not claim G2/pilot/production readiness.

---

## ⚠️ IMPORTANT WARNINGS

**THIS IS NOT G2 EVIDENCE:**
- This checklist maps local placeholders to real-target values
- Completing this checklist does NOT complete any G2 gate
- Local profile (`local-nonprod-target-profile.md`) is for rehearsal only and is NOT G2 evidence
- Real G2 evidence requires target-environment execution per [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md)

**SECRETS MUST NOT BE COMMITTED:**
- All bearer tokens, SSH keys, and credentials must be handled out-of-band
- Use env vars or secrets managers; never hardcode in documentation

**NO PRODUCTION-READY CLAIM:**
- FerrumGate v1 is RC-ready/conditional for single-node SQLite only
- PostgreSQL/multi-node/HA are not implemented

---

## Provenance Legend

| Provenance | Meaning |
|------------|---------|
| **LOCAL PLACEHOLDER** | Artificial value from `local-nonprod-target-profile.md`; must be replaced |
| **OPERATOR PROVIDE** | Operator supplies from their infrastructure |
| **OPERATOR GENERATED** | Operator generates via command (e.g., `openssl rand -hex 32`) |
| **DERIVED DURING TARGET RUN** | Produced during target deployment and recorded post-run |

---

## Gap Mapping: Local Placeholder → Real Target

### 1. Network / Host

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `http://127.0.0.1:8080` | `http://<target-host>:<port>` or `https://<fqdn>` | OPERATOR PROVIDE | Determine target host IP/FQDN and port |
| `n/a-local-test` (SSH host) | `<ssh-host>` | OPERATOR PROVIDE | Obtain target host DNS name or IP for SSH |
| `local-dev` (SSH user) | `<ssh-user>` | OPERATOR PROVIDE | Identify dedicated deployment user on target |
| `n/a-local-test` (SSH key) | `<ssh-key-path>` | OPERATOR PROVIDE | Deploy private key to target-accessible path |

### 2. Domain / TLS

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `n/a-local-test` (domain) | `<fqdn>` with DNS A record | OPERATOR PROVIDE | Register domain; create DNS A record pointing to target |
| `n/a-local-test` (TLS cert) | `/etc/ssl/certs/<cert>.crt` or certbot path | OPERATOR PROVIDE | Obtain certificate from CA or certbot |
| `n/a-local-test` (TLS key) | `/etc/ssl/private/<key>.key` | OPERATOR PROVIDE | Store private key securely; set permissions 600 |

### 3. Storage

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `/tmp/ferrumgate-local-nonprod/ferrumgate.db` | `/var/lib/ferrumgate/ferrumgate.db` or operator-chosen path | OPERATOR PROVIDE | Choose persistent, writable path; create parent dir |
| `/tmp/ferrumgate-local-nonprod/backups` | `/var/backups/ferrumgate` or operator-chosen path | OPERATOR PROVIDE | Choose persistent, writable backup directory |
| `localhost only` (network) | `<network-zone>` (e.g., DMZ/internal) + firewall rules | OPERATOR PROVIDE | Document network zone; configure firewall to allow 80/443 to proxy |

### 4. Authentication

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `bearer` (auth mode) | `bearer` | OPERATOR PROVIDE | Confirm `auth_mode = "bearer"` in config; do NOT use `disabled` |
| Auto-generated locally per session | `openssl rand -hex 32` | OPERATOR GENERATED | Generate token out-of-band; store in env var `FERRUMD_BEARER_TOKEN`; never commit |
| Token env file path | `/etc/ferrumgate/ferrumd.env` or operator-chosen | OPERATOR PROVIDE | Create env file with token; set permissions 600 |

### 5. Scheduler / Operations

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `manual` (scheduler) | cron / systemd timer / CI | OPERATOR PROVIDE | Choose scheduling method; configure backup schedule |
| `n/a-local-test` (RPO/RTO) | Actual RPO/RTO based on workload SLA | OPERATOR PROVIDE | Define backup interval; calculate RPO/RTO; accept or adjust |

### 6. Operator / Evidence

| Local Placeholder | Real Target Required | Provenance | Action |
|-------------------|---------------------|------------|--------|
| `local-dev` (operator owner) | `<operator-name>` | OPERATOR PROVIDE | Identify primary operator for target |
| `/tmp/ferrumgate-local-nonprod/evidence` | `<evidence-dir>` on persistent storage | OPERATOR PROVIDE | Choose persistent evidence directory for drill logs |

---

## Quick Reference: What to Fill in Doc 63

After completing this gap checklist, transfer values to [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md):

| Doc 63 Section | Key Fields to Populate |
|----------------|----------------------|
| Section 1 — Host and Network | Target host/IP, SSH host, SSH user, SSH key path, FQDN, network zone |
| Section 2 — ferrumd Service | Config path, service name, startup method |
| Section 3 — Storage | Store DSN/path, store parent dir |
| Section 4 — Backup | Backup dir, schedule method, schedule, retention |
| Section 5 — Auth/TLS | Bearer token (env var), TLS cert/key paths |
| Section 6 — Reverse Proxy | Proxy type, upstream address |
| Section 8 — RPO/RTO | Backup interval, RPO, RTO |
| Section 9 — Operators | Primary operator, evidence directory |

---

## Ready to Bridge When

The following prerequisites must be satisfied before executing target-environment drills and claiming G2 evidence:

| # | Prerequisite | Evidence of Completion |
|---|-------------|----------------------|
| 1 | Target host accessible via SSH with dedicated user | SSH connection verified |
| 2 | ferrumd binary deployed to target host | Binary exists at chosen path |
| 3 | Config file adapted with real values (no hardcoded secrets) | Config validated; token via env var |
| 4 | Store path exists and is writable by ferrumd service | Write test successful |
| 5 | Backup directory exists and is writable | Write test successful |
| 6 | Bearer token generated and stored in env var (not in config) | Token generated; env var set |
| 7 | TLS certificates obtained and deployed | Certificate valid; not expired |
| 8 | Reverse proxy configured (nginx or equivalent) | Proxy config validated |
| 9 | [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) fully populated | All PROVIDE/OPERATOR-GENERATED fields filled |
| 10 | Local drills (auth smoke + restore drill) passed on local profile | Prior validation confirms tooling works |

---

## What This Checklist Does NOT Provide

| Gap | Why |
|-----|-----|
| G2 gate completion | Requires target-environment execution and operator signoff per docs 58/59 |
| Production-ready claim | FerrumGate v1 is RC-ready/conditional only |
| PostgreSQL support | Not implemented; blocked until Phase 3 |
| Automatic RPO/RTO | Operator must calculate and accept based on workload SLA |
| TLS certificate procurement | Operator must obtain from CA or certbot |

---

## Relationship to Other Docs

| Doc | Relationship |
|-----|--------------|
| [`local-nonprod-target-profile.md`](./local-nonprod-target-profile.md) | Source of local placeholders; do not use as G2 evidence |
| [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) | Real target spec to populate after completing this checklist |
| [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md) | Operator questionnaire; cross-reference for PROVIDE field context |
| [`64-local-staging-simulation-guide.md`](./64-local-staging-simulation-guide.md) | Local staging simulation; broader scope |
| [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Path 2 execution plan context |

---

## Disclaimer

**LOCAL-TEST ONLY BRIDGING CHECKLIST — NOT G2 EVIDENCE — NOT PRODUCTION READY**

- No G2 complete claim is made by completing this checklist
- No pilot accepted or production-ready claim is made
- This checklist only prepares for G2 bridging; actual G2 evidence requires target-environment execution
- FerrumGate v1 is RC-ready/conditional for single-node SQLite only
- PostgreSQL/multi-node/HA are not implemented

---

*Created: 2026-05-03. Documentation-only G2 bridging aid — no G2 claim, no production-ready claim.*
