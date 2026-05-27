# Host B PgBouncer/ferrumd Redundancy and Fenced Failover Drill Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: Phase 9 host B application/PgBouncer redundancy preparation and bounded operator-controlled fenced failover drill  
> **Constraint**: Operator-controlled manual steps. No unattended automation. No production-ready, production HA, full G2, Block A closure, sustained SLO, or final production signoff claim.

---

## 1. Executive Summary

This artifact records installation and validation of PgBouncer and ferrumd on host B as standby application-layer redundancy, followed by a bounded operator-controlled fenced failover drill from host A to host B and subsequent failback to restore A-primary/B-standby topology.

Host B redundancy removed the prior absolute app/PgBouncer SPOF for the bounded drill: after host A was fenced, host B PostgreSQL was promoted, host B PgBouncer/ferrumd were started, and host B `/v1/readyz/deep` became healthy.

This is still **not HA-4 automated failover complete** because promotion, service start, routing choice, and failback were operator-controlled manual steps. No unattended automation or external endpoint cutover was demonstrated.

---

## 2. Host B Installation and Configuration

| Step | Detail |
|------|--------|
| PgBouncer install | Temporary external IP attached for package access, PgBouncer 1.22 installed, external IP removed afterward. |
| App runtime | `ferrumgate` user created; ferrumd/ferrumctl copied to `/opt/ferrumgate`. |
| Runtime config | `/etc/ferrumgate/env`, `/etc/ferrumgate/ferrumgate.toml`, PgBouncer config/userlist copied from host A; secrets were not printed or committed. |
| PgBouncer backend | `host=127.0.0.1 port=5432 dbname=ferrumgate`. |
| Config permissions | `/etc/ferrumgate/env` and config set to `root:ferrumgate` / `0640`. |
| Prepared statements | `max_prepared_statements = 100` added to host B PgBouncer. |
| Service policy | `ferrumgate.service` installed but kept disabled/stopped except during drills. |

Host B standby validation:

```text
pg_is_in_recovery() = t
```

Starting ferrumd while B was still standby failed with the expected read-only transaction error:

```text
cannot execute CREATE TABLE in a read-only transaction
```

---

## 3. Bounded Fenced Failover Drill With Host B Redundancy

### 3.1 Pre-Drill RPO Marker

| Host | Marker | WAL LSN |
|------|--------|---------|
| A primary | `235|phase9-fenced-drill-before-20260527T173015Z` | `0/170006F0` |
| B standby replay | `235|phase9-fenced-drill-before-20260527T173015Z` | `0/17000728|0/17000728` |

### 3.2 Pre-Fence Safety Steps on Host A

To reduce old-primary restart risk after boot, host A services were disabled before fencing:

```text
ferrumgate.service: disabled
pgbouncer: disabled
postgresql@16-standby: disabled
```

### 3.3 Fence Execution

| Parameter | Value |
|-----------|-------|
| Script | `scripts/gcp/phase9_fencing.sh` |
| Command shape | `--target ferrumgate-nonprod --fence --confirm ferrumgate-nonprod --force-app-host` |
| Fence start | `2026-05-27T17:30:37Z` |
| Instance terminated | `2026-05-27T17:31:35Z` |

### 3.4 Host B Promotion and Application Activation

| Event | Timestamp / Result |
|-------|--------------------|
| Host B promotion command started | `2026-05-27T17:31:41Z` |
| B `pg_is_in_recovery()` | `f` |
| B WAL LSN | `0/17000880` |
| B PgBouncer restarted | After promotion |
| B `ferrumgate.service` started | After promotion |
| B `/v1/readyz/deep` healthy | `2026-05-27T17:31:46Z` |

Observed application-level RTO from fence start to B readiness:

```text
2026-05-27T17:30:37Z -> 2026-05-27T17:31:46Z = 69 seconds
```

Observed RPO:

```text
0 marker loss; marker 235 replayed on B before promotion.
```

---

## 4. Restore / Failback to A-Primary / B-Standby

| Step | Result |
|------|--------|
| Host A VM started | Internal `10.0.0.2`; external `34.158.51.8` |
| Services on boot | Remained disabled/inactive |
| Host A rebuilt from B | `active`, `t|0/19000060|0/19000060` |
| Failback marker on B | `268|phase9-fenced-failback-before-20260527T173323Z`, WAL `0/19000670` |
| Marker replay on A | `268|phase9-fenced-failback-before-20260527T173323Z`, `0/190006A8|0/190006A8` |
| A promoted | `2026-05-27T17:33:48Z`, `pg_is_in_recovery() = f`, WAL `0/19000838` |
| A services re-enabled | `ferrumgate`, `pgbouncer`, `postgresql@16-standby` |
| A `/v1/readyz/deep` healthy | `2026-05-27T17:33:58Z` |
| B rebuilt as standby from A | `active`, `active`, `active`; `t|0/1B000060|0/1B000060` |

Final topology:

| Host | Role | Replication / Services |
|------|------|------------------------|
| A `ferrumgate-nonprod` | Primary + app/PgBouncer endpoint | `pg_stat_replication`: `16/main|10.0.0.3|streaming`; `postgresql@16-standby`, `pgbouncer`, `ferrumgate`, watchdog timer active; `/v1/readyz/deep` healthy |
| B `ferrumgate-pg-ha-b` | Standby + prepared app/PgBouncer redundancy | PostgreSQL standby active; PgBouncer active; watchdog active; ferrumgate disabled/stopped outside drills |

---

## 5. What This Proves

| Claim | Status | Evidence |
|-------|--------|----------|
| Host B app/PgBouncer redundancy can be prepared | ✅ PASS | ferrumd/ferrumctl/PgBouncer installed and configured on host B. |
| Fenced old-primary drill can recover app readiness on B | ✅ PASS / bounded | Host A fenced; B promoted; B PgBouncer/ferrumd started; B readyz/deep healthy in 69s. |
| RPO marker continuity | ✅ PASS | Marker 235 survived A→B; marker 268 survived B→A failback. |
| Failback to normal A-primary/B-standby topology | ✅ PASS | A restored primary and B restored standby with streaming replication. |
| HA-4 fully automated/fenced failover | ❌ NOT COMPLETE | Operator manually disabled services, fenced, promoted, started services, and rebuilt/failback. |

---

## 6. Boundary and Non-Claims

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — requires Tier 2 evidence and re-signoff. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain still required. |
| **multi-host production HA** | **NO** — bounded operator-controlled drill only; no unattended automation or production signoff. |
| **HA-4 automated failover** | **NOT COMPLETE** — no unattended promotion/routing/fencing workflow. |
| **External endpoint cutover** | **NOT COMPLETE** — no DNS/load-balancer/floating endpoint failover demonstrated. |
| **Sustained SLO window** | **NO** — no 7–30 day observation. |
| **Final production signoff** | **NOT COMPLETE** — bounded validation only. |

---

## 7. Related Artifacts

- [`2026-05-27-ha-phase9-gcp-fencing-evidence.md`](./2026-05-27-ha-phase9-gcp-fencing-evidence.md) — GCP fencing utility evidence.
- [`2026-05-27-ha-phase9-automated-failover-fencing-adr.md`](./2026-05-27-ha-phase9-automated-failover-fencing-adr.md) — ADR: automated failover and fencing approach.
- [`2026-05-27-ha-phase9-watchdog-config-parity-evidence.md`](./2026-05-27-ha-phase9-watchdog-config-parity-evidence.md) — Detection-only watchdog/config parity evidence.
- [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](./2026-05-27-ha-phase9-multihost-drill-evidence.md) — Manual multi-host drills.
- [`../../../../scripts/gcp/phase9_fencing.sh`](../../../../scripts/gcp/phase9_fencing.sh) — GCP fencing script.

---

*Artifact created: 2026-05-27. Host B redundancy and bounded operator-controlled fenced failover drill evidence. No production-ready or HA-4 automated completion claim.*
