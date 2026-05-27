# HA Phase 9 Automated Failover/Fencing ADR — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-automated-failover-fencing-adr  
> **Date**: 2026-05-27  
> **Owner**: Engineering + Operator  
> **Scope**: Phase 9 follow-up ADR for automated failover/fencing after manual multi-host drills  
> **Decision**: Select automated detection + operator-confirmed manual promotion as the next safe step; reject automatic promotion without fencing; defer repmgr, Patroni/etcd, managed PostgreSQL HA, and any multi-host automatic promotion until concrete fencing gates pass.  
> **Constraint**: This is an ADR/planning artifact. It does **not** claim HA-4 completion, multi-host automated failover, production HA, production-ready, full G2, Block A closure, or sustained SLO completion.

---

## 1. Decision Summary

Phase 9 now has manual multi-host PostgreSQL failover evidence, including repeated drills and failback. The next safe automation step is **not automatic promotion**. The selected next step is:

```text
automated detection + operator alert + pre-promotion validation
  -> operator confirms old-primary state / fencing decision
  -> operator manually promotes standby and reroutes PgBouncer
```

This preserves the split-brain safety boundary while reducing detection and diagnosis time. It also gives the operator more evidence before adopting a failover manager or consensus stack.

---

## 2. Options Considered

| Option | Decision | Reason |
|--------|----------|--------|
| Automated detection + manual/operator-confirmed promotion | ✅ SELECTED NEXT | Improves detection and operator guidance without creating automatic split-brain risk. Fits the current two-host/same-zone nonprod topology. |
| repmgr + repmgrd | ⏸ DEFERRED | Viable later, but automatic mode without fencing is unsafe. Consider semi-automated detection after manual drills and config parity mature. |
| Patroni + etcd/witness + proxy | ⏸ DEFERRED | Stronger HA path, but requires quorum/witness operations, additional nodes, and routing changes. Too complex for the current step. |
| Managed PostgreSQL HA | ⏸ DEFERRED | Operationally attractive if production posture changes, but it changes the deployment model and does not exercise self-hosted runbooks. |
| Auto-promotion without fencing | ❌ REJECTED | Two-node partition can create two writable primaries. This is worse than downtime. |
| Extend same-VM watchdog directly to multi-host | ❌ REJECTED | Same-VM watchdog does not prove cross-host fencing, STONITH, quorum, or mutual partition safety. |

---

## 3. Selected Immediate Design

The next implementation should be a lightweight detection/alerting workflow, not an automatic promotion workflow.

Minimum behavior:

1. Monitor current primary PostgreSQL readiness from the standby host.
2. Monitor replication lag and last replayed WAL/marker state on the standby.
3. Emit an incident record and alert when the primary appears unavailable.
4. Print/pass a pre-promotion checklist:
   - standby is in recovery before promotion
   - latest RPO marker replayed or lag is within declared bound
   - old primary state is known or explicitly fenced by the operator
   - PgBouncer target update command is ready
5. Require operator confirmation before promotion.

Non-goals for this ADR:

- No automatic `pg_promote()`.
- No unattended PgBouncer reroute.
- No HA-4 completion claim.
- No production HA claim.

Implementation evidence for this selected next step was captured in [`2026-05-27-ha-phase9-watchdog-config-parity-evidence.md`](./2026-05-27-ha-phase9-watchdog-config-parity-evidence.md). The watchdog implementation remains detection-only and does not complete HA-4.

---

## 4. Gates Required Before HA-4 Automated Failover Can Be Checked

| Gate | Requirement | Evidence required |
|------|-------------|-------------------|
| FG-1 — Fencing mechanism selected | Pick a concrete cross-host old-primary fencing method: GCP instance stop, firewall isolation, STONITH-equivalent, or consensus/witness design. | ADR update with operator signoff. |
| FG-2 — Fencing tested | Demonstrate old primary is stopped or provably isolated before standby promotion. | Drill log with timestamps and `pg_is_in_recovery()` / write tests on both hosts. |
| FG-3 — Mutual partition drill | Simulate both nodes being unable to reach each other and prove no two-writer state. | Partition artifact showing exactly one writable primary or no promotion. |
| FG-4 — Config parity validated | TLS, HBA, WAL sender/slot limits, replication users, and PgBouncer routing are pre-staged on both hosts. | Config parity artifact and drill with no certificate/HBA-induced RTO penalty. |
| FG-5 — Routing automation validated | PgBouncer or equivalent routing update is performed safely and repeatably. | Drill log showing bounded routing update and ferrumd readiness recovery. |
| FG-6 — Monitoring/incident workflow complete | Detection, alert, incident log, and operator acknowledgment are captured. | Prometheus/Alertmanager and incident-response evidence. |
| FG-7 — Repeated automated drills | At least 3 automated/fenced drills pass, including failback when practical. | Dated evidence pack with RTO/RPO and split-brain checks per drill. |

---

## 5. Risks and Current Limitations

| Risk | Status | Mitigation |
|------|--------|------------|
| Two-node partition ambiguity | Open | Do not auto-promote until FG-1/FG-2/FG-3 pass. |
| Same-zone correlated failure | Open | Accept for nonprod/operator evidence; production HA requires stronger topology. |
| PgBouncer/ferrumd on host A remains routing/application SPOF | Open | Future ADR may add host B PgBouncer/app endpoint or managed routing. |
| Host config drift | Observed | Normalize TLS/HBA/WAL settings before automation. |
| Alerting service ambiguity | Observed | Investigate Alertmanager systemd inactive/API reachable mismatch before production alerting signoff. |

---

## 6. Non-Claims Preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — re-signoff still requires Tier 2 evidence. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain is still required. |
| **multi-host production HA** | **NO** — manual drills exist, but production HA is not signed off. |
| **HA-4 automated failover** | **NOT COMPLETE** — selected next step explicitly avoids automatic promotion. |
| **sustained SLO window** | **NO** — no 7–30 day observation window. |

---

## 7. Related Evidence

- [`2026-05-27-ha-phase9-multihost-topology-adr.md`](./2026-05-27-ha-phase9-multihost-topology-adr.md)
- [`2026-05-27-ha-phase9-multihost-drill-evidence.md`](./2026-05-27-ha-phase9-multihost-drill-evidence.md)
- [`HA-multi-node-evidence-runbook.md`](./HA-multi-node-evidence-runbook.md)

---

*Artifact created: 2026-05-27. Automated failover/fencing ADR only. No automated HA or production-ready claim.*
