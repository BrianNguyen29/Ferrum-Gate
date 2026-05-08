# GCP Phase 3C Live Rehearsal Artifact

**Date**: 2026-05-08

**Scope**: Google Cloud Compute Engine Phase 3C live rehearsal and health verification

**Status**: **NON-PROD rehearsal/demo evidence only**.

## Non-Claims

This Phase 3C run does **not** claim:

- production-ready status
- G2 completion
- pilot authorization
- operator signoff
- PostgreSQL/multi-node/HA readiness
- any canonical doc 54/58/59/63/65 signoff

Phase 3C is demo/test evidence only for the GCP non-prod VM rehearsal environment.

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

---

## Pre-Existing Infrastructure (Phase 3A + 3B)

This artifact assumes Phase 3A and Phase 3B have been previously executed:

- Phase 3A: GCP VM created, binaries deployed, ferrumgate service running, backup timer enabled
- Phase 3B: Caddy installed, TLS configured for nip.io domain, ferrumgate bind changed to localhost

Reference artifacts:
- [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)

---

## Live Evidence (Orchestrator-Gathered)

The following evidence was gathered by the orchestrator and confirmed before Phase 3C work began.

### Repo State

```
main...origin/main at 07d3cdc
```

### HTTPS Endpoint Statuses

| Endpoint | Expected | Observed |
|----------|----------|----------|
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz/deep` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | 200 |

### Auth Probe Results

| Probe | Expected | Observed |
|-------|----------|----------|
| `GET /v1/approvals` without token | 401 | 401 |
| `GET /v1/approvals` with VM-local bearer token | 200 | 200 |

Token handling: Only an 8-character token prefix was observed in script/operator output. The full bearer token was retrieved on-VM via `sudo` and used only for the immediate auth probe. The full token was never printed to logs or committed.

### Service Statuses

| Service | Expected | Observed |
|---------|----------|----------|
| `caddy.service` | active | active |
| `ferrumgate.service` | active | active |
| `ferrumgate-backup.timer` | enabled | enabled |

Observed via `sudo systemctl is-active` and `sudo systemctl is-enabled` on VM.

### Manual Backup Run

```bash
sudo systemctl start ferrumgate-backup.service
```

**Observed backup journal entry**:

```
Backup: /var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db
```

**Post-trigger service state**: Service became `inactive` after oneshot success (expected behavior for oneshot services).

### Firewall Summary

| Rule | Port | Source | Observed |
|------|------|--------|----------|
| `ferrumgate-nonprod-fw-ssh` | TCP 22 | `118.69.4.63/32` | Yes |
| `ferrumgate-nonprod-fw-app` | TCP 19080 | `118.69.4.63/32` | Yes |
| `ferrumgate-nonprod-fw-http` | TCP 80 | `0.0.0.0/0` | Yes |
| `ferrumgate-nonprod-fw-https` | TCP 443 | `0.0.0.0/0` | Yes |

### VM Details

| Field | Value |
|-------|-------|
| VM Name | `ferrumgate-nonprod` |
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| Static External IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` |

---

## Phase 3C Script Verification

The Phase 3C live rehearsal script (`scripts/gcp/phase3c_live_rehearsal.sh`) was validated with the following checks:

### Full Rehearsal Run

After an initial script bug was found and fixed (newline service-status parsing plus fail-closed checks), the strict full rehearsal run passed:

```bash
bash scripts/gcp/phase3c_live_rehearsal.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --tls-domain 34-158-51-8.nip.io \
  --confirm
```

Observed output summary:

```text
/v1/healthz: HTTP 200
/v1/readyz: HTTP 200
/v1/readyz/deep: HTTP 200
/v1/metrics: HTTP 200
GET /v1/approvals without token: HTTP 401 (expected: 401)
GET /v1/approvals with VM-local token: HTTP 200 (expected: 200)
caddy.service: active (expected: active)
ferrumgate.service: active (expected: active)
ferrumgate-backup.timer: enabled (expected: enabled)
Backup: /var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db
PASSED: All checks succeeded.
```

The full bearer token was not printed; only the 8-character prefix was displayed.

### Syntax Check

```bash
bash -n scripts/gcp/phase3c_live_rehearsal.sh
```
Result: No syntax errors.

### Features Verified

| Feature | Behavior |
|---------|----------|
| Read-only HTTPS probes (healthz, readyz, readyz/deep, metrics) | Runs without --confirm |
| Auth probe (401 without token, 200 with token) | Runs without --confirm |
| Service status checks (caddy, ferrumgate, backup timer) | Runs without --confirm |
| Firewall rule summary | Runs without --confirm |
| Backup timer status | Runs without --confirm |
| Manual backup trigger | Requires --confirm |
| Token prefix only printed (never full token) | Enforced in script |
| GCP parameter overrides | Supported via flags and env vars |

### Script Non-Claims (Preserved)

The script explicitly outputs non-claims on each run:

```
Non-claims: NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff.
            This is demo/test evidence only for non-prod GCP rehearsal.
```

---

## What Phase 3C Adds Over Phase 3A/3B Artifacts

Phase 3C provides:

1. **Bounded rehearsal script**: Non-destructive verification runbook that can be re-run at any time
2. **Live ops packet**: Consolidated manual runbook for operators who prefer direct `gcloud ssh` access
3. **Auth probe validation**: Confirmed 401 (no token) and 200 (with VM-local token) behavior
4. **Manual backup trigger evidence**: Confirmed oneshot service behavior after success
5. **Token security confirmation**: Full token never printed, only prefix

Phase 3C does NOT:
- Replace Phase 3A/3B artifacts (which document the actual deployment runs)
- Claim G2 completion or production readiness
- Modify GCP firewall, VM shape, TLS configuration, or FerrumGate runtime configuration during checks

When `--confirm` is used, Phase 3C may trigger the existing `ferrumgate-backup.service` oneshot to create or refresh a non-prod SQLite backup file. This is treated as a bounded non-prod rehearsal action, not production evidence.

---

## Cleanup Warning

To stop billing for the GCP non-prod VM, run:

```bash
bash scripts/gcp/phase3a_destroy_nonprod_vm.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --confirm
```

Do not run cleanup until all needed evidence has been collected and reviewed.

---

## References

- Phase 3A plan: [94-gcp-compute-phase3a-nonprod-target-plan.md](../94-gcp-compute-phase3a-nonprod-target-plan.md)
- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B plan: [95-gcp-compute-phase3b-domain-tls-plan.md](../95-gcp-compute-phase3b-domain-tls-plan.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C live ops packet: [96-gcp-compute-phase3c-live-ops-packet.md](../96-gcp-compute-phase3c-live-ops-packet.md)
- Operator review packet (Phase 3A/3B): [97-phase3ab-operator-review-packet.md](../97-phase3ab-operator-review-packet.md)
