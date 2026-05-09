# GCP Phase 3E SQLite Pilot Evidence Artifact

**Date**: 2026-05-09

**Scope**: Google Cloud Compute Engine Phase 3E SQLite pilot evidence gathering

**Status**: **NON-PROD evidence only**. Conditional single-node SQLite pilot.

## Non-Claims

This Phase 3E run does **not** claim:

- production-ready status
- full G2 beyond conditional single-node SQLite pilot scope
- full production pilot authorization
- Phase 3E operator signoff
- PostgreSQL/multi-node/HA readiness
- full production posture
- any full production canonical doc 54/59/63/65 signoff beyond conditional pilot scope

Phase 3E is evidence gathering for a **conditional single-node SQLite pilot** on the GCP non-prod VM rehearsal environment only.

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
| Database | SQLite single-node |

---

## Pre-Existing Infrastructure (Phase 3A + 3B + 3C + 3D)

This artifact assumes Phase 3A, 3B, 3C, and 3D have been previously executed:

- Phase 3A: GCP VM created, binaries deployed, ferrumgate service running, backup timer enabled
- Phase 3B: Caddy installed, TLS configured for nip.io domain, ferrumgate bind changed to localhost
- Phase 3C: Live rehearsal, health/auth checks, monitoring validated
- Phase 3D: G2 readiness checklist, restore drill, metrics snapshot

Reference artifacts:
- [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)

---

## Live Evidence

### Repo State

```
## main...origin/main
 M docs/implementation-path/README.md
?? docs/implementation-path/99-phase3e-sqlite-pilot-evidence-plan.md
?? docs/implementation-path/artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md
?? scripts/gcp/phase3e_sqlite_pilot_evidence.sh
```

### HTTPS Endpoint Statuses

| Endpoint | Expected | Observed |
|----------|----------|----------|
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz/deep` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | 200 |

### Metrics Snapshot

| Metric | Expected | Observed |
|--------|----------|----------|
| `ferrumgate_store_health_up` | 1 | 1 |
| `ferrumgate_write_queue_depth` | 0 | 0 |

### Auth Probe Results

| Probe | Expected | Observed |
|-------|----------|----------|
| `GET /v1/approvals` without token | 401 | 401 |
| `GET /v1/approvals` with VM-local bearer token | 200 | 200 |

Token handling: The script confirms VM-local bearer token availability without printing the token or token prefix. The full bearer token is retrieved on-VM via `sudo` and used only for the immediate auth probe. The full token is never printed to logs or committed.

### Service Statuses

| Service | Expected | Observed |
|---------|----------|----------|
| `caddy.service` | active | active |
| `ferrumgate.service` | active | active |
| `ferrumgate-backup.timer` | enabled | enabled |

Observed via `sudo systemctl is-active` and `sudo systemctl is-enabled` on VM.

### Backup Files (Read-Only Listing)

```
ferrumgate_20260508_154446.db
```

### Firewall Summary

| Rule | Port | Source | Observed |
|------|------|--------|----------|
| `ferrumgate-nonprod-fw-ssh` | TCP 22 | `118.69.4.63/32` | present |
| `ferrumgate-nonprod-fw-app` | TCP 19080 | `118.69.4.63/32` | present |
| `ferrumgate-nonprod-fw-http` | TCP 80 | `0.0.0.0/0` | present |
| `ferrumgate-nonprod-fw-https` | TCP 443 | `0.0.0.0/0` | present |

---

## Phase 3E Script Verification

The Phase 3E evidence script (`scripts/gcp/phase3e_sqlite_pilot_evidence.sh`) was validated with the following checks:

### Full Evidence Run

```bash
bash scripts/gcp/phase3e_sqlite_pilot_evidence.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --tls-domain 34-158-51-8.nip.io
```

Result summary:

- VM external IP: `34.158.51.8`
- VM reachable: yes
- VM-local bearer token: present, value not printed
- HTTPS endpoints: all expected statuses observed
- Metrics: `ferrumgate_store_health_up=1`, `ferrumgate_write_queue_depth=0`
- Auth probes: no-token `401`, VM-local bearer token `200`
- Services: `caddy.service=active`, `ferrumgate.service=active`, `ferrumgate-backup.timer=enabled`
- Backup file listing: `ferrumgate_20260508_154446.db`
- Backup timer next run observed: `Sat 2026-05-09 03:30:25 UTC`
- Final script status: `PASSED: All evidence checks succeeded.`

### Syntax Check

```bash
bash -n scripts/gcp/phase3e_sqlite_pilot_evidence.sh
```
Result: passed.

### Features Verified

| Feature | Behavior |
|---------|----------|
| Read-only HTTPS probes (healthz, readyz, readyz/deep, metrics) | Runs without --confirm |
| Metrics snapshot (store health, write queue) | Runs without --confirm |
| Auth probe (401 without token, 200 with token) | Runs without --confirm |
| Service status checks (caddy, ferrumgate, backup timer) | Runs without --confirm |
| Firewall rule summary | Runs without --confirm |
| Backup file listing (read-only) | Runs without --confirm |
| Token never printed, not even prefix | Enforced in script |
| GCP parameter overrides | Supported via flags and env vars |

### Script Non-Claims (Preserved)

The script explicitly outputs non-claims on each run:

```
Non-claims: NOT production-ready, NOT full G2/full production pilot authorization, NOT Phase 3E operator signoff.
            Signed conditional single-node SQLite pilot evidence only.
            No GCP config mutations performed.
```

---

## What Phase 3E Adds Over Phase 3C/3D Artifacts

Phase 3E provides:

1. **Bounded evidence script**: Non-destructive read-only evidence gathering that can be re-run at any time
2. **Metrics snapshot**: Store health and write queue depth evidence for SQLite single-node
3. **Backup file listing**: Read-only listing of existing backup files
4. **Conditional pilot focus**: Explicit single-node SQLite pilot evidence scope
5. **Token security confirmation**: Full token and token prefix never printed

Phase 3E does NOT:
- Replace Phase 3A/3B/3C/3D artifacts (which document the actual deployment and rehearsal runs)
- Claim full G2 completion, production readiness, or full production pilot authorization
- Modify GCP firewall, VM shape, TLS configuration, or FerrumGate runtime configuration

---

## Pilot Constraints (Preserved from Phase 3D)

| Constraint | Value |
|------------|-------|
| Database | SQLite single-node only (no PostgreSQL) |
| Topology | Single-node only (no multi-node/HA) |
| TLS Domain | nip.io (temporary, not for production) |
| Production readiness | NOT claimed |

---

## References

- Phase 3E plan: [99-phase3e-sqlite-pilot-evidence-plan.md](../99-phase3e-sqlite-pilot-evidence-plan.md)
- Phase 3A plan: [94-gcp-compute-phase3a-nonprod-target-plan.md](../94-gcp-compute-phase3a-nonprod-target-plan.md)
- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B plan: [95-gcp-compute-phase3b-domain-tls-plan.md](../95-gcp-compute-phase3b-domain-tls-plan.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C live ops packet: [96-gcp-compute-phase3c-live-ops-packet.md](../96-gcp-compute-phase3c-live-ops-packet.md)
- Phase 3C artifact: [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D G2 readiness: [98-phase3d-g2-readiness-checklist.md](../98-phase3d-g2-readiness-checklist.md)
- Phase 3D artifact: [2026-05-08-gcp-phase3d-g2-readiness.md](./2026-05-08-gcp-phase3d-g2-readiness.md)
