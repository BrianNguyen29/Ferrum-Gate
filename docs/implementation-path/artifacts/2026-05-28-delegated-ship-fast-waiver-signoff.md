# Delegated Ship-Fast Waiver Signoff — 2026-05-28

> **Artifact ID**: 2026-05-28-delegated-ship-fast-waiver-signoff
> **Date**: 2026-05-28
> **Owner**: Engineering + Operator (delegated authority)
> **Scope**: Risk-acceptance waiver for shipping rehearsal evidence without waiting for full sustained-window or unattended-automated-failover completion.
> **Parent**: [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md)

---

## 1. Executive Summary

This artifact records the operator-delegated decision to accept the current rehearsal/dry-run evidence as sufficient for the immediate scope, while explicitly preserving the open items that remain NOT COMPLETE.

The following items are **USER-WAIVED / NOT COMPLETE**:

| Item | Waiver Rationale | Required Before Production Claim |
|------|-----------------|----------------------------------|
| HA-4 unattended automated failover | Only operator-controlled fenced drills exist; no unattended automation | External endpoint cutover strategy + repeated unattended drills + incident-response signoff |
| Sustained SLO observation window | Only bounded dry-run rehearsal exists (5 samples, ~4 min) | 7–30 day rolling observation window with operator review |
| Multi-host production HA | Manual drills exist; automated failover incomplete | Full fenced failover automation + production signoff |
| Tier 2 / production-ready / full G2 | Block A remains open; sustained window missing | Real domain + sustained SLO + operator final signoff |

---

## 2. Waiver Details

### 2.1 HA-4 Unattended Automated Failover — USER-WAIVED / NOT COMPLETE

- **What exists**: Local HA simulation (primary/standby streaming replication), local failover drill (RTO 3–4 s, RPO 0), local ferrumd reconnect drill (app-level RTO 4 s), multi-host manual drills (4 drills, RTO 22–246 s), bounded operator-controlled fenced drill with host B redundancy (RTO 69 s), detection-only watchdog, GCP fencing mechanism script.
- **What is missing**: Unattended automated promotion without operator confirmation; external endpoint cutover; repeated unattended drills; full incident-response signoff.
- **Waiver authority**: Operator explicitly accepts that HA-4 will remain open until external routing and operator incident-response procedures are defined.

### 2.2 Sustained SLO Observation Window — USER-WAIVED / NOT COMPLETE

- **What exists**: Bounded dry-run rehearsal (`make slo-sustained-dry-run`, 5 samples, 100% availability, avg latency 42 ms, ~4 min duration).
- **What is missing**: 7-day or 30-day rolling observation window; real traffic validation; operator-reviewed sustained evidence.
- **Waiver authority**: Operator accepts bounded rehearsal as sufficient for procedural validation, with explicit understanding that sustained-window evidence is still required before any production SLO claim.

### 2.3 Multi-Host Production HA — USER-WAIVED / NOT COMPLETE

- **What exists**: Same-VM Tier 1.5 HA topology and automated failover evidence; two-independent-host manual drills; host B redundancy drill; fencing mechanism script.
- **What is missing**: Unattended automated failover across independent hosts; production routing strategy; production operator signoff.
- **Waiver authority**: Operator accepts current evidence as planning/validation baseline, not production HA completion.

---

## 3. Signoff Block

| Field | Value |
|-------|-------|
| **Signed by** | BrianNguyen (session authorization) |
| **Date** | 2026-05-28 |
| **Scope** | Delegated ship-fast waiver for rehearsal evidence only |
| **Nature** | Risk acceptance / waiver. Not evidence completion. Not production-ready signoff. |
| **Preserved non-claims** | HA-4 unattended automated failover = NOT COMPLETE; sustained SLO window = NOT COMPLETE; multi-host production HA = NOT COMPLETE; Tier 2 / full G2 = NOT COMPLETE; Block A = WAIVED/CONDITIONAL |
| **Authority** | User explicitly authorized delegated signoff for planning and rehearsal documentation. |

---

## 4. Related Artifacts

- [`2026-05-28-slo-dry-run-rehearsal-evidence.md`](./2026-05-28-slo-dry-run-rehearsal-evidence.md) — SLO sustained dry-run rehearsal evidence
- [`2026-05-28-ha-local-rehearsal-and-script-fix-evidence.md`](./2026-05-28-ha-local-rehearsal-and-script-fix-evidence.md) — HA local rehearsal and script fix evidence
- [`2026-05-27-ha-phase9-automated-failover-fencing-adr.md`](./2026-05-27-ha-phase9-automated-failover-fencing-adr.md) — ADR selecting detection-only/manual promotion
- [`2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md`](./2026-05-27-ha-phase9-host-b-redundancy-fenced-drill-evidence.md) — host B redundancy bounded fenced drill
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — HA roadmap with preserved gaps
- [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md) — blockers plan

---

## 5. Non-Claims

- **NOT production-ready**: This waiver does not make FerrumGate production-ready.
- **NOT full G2**: Full G2 requires Block A closure + sustained SLO window + unattended automated failover + operator final signoff.
- **NOT evidence completion**: Waiver is risk acceptance, not substitution for missing evidence.
- **NOT a committed timeline**: Future evidence dates are operator-dependent.

---

*Artifact created: 2026-05-28. Delegated ship-fast waiver signoff. No production-ready claim.*
