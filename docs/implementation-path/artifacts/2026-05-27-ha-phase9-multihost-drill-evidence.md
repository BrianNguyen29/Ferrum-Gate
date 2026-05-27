# HA Phase 9 Multi-Host Drill Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-multihost-drill-evidence  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: Operator-environment two-host PostgreSQL streaming replication and manual failover evidence  
> **Result**: Multi-host manual failover evidence captured with 4 manual drill passes, bidirectional failover/failback, RPO 0 marker loss, ferrumd readiness restored, and monitoring readiness captured.
> **Constraint**: This artifact does **not** claim production-ready, full G2, Block A closure, sustained SLO, multi-host automated failover, or production HA.

---

## 1. Topology

| Component | Host / VM | Zone | Network IP | Role after drill | Notes |
|-----------|-----------|------|------------|------------------|-------|
| PostgreSQL host A + PgBouncer + ferrumd | `ferrumgate-nonprod` | `asia-southeast1-a` | `10.0.0.2` | Primary + application/routing host after repeated drills | Existing operator VM; PgBouncer routes to local PostgreSQL after Drill 4 failback. |
| PostgreSQL host B | `ferrumgate-pg-ha-b` | `asia-southeast1-a` | `10.0.0.3` | Standby after repeated drills | New independent PostgreSQL VM. Temporary external IP `34.177.105.113` was attached for package installation because no Cloud NAT existed, then removed after PostgreSQL setup. |

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
| MH-G4 — Multi-drill consistency | ✅ PASS / manual | Four manual multi-host drills captured: A→B, B→A failback, A→B repeat, and B→A failback repeat. All preserved RPO marker evidence and restored ferrumd readiness. |
| MH-G5 — RPO/RTO measurement log | ✅ PASS / manual | Each drill has before/after timestamps and marker evidence. Observed RPO was 0 marker loss for all captured drills. RTOs: Drill 1 246s, Drill 2 59s, Drill 3 29s, Drill 4 22s. |
| MH-G6 — Data consistency checks | ✅ PASS | Post-failover counts stayed consistent for FerrumGate core tables; final `failover_markers=10`, max marker ID `234`. |
| MH-G7 — Post-failover operational validation | ✅ PASS for bounded monitoring readiness / PARTIAL for full incident process | ferrumd smoke passed, schema-only `pg_dump` succeeded, Prometheus readiness passed, active Prometheus targets were up, and Alertmanager API was reachable. Full human incident-response signoff remains future work. |

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

### 3.8 Repeated multi-host drills and failback

After Drill 1, three additional manual multi-host drills were executed to satisfy the repeated-drill evidence gate and exercise both directions.

| Drill | Direction | Primary stop | Promotion complete | ferrumd ready | RTO | RPO marker evidence |
|-------|-----------|--------------|--------------------|---------------|-----|---------------------|
| Drill 1 | A→B | `2026-05-27T14:12:20Z` | `2026-05-27T14:12:28Z` | `2026-05-27T14:16:26Z` | 246s | `100|phase9-mh-drill1-before-20260527T141103Z` survived; after marker `133|phase9-mh-drill1-after-promote` written. |
| Drill 2 | B→A failback | `2026-05-27T15:50:48Z` | `2026-05-27T15:51:30Z` | `2026-05-27T15:51:47Z` | 59s | `134|phase9-repeat-drill2-before-20260527T155017Z` replayed on A before promotion. |
| Drill 3 | A→B repeat | `2026-05-27T15:58:00Z` | `2026-05-27T15:58:12Z` | `2026-05-27T15:58:29Z` | 29s | `167|phase9-repeat-drill3-before-20260527T155502Z` survived; after marker `200|phase9-repeat-drill3-after-promote` written. |
| Drill 4 | B→A failback repeat | `2026-05-27T16:00:58Z` | `2026-05-27T16:01:04Z` | `2026-05-27T16:01:20Z` | 22s | `201|phase9-repeat-drill4-before-20260527T160026Z` survived; after marker `234|phase9-repeat-drill4-after-promote` written. |

Drill 2 marker replay evidence:

```text
134|phase9-repeat-drill2-before-20260527T155017Z
0/11000518|0/11000518
```

Drill 3 marker replay and post-promotion evidence:

```text
167|phase9-repeat-drill3-before-20260527T155502Z
0/130004A8|0/130004A8
167|phase9-repeat-drill3-before-20260527T155502Z
200|phase9-repeat-drill3-after-promote
8|200
```

Drill 4 marker replay and post-promotion evidence:

```text
201|phase9-repeat-drill4-before-20260527T160026Z
0/15000578|0/15000578
201|phase9-repeat-drill4-before-20260527T160026Z
234|phase9-repeat-drill4-after-promote
10|234
```

Final topology after repeated drills was restored to host A primary and host B standby:

```text
A_PRIMARY
f|0/17000060
REPL
16/main|10.0.0.3|streaming|async
PGB
2:ferrumgate = host=127.0.0.1 port=5433 dbname=ferrumgate
READY
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true},{"component":"pool","status":"ok: idle=1/total=2/max=10","healthy":true}]}

B_STANDBY
t|0/17000060|0/17000060
```

### 3.9 Monitoring and incident-readiness evidence

Bounded post-failover monitoring evidence was captured from host A after promoted-primary routing:

```text
SERVICES
active
inactive
active
active
active
READY
{"status":"ok","healthy":true,...}
PROM_READY
Prometheus Server is Ready.
ALERT_READY
OK
```

The `systemctl is-active` sequence above corresponded to:

```text
prometheus=active
alertmanager=inactive
pgbouncer=active
ferrumgate.service=active
postgresql@16-standby=active
```

Alertmanager's HTTP API still responded `OK` on `127.0.0.1:9093`; the systemd unit state should be investigated separately before claiming complete alerting posture.

Prometheus target/rule/alert summary:

```text
TARGETS
[('ferrumgate', 'up'), ('ferrumgate-ferrumd', 'up')]
RULES
success
2
ALERTS
2
[('FerrumGatePostgresSlowAcquire', 'active'), ('FerrumGatePostgresSlowAcquire', 'active')]
```

Conclusion: bounded monitoring readiness was present, Prometheus had active targets/rules, and Alertmanager API was reachable. This is not a human incident-response or paging-delivery signoff.

---

## 4. Deviations and Issues Found

| Issue | Impact | Resolution / follow-up |
|-------|--------|------------------------|
| Host B initially had no outbound package path. | `apt-get update` could not reach Ubuntu mirrors. | Temporary external IP `34.177.105.113` attached for installation and removed afterward. Prefer Cloud NAT or prebuilt image for future repeatability. |
| Accidental root-owned `.bak.phase9-*` files were placed inside the primary data directory. | `pg_basebackup` initially failed with permission denied. | Removed accidental backup files before retrying. Future config backups must be stored outside `PGDATA`. |
| Replication slot `phase9_b_slot` already existed after a failed basebackup attempt. | Retry with `-C` failed. | Reused the inactive existing slot on retry. |
| Host B PostgreSQL used default snakeoil TLS cert after promotion. | PgBouncer `verify-ca` rejected the promoted primary, increasing RTO. | Installed FerrumGate PostgreSQL TLS cert/key on host B and restarted PostgreSQL/PgBouncer. |
| Host A standby rebuild required matching WAL settings. | Startup initially failed because `max_wal_senders=5` was lower than primary value 10. | Set `max_wal_senders=10` and `max_replication_slots=10`; standby started and streamed. |
| Host A HBA ordering rejected local TCP `postgres` connection after failback. | A verification query using `-h 127.0.0.1` failed after Drill 2. | Used local socket verification and preserved ferrumd/PgBouncer readiness. HBA parity should be normalized before automation. |
| Alertmanager systemd unit reported inactive while HTTP API returned `OK`. | Full alerting posture cannot be claimed from this evidence alone. | Treat as bounded monitoring readiness only; investigate Alertmanager service state before production alerting signoff. |

---

## 5. Current End State

| Component | State |
|-----------|-------|
| Host A PostgreSQL (`10.0.0.2:5433`) | Primary, writable, `pg_is_in_recovery() = false` |
| Host B PostgreSQL (`10.0.0.3:5432`) | Standby, streaming from host A, `pg_is_in_recovery() = true` |
| PgBouncer on host A | Routes `ferrumgate` to `127.0.0.1:5433` |
| ferrumd on host A | `/v1/readyz/deep` healthy through PgBouncer |
| Temporary host B external IP | Removed after package installation; host B remains reachable through IAP/internal networking |

---

## 6. Non-Claims Preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — re-signoff still requires Tier 2 evidence. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain is still required. |
| **multi-host production HA** | **NO** — manual repeated-drill evidence exists, but automation/fencing, full incident-response signoff, and production posture signoff remain incomplete. |
| **HA-4 automated failover** | **NOT COMPLETE** — this was manual/operator-controlled failover, not automated multi-host failover. |
| **3+ multi-host drills** | **COMPLETE FOR MANUAL PHASE 9 EVIDENCE ONLY** — four manual drills captured; this does not imply automated HA. |
| **sustained SLO window** | **NO** — no 7–30 day observation window. |

---

## 7. Follow-Ups

1. Replace ad-hoc package installation with Cloud NAT, private package mirror, or image-based provisioning for future repeatability.
2. Normalize cross-host PostgreSQL config parity: TLS, HBA ordering, WAL sender/slot limits, and pre-created replication slots.
3. Decide and implement the multi-host automated failover/fencing ADR before any HA-4 completion claim.
4. Investigate Alertmanager systemd inactive/API reachable mismatch and capture human incident-response/paging signoff if production alerting is pursued.
5. Keep Tier 2 blocked until real domain, revalidation, G2 re-signoff, sustained SLO, and final operator signoff complete.

---

*Artifact created: 2026-05-27. Multi-host manual HA evidence captured with conservative non-claims preserved.*
