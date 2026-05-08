# 96 — GCP Compute Phase 3C Live Ops Packet

## Overview

Phase 3C consolidates live operations monitoring, rehearsal, and evidence collection procedures for the GCP non-prod FerrumGate VM. This packet provides the operator with bounded runbooks for health verification, auth probes, service monitoring, firewall inspection, and manual backup trigger procedures.

This document is **NOT production-ready**, **NOT G2 complete**, **NOT pilot authorized**, and **NOT operator signoff**.

---

## Non-Claims (Phase 3C)

> **IMPORTANT**: Phase 3C carries the following explicit non-claims:
> - NOT production-ready
> - NOT G2 complete (operator signoff pending)
> - NOT pilot authorized
> - NOT operator signoff
> - NOT suitable for production workloads
> - NOT a substitute for canonical operator docs 54/58/59/63/65
>
> Phase 3C is operator-owned GCP non-prod live rehearsal/evidence support only.

---

## Live Rehearsal Script

The primary Phase 3C tool is `scripts/gcp/phase3c_live_rehearsal.sh`, a reusable non-destructive rehearsal script.

### Script Features

| Feature | Read-Only | Requires --confirm |
|---------|-----------|-------------------|
| HTTPS health/readiness/deep/metrics probes | Yes | No |
| Auth probes (401 without token, 200 with token) | Yes | No |
| Service status (caddy, ferrumgate, backup timer) | Yes | No |
| Firewall rule summary | Yes | No |
| Manual backup service trigger | No | Yes |

### Usage

```bash
# Read-only health checks only (safe, no confirmation required)
bash scripts/gcp/phase3c_live_rehearsal.sh

# Read-only checks + manual backup trigger
bash scripts/gcp/phase3c_live_rehearsal.sh --run-backup --confirm

# With explicit GCP parameters
bash scripts/gcp/phase3c_live_rehearsal.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --tls-domain 34-158-51-8.nip.io \
  --confirm
```

### Script Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All checks passed |
| 1 | One or more checks failed (HTTP non-200, VM not found, etc.) |

### What the Script Does NOT Do

- The script never prints the full bearer token
- The script never commits any secret to the repository
- The script never modifies GCP resources (firewall rules, VM config)
- The script never claims production readiness or G2 completion

---

## Manual Runbook (Alternative to Script)

When running checks manually (e.g., via direct `gcloud compute ssh`), use this runbook.

### Pre-Flight: Verify VM Is Running

```bash
gcloud compute instances describe ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 \
    --format='value(name,status,networkInterfaces[0].accessConfigs[0].natIP)'
```

Expected: `ferrumgate-nonprod  RUNNING  34.158.51.8`

### Health/Readiness Probes

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

# With bearer token (retrieve from VM first, then probe)
# On VM:
#   sudo cat /etc/ferrumgate/ferrumgate_initial_token
TOKEN="<full-token-from-vm>"
curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${TOKEN}" \
    https://34-158-51-8.nip.io/v1/approvals
# Expected: 200
```

**Security note**: The full bearer token must never be printed to logs or committed to the repository. Retrieve it only via `sudo` on the VM, use it only for the immediate probe, and never persist it elsewhere.

### Service Status Checks

```bash
# SSH to VM and check services
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    "sudo systemctl is-active caddy && \
     sudo systemctl is-active ferrumgate.service && \
     sudo systemctl is-enabled ferrumgate-backup.timer"
```

Expected output: `active\nactive\nenabled`

### Firewall Summary

```bash
gcloud compute firewall-rules list \
    --project=fairy-b13f4 \
    --filter="network:ferrumgate-nonprod-vpc" \
    --format="table(name,allowed[].map().firewall_rule().list(),sourceRanges.list().list())"
```

Expected rules:

| Rule | Port | Source |
|------|------|--------|
| `ferrumgate-nonprod-fw-ssh` | TCP 22 | `118.69.4.63/32` |
| `ferrumgate-nonprod-fw-app` | TCP 19080 | `118.69.4.63/32` |
| `ferrumgate-nonprod-fw-http` | TCP 80 | `0.0.0.0/0` |
| `ferrumgate-nonprod-fw-https` | TCP 443 | `0.0.0.0/0` |

### Backup Timer Status

```bash
gcloud compute ssh ubuntu@ferrumgate-nonprod \
    --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
    "sudo systemctl list-timers --no-pager | grep ferrumgate"
```

Expected: `ferrumgate-backup.timer` listed with next scheduled run time.

### Manual Backup Trigger

```bash
# On VM (requires sudo):
sudo systemctl start ferrumgate-backup.service

# Check backup journal
sudo journalctl -u ferrumgate-backup.service -n 10 --no-pager

# Verify backup file created
ls -t /var/lib/ferrumgate/backups/
```

Expected: New `ferrumgate_<timestamp>.db` file in backup directory. Service returns to `inactive` state after oneshot completion (expected behavior).

---

## Token Security Protocol

1. **Retrieve token on VM only** via `sudo cat /etc/ferrumgate/ferrumgate_initial_token`
2. **Never print the full token** — scripts print only 8-character prefix
3. **Never commit token** to any repository or log
4. **Use token in-memory only** for immediate curl probes
5. **Token is root-protected** on VM (`/etc/ferrumgate/ferrumgate_initial_token` mode 600)

---

## Evidence Collection

When collecting evidence for the Phase 3C artifact:

1. Run `phase3c_live_rehearsal.sh` with `--confirm` to include backup trigger
2. Capture all output from the script
3. Record the exact timestamp of the run
4. Note any deviations from expected HTTP 200 statuses
5. Do not claim this evidence constitutes G2 completion or production readiness

---

## Troubleshooting

### HTTPS probes return 000 or connection refused

- Verify Caddy is running: `sudo systemctl is-active caddy`
- Verify ferrumgate is running: `sudo systemctl is-active ferrumgate.service`
- Check Caddy logs: `sudo journalctl -u caddy -n 20 --no-pager`
- Check ferrumgate logs: `sudo journalctl -u ferrumgate.service -n 20 --no-pager`
- Verify nip.io resolves: `nslookup 34-158-51-8.nip.io`

### Auth probe returns 403 instead of 200

- Verify the full token is correct (not just prefix)
- Verify token has not expired (bearer tokens are long-lived but verify)
- Check ferrumgate service is operational

### Backup trigger fails

- Verify ferrumgate.service is running (backup requires running service)
- Check journal: `sudo journalctl -u ferrumgate-backup.service -n 20 --no-pager`
- Verify backup directory exists: `ls -la /var/lib/ferrumgate/backups/`

### VM unreachable

- Check firewall rules allow your IP (SSH port 22 from `118.69.4.63/32`)
- Verify static IP is still assigned to VM
- Check VM status: `gcloud compute instances get-serial-port-output ferrumgate-nonprod --zone=asia-southeast1-a`

---

## Phase 3C Relationship to Phase 3A/3B

| Phase | Scope | Key Script |
|-------|-------|------------|
| 3A | GCP VM create/destroy, binary deploy, bootstrap | `phase3a_create/destroy/deploy` |
| 3B | TLS termination (nip.io + Caddy) | `phase3b_configure/destroy_tls_caddy` |
| 3C | Live rehearsal, health/auth checks, monitoring | `phase3c_live_rehearsal` |

Phase 3C does not replace Phase 3A or 3B. It provides a repeatable verification runbook after Phase 3A+3B have been executed.

---

## References

- Phase 3A plan: [94-gcp-compute-phase3a-nonprod-target-plan.md](./94-gcp-compute-phase3a-nonprod-target-plan.md)
- Phase 3A artifact: [artifacts/2026-05-08-gcp-phase3a-nonprod-target.md](./artifacts/2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B plan: [95-gcp-compute-phase3b-domain-tls-plan.md](./95-gcp-compute-phase3b-domain-tls-plan.md)
- Phase 3B artifact: [artifacts/2026-05-08-gcp-phase3b-domain-tls.md](./artifacts/2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C live rehearsal script: `scripts/gcp/phase3c_live_rehearsal.sh`
- Operator review packet (Phase 3A/3B): [97-phase3ab-operator-review-packet.md](./97-phase3ab-operator-review-packet.md)

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial Phase 3C live ops packet. Operator-owned rehearsal only. |
