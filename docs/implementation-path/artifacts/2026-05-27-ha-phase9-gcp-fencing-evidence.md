# HA Phase 9 GCP Fencing Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-gcp-fencing-evidence  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: GCP Compute instance fencing utility tested on standby host B; app-host guard verified on host A  
> **Constraint**: This is fencing-mechanism evidence **only**. It does **not** claim HA-4 automated failover, automated PostgreSQL promotion, PgBouncer routing automation, multi-host production HA, production-ready status, full G2 completion, or Block A closure.

---

## 1. Summary

The GCP Phase 9 fencing script (`scripts/gcp/phase9_fencing.sh`) was created and exercised:

- **Syntax validation passed** (`bash -n`).
- **Dry-run** against host B wrote no action and confirmed no PostgreSQL promote / no PgBouncer rewrite.
- **App-host guard** blocked fencing of `ferrumgate-nonprod` (host A) without `--force-app-host`, returning `RC=1`.
- **Real safe fencing test** on standby host B (`ferrumgate-pg-ha-b`) succeeded: instance reached `TERMINATED`, host A remained primary, app stayed healthy.
- **Recovery** succeeded: VM restarted, PostgreSQL started, B returned to standby streaming, A replication verified.

**Important**: This validates the fencing *mechanism* on a standby host. It does **not** validate an automated/fenced failover end-to-end (HA-4), because host A still carries the app and PgBouncer SPOF.

---

## 2. Script Safety Boundary

`scripts/gcp/phase9_fencing.sh` has the following hardcoded safety boundaries:

| Boundary | Behavior |
|----------|----------|
| No PostgreSQL promotion | Script calls `gcloud compute instances stop` only. It never runs `pg_promote`. |
| No PgBouncer rewrite | Script does not modify PgBouncer config or DSN. |
| Default dry-run | `--dry-run` is the default; `--fence` is required to act. |
| Confirm gate | `--fence` requires `--confirm <instance_name>` matching `--target`. |
| App-host guard | If target is `ferrumgate-nonprod` (the app/PgBouncer host), script refuses unless `--force-app-host` is passed. |
| Status check | Script refuses to fence an instance that is not `RUNNING`. |
| Poll to terminated | After stop, script polls up to 180 s for `TERMINATED` status. |

---

## 3. Evidence Detail

### 3.1 Syntax validation

```text
bash -n scripts/gcp/phase9_fencing.sh
RC=0
```

### 3.2 Dry-run against standby host B

```text
scripts/gcp/phase9_fencing.sh --target ferrumgate-pg-ha-b --dry-run
```

**Observed behavior**: dry-run logged `no_action_taken`, `no_postgres_promote`, `no_pgbouncer_rewrite`. No GCP API mutation was issued.

### 3.3 App-host guard on host A

```text
scripts/gcp/phase9_fencing.sh \
  --target ferrumgate-nonprod \
  --fence \
  --confirm ferrumgate-nonprod \
  --log-file /tmp/phase9-fencing-guard.log
RC=1
```

**Observed behavior**: script exited with `FATAL: refusing to fence app/PgBouncer host 'ferrumgate-nonprod' without --force-app-host`. The guard successfully prevents accidental fencing of the application host.

### 3.4 Real safe fencing test on standby host B

```text
scripts/gcp/phase9_fencing.sh \
  --target ferrumgate-pg-ha-b \
  --fence \
  --confirm ferrumgate-pg-ha-b \
  --log-file /tmp/phase9-fencing-hostb.log
```

| Milestone | Timestamp (UTC) | Observation |
|-----------|-----------------|-------------|
| Start | 2026-05-27T17:07:56Z | Command invoked |
| Stop issued | ~2026-05-27T17:07:57Z | `gcloud compute instances stop` accepted |
| Poll loop | 2026-05-27T17:07:57Z – 17:10:17Z | Poll interval 5 s; status transitions observed |
| Stop completed | 2026-05-27T17:10:17Z | Instance status `TERMINATED` confirmed |
| Host A health | During B fence | `f|0/17000148`; readyz/deep healthy |

**RTO impact on app**: None. Host A remained the writable primary; PgBouncer and ferrumd on host A continued serving traffic.

### 3.5 Recovery of host B

| Step | Command / Action | Result |
|------|------------------|--------|
| Restart VM | `gcloud compute instances start ferrumgate-pg-ha-b` | Success |
| Internal IP | Observed after start | `10.0.0.3` (unchanged) |
| PostgreSQL status | Checked immediately after VM boot | `inactive` — did not auto-start on boot |
| Manual start | `systemctl start postgresql` | Success |
| B standby state | `pg_is_in_recovery()` + replication lag | Standby `active`; watchdog timer `active`; `t|0/17000000|0/17000148` |
| A replication | `pg_stat_replication` on host A | `16/main|10.0.0.3|streaming` |
| A health post-recovery | readyz/deep | Healthy |

---

## 4. What This Proves

| Claim | Status | Evidence |
|-------|--------|----------|
| FG-1 — Fencing mechanism selected | ✅ **Progress** | GCP instance stop selected as concrete mechanism; script exists with safety boundaries. |
| FG-2 — Fencing tested | 📝 **Partial** | Standby host B was successfully fenced and recovered. App-host guard blocks host A by default. Full FG-2 requires old-primary isolation before standby promotion in a real failover scenario, which was not performed here. |
| Host A app/PgBouncer SPOF | ⚠️ **Still open** | Because host A carries ferrumd and PgBouncer, fencing host A would cause application outage regardless of PostgreSQL HA. This blocks safe host-A fencing and automated HA-4. |
| Automated promotion | ❌ **Not done** | No `pg_promote()` was executed by the script or automation. |
| PgBouncer reroute | ❌ **Not done** | No PgBouncer configuration change occurred. |

---

## 5. What This Does NOT Prove

| Non-claim | Rationale |
|-----------|-----------|
| **HA-4 automated failover** | Only a single-instance stop was performed on the standby. No automated detection, no promotion, no routing change. |
| **Multi-host production HA** | One standby fence does not prove sustained HA, partition tolerance, or repeated automated drills. |
| **Automated PostgreSQL promotion** | The script explicitly does not promote. |
| **Automated PgBouncer reroute** | The script explicitly does not rewrite PgBouncer config. |
| **Host A safe fencing** | Host A remains the app/PgBouncer SPOF; fencing it without `--force-app-host` is blocked by design. |
| **Production-ready** | No production-ready claim. |
| **Full G2** | Not complete; Tier 2 and Block A remain open. |
| **Block A closed** | Block A remains WAIVED/CONDITIONAL. |

---

## 6. Related Artifacts

- [`2026-05-27-ha-phase9-automated-failover-fencing-adr.md`](./2026-05-27-ha-phase9-automated-failover-fencing-adr.md) — Fencing ADR with gates FG-1 through FG-7
- [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](./2026-05-27-ha-phase9-multihost-drill-evidence.md) — Manual failover/failback drills
- [`2026-05-27-ha-phase9-watchdog-config-parity-evidence.md`](./2026-05-27-ha-phase9-watchdog-config-parity-evidence.md) — Detection-only watchdog evidence
- `scripts/gcp/phase9_fencing.sh` — GCP fencing utility

---

*Artifact created: 2026-05-27. Fencing-mechanism evidence on standby host B only. No HA-4 claim.*
