# 99 — Phase 3E SQLite Pilot Evidence Plan

## Overview

Phase 3E consolidates evidence collection procedures for the signed conditional single-node SQLite pilot on the active GCP non-prod VM. This is **NOT production-ready**, **NOT full G2 beyond conditional signed scope**, **NOT full production pilot authorization**, and **NOT Phase 3E operator signoff**.

This document provides a read-only evidence gathering script for bounded pilot evidence collection on the existing GCP non-prod deployment. No GCP configuration mutations are performed.

---

## Non-Claims (Phase 3E)

> **IMPORTANT**: Phase 3E carries the following explicit non-claims:
> - NOT production-ready
> - NOT full G2 beyond conditional single-node SQLite pilot scope
> - NOT full production pilot authorization
> - NOT operator signoff
> - NOT suitable for production workloads
> - NOT a full production posture claim
> - NOT PostgreSQL/multi-node/HA validated
> - Conditional single-node SQLite pilot evidence only
> - Canonical docs 54/59/63/65 record conditional pilot signoff only; production signoff remains out of scope

> Phase 3E is operator-owned GCP non-prod evidence gathering only.

---

## Scope

Phase 3E covers evidence gathering for a **conditional single-node SQLite pilot** on the existing GCP non-prod VM (`ferrumgate-nonprod`).

### What Phase 3E IS

- Evidence collection for a conditional single-node SQLite pilot
- Read-only checks against the existing GCP non-prod deployment
- Bounded evidence gathering that does not modify VM, firewall, Caddy, systemd, or any GCP resource
- Consolidation of Phase 3C/3D health/auth/backup evidence with pilot-specific checks

### What Phase 3E is NOT

- No GCP VM/firewall/Caddy/systemd mutations
- No production-ready claim
- No PostgreSQL/multi-node/HA evidence
- No full production posture claim
- No bearer token exposure or persistence

---

## Target Environment

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` |
| HTTPS URL | `https://34-158-51-8.nip.io` |
| App Port | `19080` (localhost only, behind Caddy) |
| TLS Terminator | Caddy v2.11.2 |
| Database | SQLite single-node (no PostgreSQL) |
| Auth Mode | Bearer token |

---

## Phase 3E Relationship to Phase 3A/3B/3C/3D

| Phase | Scope | Key Script |
|-------|-------|------------|
| 3A | GCP VM create/destroy, binary deploy, bootstrap | `phase3a_create/destroy/deploy` |
| 3B | TLS termination (nip.io + Caddy) | `phase3b_configure/destroy_tls_caddy` |
| 3C | Live rehearsal, health/auth checks, monitoring | `phase3c_live_rehearsal` |
| 3D | G2 readiness checklist mapping | `98-phase3d-g2-readiness-checklist.md` |
| 3E | SQLite pilot evidence gathering (this phase) | `phase3e_sqlite_pilot_evidence.sh` |

Phase 3E does not replace Phase 3A, 3B, 3C, or 3D. It provides bounded evidence gathering for a conditional single-node SQLite pilot on the existing GCP non-prod deployment.

---

## Evidence Gathering Script

The primary Phase 3E tool is `scripts/gcp/phase3e_sqlite_pilot_evidence.sh`, a read-only evidence gathering script.

### Script Features (Read-Only)

| Check | Description | Requires --confirm |
|-------|-------------|-------------------|
| HTTPS probes | /v1/healthz, /v1/readyz, /v1/readyz/deep, /v1/metrics | No |
| Auth probes | 401 without token, 200 with token | No |
| Service status | caddy, ferrumgate.service, ferrumgate-backup.timer | No |
| Firewall summary | Read-only firewall rule listing | No |
| Backup timer status | Next scheduled backup run | No |
| Token availability check | Confirm VM-local token exists; never print token or prefix | No |
| SQLite health | Store health metric, write queue depth | No |
| Backup file check | List backup files (read-only) | No |

### Usage

```bash
# Read-only evidence gathering (default)
bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh

# With explicit GCP parameters
bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --tls-domain 34-158-51-8.nip.io \
  --app-port 19080
```

### Script Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All evidence checks passed |
| 1 | One or more checks failed |

### What the Script Does NOT Do

- Never prints the full bearer token
- Never commits any secret to the repository
- Never modifies GCP resources (firewall rules, VM config)
- Never restarts services or modifies systemd units
- Never creates or deletes backup files
- Never claims production readiness or G2 completion

---

## Pilot Evidence Focus Areas

### 1. SQLite Single-Node Health

Evidence that SQLite single-node is functioning correctly in the GCP non-prod environment:

| Check | Expected | Evidence |
|-------|----------|----------|
| Store health metric | `ferrumgate_store_health_up=1` | GCP non-prod confirmed |
| Write queue depth | `ferrumgate_write_queue_depth=0` (empty) | Normal |
| Backup file exists | `ferrumgate_*.db` in backup dir | Confirmed |
| Database integrity | `PRAGMA integrity_check` passes | Per Phase 3D evidence |

### 2. Bearer Auth + TLS (from Phase 3C/3D)

| Check | Expected | Evidence |
|-------|----------|----------|
| TLS termination | Caddy reverse proxy | Active |
| TLS domain | nip.io (temporary) | `34-158-51-8.nip.io` |
| Auth: no token | 401 | Confirmed |
| Auth: with token | 200 | Confirmed |
| Firewall: SSH 22 | From allowlist only | Confirmed |

### 3. Backup/Restore Evidence (from Phase 3D)

| Check | Expected | Evidence |
|-------|----------|----------|
| Backup timer | Enabled | Confirmed |
| Manual backup trigger | Success | Confirmed |
| Restore drill | `PRAGMA integrity_check=ok` | Phase 3D passed |
| RPO acceptance | Operator-defined | Pending |

### 4. Conditional Single-Node SQLite Pilot Constraints

This evidence is for a **conditional single-node SQLite pilot only**:

- No PostgreSQL evidence gathered or claimed
- No multi-node/HA evidence gathered or claimed
- No production-ready claim made
- nip.io is temporary and not suitable for production
- Any future full production posture requires proper domain and DNS

---

## Manual Runbook (Alternative to Script)

When gathering evidence manually, use this runbook.

### Pre-Flight: Verify VM Is Running

```bash
gcloud compute instances describe ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 \
    --format='value(name,status,networkInterfaces[0].accessConfigs[0].natIP)'
```

Expected: `ferrumgate-nonprod  RUNNING  34.158.51.8`

### Health/Readiness/Metrics Probes

```bash
# HTTPS health
curl -s -o /dev/null -w "%{http_code}" https://34-158-51-8.nip.io/v1/healthz
# Expected: 200

# HTTPS readyz
curl -s -o /dev/null -w "%{http_code}" https://34-158-51-8.nip.io/v1/readyz
# Expected: 200

# HTTPS deep readyz
curl -s -o /dev/null -w "%{http_code}" https://34-158-51-8.nip.io/v1/readyz/deep
# Expected: 200

# HTTPS metrics (no auth required)
curl -s -o /dev/null -w "%{http_code}" https://34-158-51-8.nip.io/v1/metrics
# Expected: 200
```

### Auth Probes

```bash
# Without bearer token (expect 401)
curl -s -o /dev/null -w "%{http_code}" https://34-158-51-8.nip.io/v1/approvals
# Expected: 401

# With bearer token (retrieve from VM first, use only in-memory)
# On VM:
#   sudo cat /etc/ferrumgate/ferrumgate_initial_token
# Use token only for immediate probe, never persist
```

**Security note**: The full bearer token must never be printed to logs or committed to the repository.

### Service Status Checks

```bash
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    "sudo systemctl is-active caddy && \
     sudo systemctl is-active ferrumgate.service && \
     sudo systemctl is-enabled ferrumgate-backup.timer"
```

Expected output: `active\nactive\nenabled`

### Metrics Snapshot

```bash
curl -s https://34-158-51-8.nip.io/v1/metrics
```

Key metrics to capture:
- `ferrumgate_store_health_up`
- `ferrumgate_write_queue_depth`
- Request counts for healthz, readyz, readyz/deep, metrics

### Backup File Check (Read-Only)

```bash
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    "ls -t /var/lib/ferrumgate/backups/ | head -5"
```

Expected: List of `ferrumgate_*.db` backup files.

---

## Token Security Protocol

1. **Retrieve token on VM only** via `sudo cat /etc/ferrumgate/ferrumgate_initial_token`
2. **Never print the full token or token prefix** — scripts only confirm token file presence
3. **Never commit token** to any repository or log
4. **Use token in-memory only** for immediate curl probes
5. **Token is root-protected** on VM (`/etc/ferrumgate/ferrumgate_initial_token` mode 600)

---

## Evidence Collection for Artifact

When collecting evidence for the Phase 3E artifact:

1. Run `phase3e_sqlite_pilot_evidence.sh`
2. Capture all output
3. Record the exact timestamp of the run
4. Note any deviations from expected statuses
5. Do not claim this evidence constitutes production readiness, PostgreSQL/HA readiness, or full production pilot authorization

---

## Troubleshooting

### HTTPS probes return 000 or connection refused

- Verify Caddy is running: `sudo systemctl is-active caddy`
- Verify ferrumgate is running: `sudo systemctl is-active ferrumgate.service`
- Check Caddy logs: `sudo journalctl -u caddy -n 20 --no-pager`
- Check ferrumgate logs: `sudo journalctl -u ferrumgate.service -n 20 --no-pager`

### Metrics show store health down

- Check ferrumgate service is running
- Check SQLite database file exists: `ls -la /var/lib/ferrumgate/ferrumgate.db`
- Check disk space on VM

### Backup timer not enabled

- This is an operator configuration issue, not a Phase 3E script issue
- Re-enable: `sudo systemctl enable ferrumgate-backup.timer`

---

## References

- Phase 3A plan: [94-gcp-compute-phase3a-nonprod-target-plan.md](./94-gcp-compute-phase3a-nonprod-target-plan.md)
- Phase 3A artifact: [artifacts/2026-05-08-gcp-phase3a-nonprod-target.md](./artifacts/2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B plan: [95-gcp-compute-phase3b-domain-tls-plan.md](./95-gcp-compute-phase3b-domain-tls-plan.md)
- Phase 3B artifact: [artifacts/2026-05-08-gcp-phase3b-domain-tls.md](./artifacts/2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C live ops packet: [96-gcp-compute-phase3c-live-ops-packet.md](./96-gcp-compute-phase3c-live-ops-packet.md)
- Phase 3C artifact: [artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md](./artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D G2 readiness: [98-phase3d-g2-readiness-checklist.md](./98-phase3d-g2-readiness-checklist.md)
- Phase 3D artifact: [artifacts/2026-05-08-gcp-phase3d-g2-readiness.md](./artifacts/2026-05-08-gcp-phase3d-g2-readiness.md)
- Operator signoff packet: [54-operator-signoff-packet.md](./54-operator-signoff-packet.md)
- Pilot readiness evidence: [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md)

---

## Document History

| Date | Change |
|---|---|
| 2026-05-09 | Initial Phase 3E SQLite pilot evidence plan. Evidence-only. Operator-owned. |
