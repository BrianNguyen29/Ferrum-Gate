# HA Phase 9 Multi-Host Topology ADR — 2026-05-27

> **Artifact ID**: 2026-05-27-ha-phase9-multihost-topology-adr
> **Date**: 2026-05-27
> **Owner**: Engineering + Operator
> **Scope**: Phase 9 follow-up ADR/topology choice for future multi-host/operator-environment HA evidence
> **Decision**: Select the simplest viable multi-host starting topology: two independent PostgreSQL hosts using streaming replication, PgBouncer routing to the current primary, and manual/operator-controlled failover drills before any multi-host automated failover claim.
> **Constraint**: This is a planning/topology ADR only. It does not claim multi-host production HA, HA-4/HA-5 completion, Tier 2 production-ready, full G2, Block A closure, or a sustained SLO observation window.

---

## 1. Decision Summary

Phase 9 will start from a **two independent VM/host PostgreSQL primary/standby topology** that extends the proven Tier 1.5 same-VM design to separate host failure domains.

The selected starting topology is:

```text
ferrumd
  |
  v
PgBouncer / routing endpoint
  |
  +--> PostgreSQL host A (primary or standby)
  |
  +--> PostgreSQL host B (standby or primary)

PostgreSQL streaming replication: current primary -> current standby
Failover mode for first evidence pass: manual/operator-controlled promotion and PgBouncer reroute
```

This ADR chooses **manual multi-host failover evidence first**. Multi-host automated failover remains deferred until the operator has passed manual multi-host drills and selected a concrete fencing/STONITH or consensus design.

---

## 2. Why This Topology

| Reason | Explanation |
|--------|-------------|
| Reuses proven Tier 1.5 components | Tier 1.5 already validated PostgreSQL streaming replication, PgBouncer routing, same-VM promotion, RTO/RPO measurement, and no split-brain checks. |
| Smallest new variable | The main new variable is independent host failure domains; no new consensus stack is introduced yet. |
| Operator-friendly | Manual failover drills are easier to reason about and review before automated failover is introduced. |
| Avoids premature complexity | Patroni, repmgr, etcd, HAProxy/VIP, and STONITH require additional operator maturity and infrastructure. |
| Preserves safety | Automated failover without reliable cross-host fencing can create split-brain, which is worse than manual failover. |

---

## 3. Options Considered

| Option | Decision | Reason |
|--------|----------|--------|
| **Two independent VMs + streaming replication + PgBouncer + manual failover** | ✅ SELECTED FOR PHASE 9 START | Simplest viable multi-host progression from Tier 1.5; enables real RPO/RTO and split-brain evidence without new orchestration stack. |
| Managed PostgreSQL HA | ⏸ DEFERRED | Valid if operator chooses managed PG later, but it changes deployment model and does not exercise self-hosted multi-host runbooks. |
| Patroni + etcd + HAProxy/VIP | ⏸ DEFERRED | Stronger automated HA path, but requires consensus cluster and fencing discipline; premature before manual multi-host drills. |
| repmgr/repmgrd | ⏸ DEFERRED | Viable candidate for later automation, but adds a new failover manager without current FerrumGate evidence. |
| Extend same-VM custom watchdog directly to multi-host | ❌ REJECTED FOR NOW | Same-VM watchdog does not solve network partition or cross-host fencing. It must not be promoted to multi-host automation without a fencing ADR. |
| Read replicas for query scaling | ⏸ DEFERRED TO SEPARATE ADR | Requires Strategy A (proxy routing) or Strategy B (dual DSN/read pool) decision and endpoint audit. |

---

## 4. Minimum Future Evidence Gates

These gates are required before Phase 9 HA can be marked complete.

| Gate | Requirement | Acceptance criteria |
|------|-------------|---------------------|
| **MH-G1 — Topology deployed** | At least two independent PostgreSQL hosts/VMs with streaming replication. | `pg_stat_replication` shows streaming; `pg_is_in_recovery()` returns false on primary and true on standby; hosts have independent failure domains. |
| **MH-G2 — Manual multi-host failover drill** | Execute operator-controlled primary failure, standby promotion, PgBouncer reroute, ferrumd smoke test, old-primary fencing check. | Drill log with timestamps; exactly one writable primary; ferrumd readyz/deep 200 after reroute; RTO/RPO captured. |
| **MH-G3 — Network partition drill** | Simulate primary unreachable via network/firewall path, not just process stop. | Operator confirms old primary is fenced or isolated before promotion; no split-brain observed. |
| **MH-G4 — Multi-drill consistency** | At least three successful drills, including both directions when practical. | 3+ pass artifacts; RPO within declared bound; zero split-brain events. |
| **MH-G5 — RPO/RTO measurement log** | Record timestamps and RPO marker/row evidence for every drill. | Published dated artifact with RTO and RPO per drill. |
| **MH-G6 — Data consistency checks** | Verify FerrumGate table row counts and key integrity after failover. | Row counts/content checks pass except any explicitly documented async-replication loss window. |
| **MH-G7 — Post-failover operational validation** | Confirm backup, monitoring, ferrumd smoke, and incident/audit recording after each drill. | Checklist complete in evidence artifact. |

---

## 5. Phase 9 Execution Plan

1. Provision two independent PostgreSQL hosts/VMs for the operator environment.
2. Configure PostgreSQL 16 streaming replication across hosts.
3. Configure PgBouncer/routing endpoint to target the current primary.
4. Run baseline validation:
   - primary writable
   - standby read-only
   - replication streaming
   - replication lag measured
   - ferrumd `/v1/readyz/deep` 200 through PgBouncer
5. Execute manual multi-host failover drill using `HA-multi-node-evidence-runbook.md`.
6. Execute network-partition drill.
7. Repeat until at least 3 passing drills are captured.
8. Publish dated evidence pack with drill logs, RTO/RPO, split-brain checks, and consistency results.
9. Only after manual multi-host drills pass, decide whether to draft a new ADR for Patroni, repmgr, or another automated failover/fencing mechanism.

---

## 6. Deferred Follow-Up ADRs

| Follow-up | Trigger |
|-----------|---------|
| Automated failover/fencing ADR | After manual multi-host drills pass and operator selects automation maturity target. |
| Read replica implementation ADR | Before routing read-only ferrumd traffic to replicas; must select proxy strategy vs dual-DSN strategy. |
| PG-2.3b circuit-breaker ADR | When a concrete load-balanced topology exists and half-open/fail-fast semantics can be tested. |
| Managed PostgreSQL HA ADR | If operator decides to migrate from self-hosted PostgreSQL to managed HA PostgreSQL. |

---

## 7. Operator Authorization

- **Authorized by**: BrianNguyen
- **Date**: 2026-05-27
- **Method**: Explicit authorization via task instruction: `Thực hiện Phase 9 ADR/topology choice follow-up cho multi-host/operator-environment HA evidence.`
- **Scope limitation**: Authorization covers this planning/topology ADR only. It does not authorize claims of multi-host production HA, Tier 2 production-ready, full G2 completion, Block A closure, sustained SLO completion, HA-4/HA-5 completion, or final production signoff.

---

## 8. Non-Claims Preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Tier 2 remains gated. |
| **full G2** | **NOT COMPLETE** — re-signoff still requires Tier 2 evidence. |
| **Block A** | **WAIVED/CONDITIONAL** — real owned domain is still required. |
| **multi-host production HA** | **NO** — topology selected, but not deployed/proven. |
| **HA-4 automated failover** | **NOT COMPLETE** — multi-host automation/fencing evidence does not exist. |
| **HA-5 RPO/RTO operator-environment evidence** | **NOT COMPLETE** — future multi-host drill logs required. |
| **sustained SLO window** | **NO** — available SLO evidence is bounded/canonical, not 7–30 days. |

---

## 9. Related Docs

- [`docs/production-readiness-v2/ha-adr.md`](../../production-readiness-v2/ha-adr.md) — Original HA phased strategy ADR.
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — HA roadmap and current Phase 9 status.
- [`HA-multi-node-evidence-runbook.md`](./HA-multi-node-evidence-runbook.md) — Operator drill procedure and evidence capture template.
- [`TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md) — Evidence pack template for future real drills.
- [`2026-05-27-ha-phase9-prerequisites-unblocked.md`](./2026-05-27-ha-phase9-prerequisites-unblocked.md) — Phase 9 prerequisite unblock notice.
- [`2026-05-27-ha-automated-failover-signoff.md`](./2026-05-27-ha-automated-failover-signoff.md) — Tier 1.5 same-VM automated failover evidence; not multi-host HA.

---

*Artifact created: 2026-05-27. Phase 9 multi-host topology ADR only. No multi-host HA or production-ready claim.*
