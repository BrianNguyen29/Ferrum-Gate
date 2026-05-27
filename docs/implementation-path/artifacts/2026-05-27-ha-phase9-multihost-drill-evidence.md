# HA Phase 9 Multi-Host Drill Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-multihost-drill-evidence  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: Operator-environment two-host PostgreSQL streaming replication and manual failover evidence  
> **Result**: Multi-host manual failover evidence captured with RPO 0 marker loss and ferrumd readiness restored.  
> **Constraint**: This artifact does **not** claim production-ready, full G2, Block A closure, sustained SLO, multi-host automated failover, or production HA.

---

## 1. Topology

| Component | Host / VM | Zone | Network IP | Role after drill | Notes |
|-----------|-----------|------|------------|------------------|-------|
| PostgreSQL host A + PgBouncer + ferrumd | `ferrumgate-nonprod` | `asia-southeast1-a` | `10.0.0.2` | Standby + application/routing host | Existing operator VM; PgBouncer routed to host B after failover. |
| PostgreSQL host B | `ferrumgate-pg-ha-b` | `asia-southeast1-a` | `10.0.0.3` | Primary | New independent PostgreSQL VM. Temporary external IP `34.177.105.113` was attached for package installation because no Cloud NAT existed, then removed after PostgreSQL setup. |

GCP context:

- Project: `fairy-b13f4`
- Region/zone: `asia-southeast1` / `asia-southeast1-a`
- Network: `ferrumgate-nonprod-vpc`
- Firewall: `ferrumgate-pg-ha-internal`, allowing `tcp:5432,tcp:5433,tcp:6432` from `10.0.0.0/24` to tag `ferrumgate-pg-ha`
- PostgreSQL: 16.14

---

## 2. Execution Summary

| Gate | Status | Evidence summary |
|------|--------|------------------|
| MH-G1 — Topology deployed | ✅ PASS | Two independent VMs deployed. Initial primary host A `10.0.0.2:5433`; standby host B `10.0.0.3:5432`; `pg_stat_replication` showed host B streaming. |
| MH-G2 — Manual multi-host failover | ✅ PASS / manual | Host A PostgreSQL primary stopped; host B promoted with `pg_ctlcluster 16 main promote`; PgBouncer on host A rerouted to `10.0.0.3:5432`; ferrumd `/v1/readyz/deep` recovered to 200/healthy. |
| MH-G3 — Network partition drill | ✅ PASS / bounded standby partition | Outbound `10.0.0.2 -> 10.0.0.3:5432` was temporarily blocked with `iptables`; host A remained in recovery/read-only; attempted write failed; rule removed and replication resumed. |
| MH-G4 — Multi-drill consistency | ⚠️ PARTIAL | One real cross-host failover drill plus one bounded partition/split-brain check captured. The ADR target of 3+ multi-host drills, including both directions when practical, is **not yet complete**. |
| MH-G5 — RPO/RTO measurement log | ✅ PASS for Drill 1 | RPO marker `phase9-mh-drill1-before-20260527T141103Z` replayed on host B before promotion; RPO observed 0 marker loss. Application readiness was restored at 2026-05-27T14:16:26Z after primary stop at 2026-05-27T14:12:20Z, RTO 246s. |
| MH-G6 — Data consistency checks | ✅ PASS | Post-failover counts: `intents=4459`, `proposals=13`, `capabilities=13`, `provenance_events=26`, `failover_markers=5`. |
| MH-G7 — Post-failover operational validation | ⚠️ PARTIAL | ferrumd smoke passed and schema-only `pg_dump` succeeded on promoted host B. Full monitoring/alert incident workflow and repeated-drill incident/audit recording remain future work. |

---

## 3. Key Command Evidence

### 3.1 Host provisioning and access

Host B was created as a second PostgreSQL VM and later given temporary outbound package access:

```text
gcloud compute instances add-access-config ferrumgate-pg-ha-b ...
34.177.105.113
```

PostgreSQL install succeeded after outbound access was available:

```text
psql (PostgreSQL) 16.14 (Ubuntu 16.14-0ubuntu0.24.04.1)
```

Final VM facts:

```text
ferrumgate-nonprod   10.0.0.2  RUNNING  ferrumgate-nonprod-app;ferrumgate-pg-ha;ferrumgate-ssh-iap
ferrumgate-pg-ha-b   10.0.0.3  34.177.105.113  RUNNING  ferrumgate-pg-ha;ferrumgate-ssh-iap
```

After package installation, the temporary external IP was removed:

```text
ferrumgate-pg-ha-b   10.0.0.3  <no external IP>  RUNNING
```

### 3.2 Baseline streaming replication

Primary-side baseline from host A before failover:

```text
PRIMARY
f|2026-05-27 14:08:19.525414+00
REPL
16/main|127.0.0.1|streaming|async|||
16/main|10.0.0.3|streaming|async|||
SLOT
phase9_b_slot|t
PGB
2:ferrumgate = host=127.0.0.1 port=5433 dbname=ferrumgate
READY
{"status":"ok","healthy":true,...}
```

Standby-side baseline from host B:

```text
STANDBY
t|2026-05-27 14:08:39.725524+00|0/F000060|0/F000060|
COUNT
4459
```

### 3.3 RPO marker replay before promotion

Marker written on host A primary:

```text
100|phase9-mh-drill1-before-20260527T141103Z
0/F000328
```

Marker verified on host B standby before promotion:

```text
100|phase9-mh-drill1-before-20260527T141103Z
0/F000360|0/F000360
```

### 3.4 Manual failover and application RTO

Failover timeline:

```text
2026-05-27T14:12:20Z  host A primary stop initiated
2026-05-27T14:12:28Z  host B promotion command completed; pg_is_in_recovery=f
2026-05-27T14:12:37Z  PgBouncer reroute attempted; readiness initially 503 due TLS CA mismatch
2026-05-27T14:16:26Z  PostgreSQL TLS certs installed on host B and PgBouncer restarted; readyz/deep healthy
```

Application recovery evidence:

```text
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true},{"component":"pool","status":"ok: idle=0/total=1/max=10","healthy":true}]}
```

Observed Drill 1 metrics:

- RTO: 246 seconds from primary stop (`14:12:20Z`) to healthy application readiness (`14:16:26Z`).
- RPO: 0 observed marker loss; pre-failover marker ID `100` survived promotion.
- Root cause of added RTO: host B initially used the default snakeoil PostgreSQL TLS certificate; PgBouncer on host A required `server_tls_sslmode=verify-ca`. Installing the FerrumGate PostgreSQL server cert/key on host B resolved it.

Post-promotion marker on host B:

```text
100|phase9-mh-drill1-before-20260527T141103Z
133|phase9-mh-drill1-after-promote
```

### 3.5 Host A rebuilt as standby from host B

After failover, host A was rebuilt as the standby following host B primary:

```text
B_PRIMARY
f|0/11000060
REPL
16/main|10.0.0.2|streaming|async
MARKERS
5|133

A_STANDBY
t|0/11000060|0/11000060
READY
{"status":"ok","healthy":true,...}
PGB
2:ferrumgate = host=10.0.0.3 port=5432 dbname=ferrumgate
```

### 3.6 Network partition / split-brain check

Bounded standby network partition on host A:

```text
PARTITION_START=2026-05-27T14:20:46Z
A_RECOVERY
t
A_WRITE_ATTEMPT
ERROR:  cannot execute INSERT in a read-only transaction
PARTITION_END=2026-05-27T14:20:54Z
t|0/11000060|0/11000060
```

Replication resumed from host B to host A:

```text
16/main|10.0.0.2|streaming
```

Conclusion: during the bounded partition check, host A remained read-only and did not become a second writable primary.

### 3.7 Post-failover backup and consistency

Schema-only backup succeeded on promoted host B:

```text
/tmp/phase9-postfailover-schema-20260527T142131Z.sql 17645 bytes
```

Post-failover table counts:

```text
4459|13|13|26|5
```

Column order:

```text
intents|proposals|capabilities|provenance_events|failover_markers
```

---

## 4. Deviations and Issues Found

| Issue | Impact | Resolution / follow-up |
|-------|--------|------------------------|
| Host B initially had no outbound package path. | `apt-get update` could not reach Ubuntu mirrors. | Temporary external IP `34.177.105.113` attached for installation and removed afterward. Prefer Cloud NAT or prebuilt image for future repeatability. |
| Accidental root-owned `.bak.phase9-*` files were placed inside the primary data directory. | `pg_basebackup` initially failed with permission denied. | Removed accidental backup files before retrying. Future config backups must be stored outside `PGDATA`. |
| Replication slot `phase9_b_slot` already existed after a failed basebackup attempt. | Retry with `-C` failed. | Reused the inactive existing slot on retry. |
| Host B PostgreSQL used default snakeoil TLS cert after promotion. | PgBouncer `verify-ca` rejected the promoted primary, increasing RTO. | Installed FerrumGate PostgreSQL TLS cert/key on host B and restarted PostgreSQL/PgBouncer. |
| Host A standby rebuild required matching WAL settings. | Startup initially failed because `max_wal_senders=5` was lower than primary value 10. | Set `max_wal_senders=10` and `max_replication_slots=10`; standby started and streamed. |

---

## 5. Current End State

| Component | State |
|-----------|-------|
| Host B PostgreSQL (`10.0.0.3:5432`) | Primary, writable, `pg_is_in_recovery() = false` |
| Host A PostgreSQL (`10.0.0.2:5433`) | Standby, streaming from host B, `pg_is_in_recovery() = true` |
| PgBouncer on host A | Routes `ferrumgate` to `10.0.0.3:5432` |
| ferrumd on host A | `/v1/readyz/deep` healthy through PgBouncer |
| Temporary host B external IP | Removed after package installation; host B remains reachable through IAP/internal networking |

---

## 6. Non-Claims Preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — re-signoff still requires Tier 2 evidence. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain is still required. |
| **multi-host production HA** | **NO** — manual evidence exists, but repeated drills, automation/fencing, and sustained operational signoff remain incomplete. |
| **HA-4 automated failover** | **NOT COMPLETE** — this was manual/operator-controlled failover, not automated multi-host failover. |
| **3+ multi-host drills** | **NOT COMPLETE** — one cross-host failover drill plus one partition/split-brain check captured. |
| **sustained SLO window** | **NO** — no 7–30 day observation window. |

---

## 7. Follow-Ups

1. Replace ad-hoc package installation with Cloud NAT, private package mirror, or image-based provisioning for future repeatability.
2. Repeat multi-host drills until at least three passing drill artifacts exist, including failback B→A when practical.
3. Decide the multi-host automated failover/fencing ADR before any HA-4 completion claim.
4. Add monitoring/alert incident evidence for the promoted-primary topology.
5. Keep Tier 2 blocked until real domain, revalidation, G2 re-signoff, sustained SLO, and final operator signoff complete.

---

*Artifact created: 2026-05-27. Multi-host manual HA evidence captured with conservative non-claims preserved.*
