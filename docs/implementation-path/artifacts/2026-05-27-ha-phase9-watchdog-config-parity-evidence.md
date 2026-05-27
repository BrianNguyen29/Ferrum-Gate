# HA Phase 9 Watchdog + Config Parity Evidence — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-watchdog-config-parity-evidence  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: Detection-only/manual-promotion watchdog implementation, PostgreSQL HA config parity normalization, and Alertmanager service/API investigation for the Phase 9 operator environment.  
> **Result**: Detection-only watchdog installed and enabled on both PostgreSQL hosts; watchdog healthy-path and alert-path checks passed without auto-promotion; PostgreSQL TLS/WAL settings normalized; Alertmanager mismatch resolved as service-name mismatch.  
> **Constraint**: This does **not** complete HA-4 automated/fenced failover and does **not** claim production HA, production-ready, full G2, Block A closure, sustained SLO, or final production signoff.

---

## 1. Runtime Scope

| Host | Role during verification | Watchdog local port | Watchdog remote target | Timer state |
|------|--------------------------|---------------------|------------------------|-------------|
| `ferrumgate-nonprod` (`10.0.0.2`) | Primary + PgBouncer + ferrumd | `5433` | `10.0.0.3:5432` | `enabled`, `active` |
| `ferrumgate-pg-ha-b` (`10.0.0.3`) | Standby | `5432` | `10.0.0.2:5433` | `enabled`, `active` |

Installed runtime files on both hosts:

```text
/opt/ferrumgate/scripts/ha-detection-watchdog.sh
/etc/ferrumgate/ha-watchdog.env
/etc/systemd/system/ferrumgate-ha-detection-watchdog.service
/etc/systemd/system/ferrumgate-ha-detection-watchdog.timer
/var/log/ferrumgate/ha-watchdog.log
```

The canonical script has also been added to the repository for repeatability:

```text
scripts/gcp/phase9_ha_detection_watchdog.sh
```

---

## 2. Watchdog Safety Boundary

The watchdog is intentionally detection-only:

- it checks whether the local PostgreSQL node is a standby;
- if the local node is primary, it logs `action=none` and exits successfully;
- if the local node is standby and the remote primary is reachable, it logs healthy state;
- if the local node is standby and the remote primary is unreachable, it logs an `ALERT`, exits `2`, and instructs the operator to confirm fencing before manual promotion;
- it never runs `pg_promote`, never stops a remote host, and never rewrites PgBouncer routing.

This implements the ADR-selected next step: automated detection + operator-confirmed manual promotion, not automatic failover.

---

## 3. Watchdog Verification Evidence

Host A primary-path check:

```text
2026-05-27T16:26:33Z OK local_port=5433 local_role=primary_or_unavailable state=f action=none reason=watchdog_is_detection_only
```

Host B standby healthy-path check:

```text
2026-05-27T16:27:21Z OK local_port=5432 local_role=standby remote=10.0.0.2:5433 remote_state=reachable replay_lag_seconds=0 action=none reason=watchdog_is_detection_only
```

Host B bounded alert-path simulation used an intentionally unreachable remote target (`10.0.0.254:5433`) without stopping the real primary:

```text
2026-05-27T16:27:24Z ALERT local_port=5432 local_role=standby remote=10.0.0.254:5433 remote_state=unreachable replay_lag_seconds=0 action=operator_required next_step=confirm_fencing_then_manual_promote reason=watchdog_never_auto_promotes pg_isready=
RC=2
B_STATE_AFTER_ALERT
t|/etc/ferrumgate/certs/pg-server.crt|/etc/ferrumgate/certs/pg-server.key|512MB|10|10
```

Conclusion: the alert path fired and the standby remained a standby (`pg_is_in_recovery() = t`). No automatic promotion occurred.

---

## 4. PostgreSQL Config Parity Normalization

The following settings were normalized across host A and host B:

| Setting | Host A final | Host B final |
|---------|--------------|--------------|
| `ssl_cert_file` | `/etc/ferrumgate/certs/pg-server.crt` | `/etc/ferrumgate/certs/pg-server.crt` |
| `ssl_key_file` | `/etc/ferrumgate/certs/pg-server.key` | `/etc/ferrumgate/certs/pg-server.key` |
| `wal_keep_size` | `512MB` | `512MB` |
| `max_wal_senders` | `10` | `10` |
| `max_replication_slots` | `10` | `10` |

Host A verification:

```text
f|/etc/ferrumgate/certs/pg-server.crt|/etc/ferrumgate/certs/pg-server.key|512MB|10|10
```

Host B verification:

```text
t|/etc/ferrumgate/certs/pg-server.crt|/etc/ferrumgate/certs/pg-server.key|512MB|10|10
```

HBA symmetry was also improved for cross-host replication and future failback. This evidence does not claim final production security posture; it records the operator-environment HA drill configuration only.

---

## 5. Alertmanager Service/API Investigation

Initial check used the wrong unit name:

```text
Unit alertmanager.service could not be found.
```

The actual unit is `prometheus-alertmanager.service`, and it was active:

```text
SYSTEMD_UNITS
prometheus-alertmanager.service loaded active running Alertmanager for prometheus

ALERT_SERVICE
active
OK
```

Conclusion: the earlier service/API mismatch was caused by checking `alertmanager.service` instead of the distro unit name `prometheus-alertmanager.service`. Alertmanager API readiness returned `OK`.

---

## 6. Final HA State After Watchdog/Parity Work

```text
FINAL_A
enabled
active
f|0/17000148
16/main|10.0.0.3|streaming
{"status":"ok","healthy":true,"components":[{"component":"store","status":"ok","healthy":true},{"component":"write_queue","status":"ok: depth=0, threshold=100","healthy":true},{"component":"pool","status":"ok: idle=1/total=2/max=10","healthy":true}]}

FINAL_B
enabled
active
t|0/17000000|0/17000148
```

Interpretation:

- Host A remains primary.
- Host B remains standby and streams from host A.
- Both watchdog timers are enabled and active.
- ferrumd remains healthy through PgBouncer.

---

## 7. Non-Claims Preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — re-signoff still requires Tier 2 evidence. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain is still required. |
| **multi-host production HA** | **NO** — manual drills and detection-only watchdog exist, but production HA is not signed off. |
| **HA-4 automated failover** | **NOT COMPLETE** — watchdog does not promote or fence; automated/fenced drills still required. |
| **sustained SLO window** | **NO** — no 7–30 day observation window. |

---

## 8. Remaining Follow-Ups

1. Implement and test a real fencing mechanism before any automatic promotion.
2. Run automated/fenced drills only after fencing gates pass.
3. Add human incident-response / paging-delivery signoff if production alerting is pursued.
4. Keep Tier 2 blocked until real domain, revalidation, G2 re-signoff, sustained SLO, and final operator signoff complete.

---

*Artifact created: 2026-05-27. Detection-only watchdog and config parity evidence only. No automated HA or production-ready claim.*
