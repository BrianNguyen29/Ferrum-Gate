# HA Automated Failover Design — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-automated-failover-design
> **Date**: 2026-05-27
> **Owner**: Engineering
> **Scope**: Tier 1.5 Batch 3 — automated failover design for HA-A.1 through HA-A.5
> **Constraint**: Same-VM topology only. No production-ready or multi-host production HA claim.

---

## 1. Summary

The selected Batch 3 design is a custom same-VM watchdog plus PgBouncer `RECONNECT`. Patroni and repmgr were rejected for the current same-VM topology because they add consensus/witness dependencies that provide no value when both PostgreSQL instances share the same VM fate.

---

## 2. Design Decision

| Option | Decision | Reason |
|--------|----------|--------|
| Patroni | Rejected | Requires external consensus; overkill for same-VM topology. |
| repmgr/repmgrd | Rejected | Safe automatic failover requires witness/monitor node; outside same-VM scope. |
| Custom watchdog | Selected | Can fence primary with `systemctl stop`, promote standby, switch PgBouncer, and preserve ferrumd process continuity. |

---

## 3. Watchdog Sequence

1. Capture start timestamp and ferrumd PID.
2. Verify standby is reachable and in recovery.
3. Inject or detect primary failure.
4. Fence primary by stopping its systemd service.
5. Verify primary port is closed and cannot accept writes.
6. Promote standby using PostgreSQL superuser.
7. Wait for `pg_is_in_recovery()` to return false on the promoted node.
8. Rewrite PgBouncer backend to the promoted node.
9. Reload PgBouncer and run `RECONNECT`.
10. Poll ferrumd `/v1/readyz/deep` until HTTP 200.
11. Record RTO and verify RPO marker row exists.
12. Write failover log and sentinel to prevent failover loops.

---

## 4. Non-Claims

- **production-ready = NO**.
- **full G2 = NOT COMPLETE**.
- **Block A = WAIVED/CONDITIONAL**.
- **multi-host production HA = NO** — this remains same-VM only.
- **Tier 2 production-ready = NO** — real domain and Tier 2 revalidation are still required.

---

## 5. Related Artifacts

- [`2026-05-27-ha-automated-failover-drill-evidence.md`](./2026-05-27-ha-automated-failover-drill-evidence.md) — Automated failover drill evidence.
- [`2026-05-27-ha-automated-failover-signoff.md`](./2026-05-27-ha-automated-failover-signoff.md) — Batch 3 consolidated signoff.
- [`2026-05-27-ha-multinode-topology-signoff.md`](./2026-05-27-ha-multinode-topology-signoff.md) — Batch 2 HA topology evidence.

---

*Artifact created: 2026-05-27. Automated failover design. No production-ready claim.*
