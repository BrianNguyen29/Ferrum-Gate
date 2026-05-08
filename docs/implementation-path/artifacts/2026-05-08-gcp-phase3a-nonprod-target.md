# GCP Phase 3A Non-Prod Target Rehearsal — Run Result

**Generated**: 2026-05-08
**Scope**: Google Cloud Compute Engine Phase 3A non-prod target rehearsal
**Status**: Local/operator-owned non-prod target resources created and tested

## Target Summary

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Machine | `e2-medium` |
| Image | Ubuntu 24.04 LTS amd64 (`ubuntu-2404-lts-amd64`) |
| Network | `ferrumgate-nonprod-vpc` |
| Subnet | `ferrumgate-nonprod-subnet` (`10.0.0.0/24`) |
| Static IP | `34.158.51.8` |
| Internal IP | `10.0.0.2` |
| App Port | `19080` |
| Firewall Allowlist | `118.69.4.63/32` for SSH and app port |
| Store DSN | `sqlite:///var/lib/ferrumgate/data/ferrumgate.db?mode=rwc` |
| Backup Dir | `/var/lib/ferrumgate/backups` |
| Config Dir | `/etc/ferrumgate` |

## Scripts Used

- `scripts/gcp/phase3a_create_nonprod_vm.sh`
- `scripts/gcp/phase3a_deploy_binaries.sh`
- `scripts/gcp/phase3a_bootstrap_vm.sh`
- `scripts/gcp/phase3a_destroy_nonprod_vm.sh` (cleanup path; not run)

## Creation Result

`phase3a_create_nonprod_vm.sh --confirm` created/reused:

- custom VPC
- regional subnet
- regional static external IP
- firewall rule for SSH from allowlisted IP only
- firewall rule for app port `19080` from allowlisted IP only
- VM `ferrumgate-nonprod`

Observed VM result:

```text
NAME                ZONE               MACHINE_TYPE  INTERNAL_IP  EXTERNAL_IP  STATUS
ferrumgate-nonprod  asia-southeast1-a  e2-medium     10.0.0.2     34.158.51.8 RUNNING
```

## Deployment Result

`phase3a_deploy_binaries.sh --confirm`:

- built release `ferrumd` and `ferrumctl`
- copied binaries to `/opt/ferrumgate/`
- ran VM bootstrap
- generated bearer token on VM without printing or committing the full token
- installed systemd service and backup timer
- restarted service successfully

Observed service status:

```text
ferrumgate.service: active (running)
ferrumgate-backup.timer: enabled
```

## Probe Results

External probes from allowlisted operator IP:

| Probe | Expected | Observed |
|-------|----------|----------|
| `GET /v1/healthz` | `200` | `200` |
| `GET /v1/readyz` | `200` | `200` |
| `GET /v1/readyz/deep` | `200` | `200` |
| `GET /v1/metrics` | `200` | `200` |
| `GET /v1/approvals` without token | `401` | `401` |
| `GET /v1/approvals` with VM bearer token | `200` | `200` |

Bearer token handling note: the full token remains only on the VM under root-protected files. Only token prefixes were printed during setup.

## Backup Result

Manual backup service run:

```text
sudo systemctl start ferrumgate-backup.service
```

Observed backup file:

```text
/var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db
```

## Explicit Non-Claims

This Phase 3A run does **not** claim:

- production-ready status
- G2 completion
- pilot authorization
- operator signoff
- domain validation
- TLS readiness
- Phase 3 PostgreSQL/multi-node/HA readiness

Phase 3B remains required for domain/TLS hardening. Canonical G2/operator docs remain gated by real evidence review and signoff.

## Cleanup

To stop billing for Phase 3A resources, run the cleanup script with explicit confirmation:

```bash
bash scripts/gcp/phase3a_destroy_nonprod_vm.sh \
  --project-id fairy-b13f4 \
  --region asia-southeast1 \
  --zone asia-southeast1-a \
  --confirm
```

Do not run cleanup until any needed evidence has been collected.
