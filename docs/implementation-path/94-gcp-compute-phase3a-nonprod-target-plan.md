# 94 — GCP Compute Phase 3A Non-Prod Target Plan

## Overview

Phase 3A establishes an operator-owned GCP non-prod Compute Engine target for FerrumGate rehearsal and evidence collection. This is **NOT production-ready**, **NOT G2 complete**, **NOT pilot authorized**, and **NOT operator signoff**.

This document describes the GCP non-prod target architecture, script usage, required environment variables, test commands, cleanup procedure, claim boundaries, and the Phase 3B roadmap (domain/TLS).

---

## Non-Claims (Phase 3A)

> **IMPORTANT**: Phase 3A carries the following explicit non-claims:
> - NOT production-ready
> - NOT G2 complete (operator signoff pending)
> - NOT pilot authorized
> - NOT operator signoff
> - NOT target evidence for any gate review
> - NOT complete FerrumGate deployment (no domain, no TLS)
>
> Phase 3A is operator-owned GCP non-prod target rehearsal/evidence support only.

---

## Architecture

### GCP Resources

| Resource | Name | Notes |
|---|---|---|
| Custom VPC | `ferrumgate-nonprod-vpc` | `/24` subnet, `asia-southeast1` |
| Subnet | `ferrumgate-nonprod-subnet` | `10.0.0.0/24`, regional |
| Static External IP | `ferrumgate-nonprod-ip` | Regional, reserved |
| Firewall: SSH | `ferrumgate-nonprod-fw-ssh` | TCP 22 from allowlist only |
| Firewall: App | `ferrumgate-nonprod-fw-app` | TCP 19080 from allowlist only |
| VM | `ferrumgate-nonprod` | `e2-medium`, Ubuntu 24.04 LTS amd64, 30GB pd-balanced |
| Network Tag | `ferrumgate-nonprod-app` | Applied to VM for firewall targeting |

### Network Security

- **SSH (TCP 22)**: Source restricted to `ALLOWLIST_CIDR` (default: `118.69.4.63/32`)
- **App (TCP 19080)**: Source restricted to `ALLOWLIST_CIDR` (default: `118.69.4.63/32`)
- **No broad `0.0.0.0/0` rules** — custom VPC with strict ingress allowlisting
- Default network is NOT used; broad SSH from anywhere is avoided

### VM Configuration

| Parameter | Value |
|---|---|
| Machine Type | `e2-medium` (2 vCPU, 4 GB RAM) |
| Image | Ubuntu 24.04 LTS amd64 (`ubuntu-2404-lts-amd64`) |
| Boot Disk | 30 GB pd-balanced |
| External IP | Static regional |
| Internal IP | From subnet `10.0.0.0/24` |
| Network | Custom VPC `ferrumgate-nonprod-vpc` |

### FerrumGate Configuration (on VM)

| Parameter | Value |
|---|---|
| Bind Address | `0.0.0.0` |
| App Port | `19080` |
| Auth Mode | `bearer` |
| Store DSN | `sqlite:///var/lib/ferrumgate/data/ferrumgate.db?mode=rwc` |
| Store Synchronous | `true` |
| WAL Autocheckpoint | `1000` |
| Service User | `ferrumgate` (system, no-login) |
| Data Dir | `/var/lib/ferrumgate` |
| Backup Dir | `/var/lib/ferrumgate/backups` |
| Log Dir | `/var/log/ferrumgate` |
| Config Dir | `/etc/ferrumgate` |

### Backup Schedule

- **Service**: `ferrumgate-backup.service` (oneshot)
- **Timer**: `ferrumgate-backup.timer` — 5 min after boot, then hourly
- **Target**: `/var/lib/ferrumgate/backups/ferrumgate_<timestamp>.db`
- SQLite native `VACUUM`-safe copy; service requires `ferrumgate.service` to be running

---

## Scripts

All scripts live under `scripts/gcp/`.

| Script | Purpose | Runs On |
|---|---|---|
| `phase3a_create_nonprod_vm.sh` | Creates all GCP resources (VPC, subnet, IP, firewall, VM) | **Local** (operator machine) |
| `phase3a_destroy_nonprod_vm.sh` | Destroys all GCP resources in safe order | **Local** (operator machine) |
| `phase3a_bootstrap_vm.sh` | Bootstraps VM: user, dirs, config, service, backup timer | **VM** (via SSH) |
| `phase3a_deploy_binaries.sh` | Builds release, copies binaries, runs bootstrap, restarts service | **Local** (operator machine) |

---

## Required Environment Variables / Flags

### Create (`phase3a_create_nonprod_vm.sh`)

| Variable | Flag | Default |
|---|---|---|
| `GCP_PROJECT_ID` | `--project-id` | `fairy-b13f4` |
| `GCP_REGION` | `--region` | `asia-southeast1` |
| `GCP_ZONE` | `--zone` | `asia-southeast1-a` |
| `GCP_VM_NAME` | `--vm-name` | `ferrumgate-nonprod` |
| `GCP_ALLOWLIST_CIDR` | `--allowlist-cidr` | `118.69.4.63/32` |
| `GCP_APP_PORT` | `--app-port` | `19080` |
| `GCP_MACHINE_TYPE` | `--machine-type` | `e2-medium` |
| `GCP_DISK_SIZE_GB` | `--disk-size-gb` | `30` |

**Required**: `--confirm` flag (or `CONFIRM=true`) to avoid accidental cost.

### Destroy (`phase3a_destroy_nonprod_vm.sh`)

Same project/region/zone/vm-name variables. **Required**: `--confirm`.

### Deploy (`phase3a_deploy_binaries.sh`)

Same project/region/zone/vm-name variables, plus:

| Variable | Flag | Default |
|---|---|---|
| `FERRUM_VERSION` | `--version` | `dev-build` |

**Required**: `--confirm` for actual deployment. `--build-only` builds binaries locally without deploying.

### Bootstrap (`phase3a_bootstrap_vm.sh`)

Runs **on the VM** via SSH. Set environment variables before SSH:

| Variable | Default |
|---|---|
| `FERRUM_APP_PORT` | `19080` |
| `FERRUM_BIND_ADDR` | `0.0.0.0` |
| `FERRUM_STORE_DSN` | `sqlite:///var/lib/ferrumgate/data/ferrumgate.db?mode=rwc` |
| `FERRUM_VERSION` | `placeholder` (set by deploy script) |
| `FERRUM_BEARER_TOKEN` | (generated on VM if not set) |

---

## Usage Sequence

### 1. Create GCP Resources

```bash
cd /home/uong_guyen/work/Ferrum-Gate

# Dry-run check (no resources created, no --confirm needed)
gcloud compute instances list --project=fairy-b13f4 --zone=asia-southeast1-a
gcloud compute networks list --project=fairy-b13f4

# Create with explicit confirmation
bash scripts/gcp/phase3a_create_nonprod_vm.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --confirm
```

Expected output: `VM_NAME`, `INTERNAL_IP`, `EXTERNAL_IP`, `ZONE`, `REGION`, `PROJECT_ID`.

### 2. Build and Deploy Binaries

```bash
# Build only (no deploy)
bash scripts/gcp/phase3a_deploy_binaries.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --build-only

# Build and deploy (requires --confirm)
bash scripts/gcp/phase3a_deploy_binaries.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --version "phase3a-$(date +%Y%m%d)" \
  --confirm
```

### 3. Verify Deployment

```bash
# Retrieve token prefix (never full token)
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
  "grep FERRUMD_BEARER_TOKEN /etc/ferrumgate/env | cut -d= -f2 | head -c 8"

# Test health endpoint (replace <token> with full token retrieved via sudo on VM)
curl -H "Authorization: Bearer <token>" http://<EXTERNAL_IP>:19080/v1/healthz

# Test readyz endpoint
curl -H "Authorization: Bearer <token>" http://<EXTERNAL_IP>:19080/v1/readyz

# Test deep readyz
curl -H "Authorization: Bearer <token>" http://<EXTERNAL_IP>:19080/v1/readyz/deep

# Check service status
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
  "sudo systemctl status ferrumgate.service --no-pager"

# Check backup timer
gcloud compute ssh ubuntu@ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet -- \
  "sudo systemctl list-timers --no-pager | grep ferrumgate"
```

### 4. Test FerrumGate Capabilities (operator-owned rehearsal)

```bash
# Example: Test with ferrumctl from operator machine
FERRUMCTL_BIN="$REPO_ROOT/target/release/ferrumctl"
BASE_URL="http://<EXTERNAL_IP>:19080"

# Retrieve token from VM (requires sudo on VM):
#   sudo cat /etc/ferrumgate/ferrumgate_initial_token

$FERRUMCTL_BIN --base-url "$BASE_URL" --bearer-token "<token>" probe health
```

### 5. Cleanup

```bash
bash scripts/gcp/phase3a_destroy_nonprod_vm.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --confirm
```

---

## Token Security

- **Bearer token** is generated on the VM during bootstrap if not provided
- The full token is stored in `/etc/ferrumgate/ferrumgate_initial_token` (root-only read)
- The full token is **never printed to logs or console** by any script
- Scripts print only the **token prefix** (first 8 characters)
- To retrieve the full token: `sudo cat /etc/ferrumgate/ferrumgate_initial_token` on the VM
- **Do NOT commit any bearer token to the repository**

---

## Claim Boundaries and Evidence Limitations

Phase 3A evidence is **operator-owned** and **non-prod**. It cannot be used for:

- G2 signoff
- Pilot authorization
- Production readiness determination
- Any gate review requiring canonical evidence (docs 54, 58, 59, 63, 65)

### What Phase 3A can support

- Internal operator rehearsal of the full target setup procedure
- Smoke testing of GCP resource creation/destruction idempotency
- FerrumGate binary deployment and bootstrap validation
- Evidence collection for the **operator's own** Phase 3A review
- Backup/restore drill on the non-prod SQLite store

### What Phase 3A does NOT support

- Production workload
- Real operator signoff (requires canonical docs 54/58/59/63/65)
- G2 completion
- Pilot authorization
- Domain/TLS (see Phase 3B below)

---

## Phase 3B: Domain and TLS (Deferred)

Phase 3B is out of scope for this document and this task. When Phase 3B is authorized:

1. A real domain or Cloud DNS-managed domain will be provisioned
2. Let's Encrypt or Cloud CA-managed TLS certificates will be issued
3. The VM will be updated to bind HTTPS (443) in addition to or instead of 19080
4. DNS A record will point to the static external IP
5. Firewall rules will be updated to allow 443 from `0.0.0.0/0` (or scoped)
6. Documentation will be updated with the Phase 3B plan and non-claims preserved

**Phase 3B is NOT authorized at this time.**

---

## Troubleshooting

### VM unreachable after create

- Check firewall rules: SSH (22) and app (19080) must allow from your current IP
- Verify static IP is assigned: `gcloud compute instances describe ferrumgate-nonprod --zone=asia-southeast1-a --format='value(networkInterfaces[0].accessConfigs[0].natIP)'`
- Check VM status: `gcloud compute instances get-serial-port-output ferrumgate-nonprod --zone=asia-southeast1-a`

### Service fails to start

```bash
# On VM:
sudo journalctl -u ferrumgate.service -n 50 --no-pager
sudo systemctl status ferrumgate.service --no-pager
# Check binary exists and is executable:
ls -la /opt/ferrumgate/ferrumd
file /opt/ferrumgate/ferrumd
```

### Token retrieval

```bash
# On VM (requires root):
sudo cat /etc/ferrumgate/ferrumgate_initial_token

# Or from deploy script output (token prefix shown):
# Token prefix: <prefix>...
```

### Backup not running

```bash
# On VM:
sudo systemctl list-timers --all | grep ferrumgate
sudo systemctl status ferrumgate-backup.timer
sudo journalctl -u ferrumgate-backup.service -n 10 --no-pager
ls -la /var/lib/ferrumgate/backups/
```

---

## References

- GCP Compute Engine documentation: <https://cloud.google.com/compute/docs>
- GCP Firewall rules: <https://cloud.google.com/vpc/docs/firewalls>
- GCP Static external IP: <https://cloud.google.com/compute/docs/ip-addresses/reserve-static-external-ip-address>
- FerrumGate AGENTS.md (`../AGENTS.md`) — workspace configuration and critical invariants
- Canonical operator docs: `63-path-2-target-environment-spec.md`, `65-path-2-target-questionnaire.md` (NOT modified by Phase 3A)

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial Phase 3A non-prod target plan. Operator-owned rehearsal only. |
