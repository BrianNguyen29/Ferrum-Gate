# DEP-4 / DEP-6 Target Execution Evidence — 2026-05-19

## Status

- **Scope**: Target-host execution attempt for DEP-4 (systemd runtime validation) and DEP-6 (hosted backup/restore drill).
- **Verdict**: 🚫 BLOCKED — SSH access unavailable; no target-host commands executed.
- **DEP-4**: OPEN — runbook prepared, target-host `systemctl` runtime evidence not obtained.
- **DEP-6**: OPEN — preflight prepared, hosted-mode drill not executed.
- **Target-host validated**: NO.
- **Production-ready**: NO.
- **Operator signoff**: NOT OBTAINED.

This artifact records an **attempted** target-host execution against the GCP VM
`ferrumgate-nonprod`. It does **not** claim that DEP-4 or DEP-6 are complete,
that the target is validated, or that FerrumGate is production-ready. The
execution was blocked before any `systemctl` command or backup/restore drill was
run.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Cloud provider | Google Cloud Platform (GCP) |
| Project | `fairy-b13f4` |
| Zone | `asia-southeast1-a` |
| VM name | `ferrumgate-nonprod` |
| Machine type | `n2-standard-2` |
| Network tag | `ferrumgate-nonprod-app` |
| NAT IP | `34.158.51.8` |
| Domain | `ferrumgate.duckdns.org` |
| Source IP during attempt | `1.54.209.100` |

## Oracle review guidance

Before attempting target-host execution, an oracle review advised:

- **Fail-closed**: Do not proceed if access or identity prerequisites are unsatisfied.
- **DEP-4** is only safe if service identity is discovered/adapted on the target host.
- **DEP-6** is only safe with a temp-copy restore; never overwrite the live database.

These constraints were honored: no commands were executed on the target host.

## SSH access attempts

### Direct SSH

Command:

```bash
ssh <USER>@34.158.51.8
```

Observed result:

```
ssh: connect to host 34.158.51.8 port 22: Connection timed out
```

Exit code: non-zero (connection timeout).

### gcloud compute ssh

Command:

```bash
gcloud compute ssh ferrumgate-nonprod --project=fairy-b13f4 --zone=asia-southeast1-a
```

Observed result: exited **255**.

### IAP SSH

Command:

```bash
gcloud compute ssh ferrumgate-nonprod --project=fairy-b13f4 --zone=asia-southeast1-a --tunnel-through-iap
```

Observed result:

```
failed to connect to backend
Failed to connect to port 22
```

Exit code: **255**.

## VM status probe

Command:

```bash
gcloud compute instances describe ferrumgate-nonprod \
  --project=fairy-b13f4 \
  --zone=asia-southeast1-a \
  --format='table(name, status, zone, networkInterfaces[0].accessConfigs[0].natIP, tags.items)'
```

Observed result:

| Field | Value |
|-------|-------|
| name | `ferrumgate-nonprod` |
| status | `RUNNING` |
| zone | `asia-southeast1-a` |
| natIP | `34.158.51.8` |
| tags | `ferrumgate-nonprod-app` |

No boot or VM-level issues detected.

## Public health probes (observed, not a gate closure)

The following probes passed from the public internet. These confirm the HTTP
service is reachable; they do **not** validate systemd deployment or backup
procedures and must not be used to claim DEP-4 or DEP-6 closure.

| Endpoint | HTTP status | Response body |
|----------|-------------|---------------|
| `GET https://ferrumgate.duckdns.org/v1/healthz` | 200 | `{"status":"ok"}` |
| `GET https://ferrumgate.duckdns.org/v1/readyz/deep` | 200 | store and write_queue healthy |

## SSH troubleshoot findings

| Check | Result |
|-------|--------|
| Forward path from source `1.54.209.100` to `34.158.51.8` | **UNREACHABLE** |
| VM boot / status issues | 0 issues |
| User permissions / IAM | 0 issues |
| VPC / subnet settings | 0 issues |

### Firewall analysis

GCP firewall rule list showed:

- Rule `ferrumgate-nonprod-fw-ssh` allows source `118.69.4.63/32` to target tag `ferrumgate-nonprod-app`.
- Default-allow-ssh exists but connectivity test remains unreachable.

**Likely root cause**: source IP allowlist mismatch. The current source IP during
the attempt was `1.54.209.100`, while the firewall rule allows `118.69.4.63/32`.

**Resolution constraint**: Do not broaden firewall rules unless separately
authorized by the operator. No firewall changes were made.

## Blocker summary

| Gate | Blocker | Commands executed on target |
|------|---------|----------------------------|
| DEP-4 | SSH unreachable from current source IP; firewall allowlist mismatch | **None** |
| DEP-6 | Same SSH blocker; no hosted backup/restore drill possible | **None** |

Because SSH access was unavailable, **no target-host command execution was
attempted or performed**. Specifically:

- No `systemctl` commands were run.
- No binary or config was copied to the target host.
- No backup or restore commands were executed.
- No live database was read or written.

## Next action

1. **Operator decision**: Authorize either:
   - Updating the GCP firewall rule `ferrumgate-nonprod-fw-ssh` to include the
current source IP `1.54.209.100/32`, **or**
   - Executing the DEP-4/DEP-6 steps from a host whose source IP is already
allowed (`118.69.4.63/32`).
2. **Re-attempt SSH** from an authorized source.
3. If SSH succeeds, execute the DEP-4 runbook
(`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md`).
4. If DEP-4 closes successfully, execute the DEP-6 preflight
(`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md`).
5. Capture real evidence artifacts and seek operator signoff before marking
either gate complete.

## Workload / drill execution

**NOT EXECUTED**. The execution was halted at access verification. No systemd
service start, backup creation, restore drill, or operational command was run on
the target host.

## Anomalies and caveats

1. **Public health probes are not gate evidence**: The `healthz` and `readyz/deep`
successes confirm the ferrumd process is running (likely via Docker Compose or
another supervisor), but they do not validate systemd deployment, env-file
configuration, or backup/restore procedures.
2. **No secrets recorded**: No bearer tokens, passwords, DSNs, or private keys
appear in this artifact.
3. **No target changes made**: No files, firewall rules, or services were
modified on the target host.
4. **NOT target-host validated**: This artifact records only a blocked access
attempt. It does not validate DEP-4 or DEP-6.

## Operator signoff

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Operator reviewer | | | |

> **Blank until reviewed**. This artifact is a blocked execution record and has
> not been reviewed or signed off by an operator.

## Related docs

- [`docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-runbook.md`](./2026-05-19-dep4-target-host-systemd-runbook.md) — DEP-4 prepared runbook (gate open)
- [`docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-preflight.md`](./2026-05-19-dep6-hosted-backup-preflight.md) — DEP-6 prepared preflight (gate open)
- [`docs/production-readiness-v2/08-hosted-deployment-plan.md`](../../production-readiness-v2/08-hosted-deployment-plan.md) — Hosted deployment plan
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
- [`docs/ROADMAP.md`](../../ROADMAP.md) — Roadmap Phase 8
