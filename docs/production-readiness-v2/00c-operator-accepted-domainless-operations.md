# 00c — Operator-Accepted Domainless Operations

> **Status**: Posture declaration. Not a tier. Not production-ready.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-28
> **Parent**: [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md)
> **Waiver basis**: [`docs/implementation-path/artifacts/2026-05-28-delegated-ship-fast-waiver-signoff.md`](../implementation-path/artifacts/2026-05-28-delegated-ship-fast-waiver-signoff.md)

---

## Goal

Define the canonical `Operator-Accepted Domainless Operations` posture: a non-tier acceptance state that exists alongside the tiered readiness model, recording what the operator has explicitly accepted under domainless conditions and what remains deferred or waived.

This posture is **not** Tier 1.75, Tier 2, or any new tier. It is a posture declaration that captures operator acceptance of rehearsal evidence and deferred items while preserving all existing non-claims.

---

## What this posture means

- The operator has reviewed the existing Tier 1 and Tier 1.5 evidence and elected to accept rehearsal/dry-run evidence as sufficient for immediate planning and procedural validation.
- This posture does **not** advance the tier model. Tier 1 and Tier 1.5 remain COMPLETE / ACKNOWLEDGED. Tier 2 remains gated.
- All non-claims from `00a-domainless-readiness-tier.md`, `00b-tier-1.5-domainless-infrastructure.md`, and `00-scope-and-nonclaims.md` remain preserved unchanged.

---

## Allowed claims (what this posture explicitly claims)

| Claim | Basis | Evidence |
|-------|-------|----------|
| **Operator acceptance recorded** | Operator explicitly authorized delegated signoff for rehearsal documentation on 2026-05-28 | [`2026-05-28-delegated-ship-fast-waiver-signoff.md`](../implementation-path/artifacts/2026-05-28-delegated-ship-fast-waiver-signoff.md) |
| **Tier 1 complete / acknowledged** | B+C+HA-B engineering evidence complete; operator acknowledgment recorded 2026-05-26 | [`2026-05-26-domainless-tier1-operator-acknowledgment.md`](../implementation-path/artifacts/2026-05-26-domainless-tier1-operator-acknowledgment.md) |
| **Tier 1.5 complete / acknowledged** | PostgreSQL target deployment + same-VM HA topology + same-VM automated failover evidence complete; operator acknowledgment recorded 2026-05-27 | [`2026-05-27-tier-1-5-operator-acknowledgment.md`](../implementation-path/artifacts/2026-05-27-tier-1-5-operator-acknowledgment.md) |
| **Rehearsal evidence exists** | Bounded dry-run rehearsal, local HA rehearsal, and local ferrumd reconnect rehearsal passed | [`2026-05-28-slo-dry-run-rehearsal-evidence.md`](../implementation-path/artifacts/2026-05-28-slo-dry-run-rehearsal-evidence.md), [`2026-05-28-ha-local-rehearsal-and-script-fix-evidence.md`](../implementation-path/artifacts/2026-05-28-ha-local-rehearsal-and-script-fix-evidence.md) |
| **D.2 TUI MVP implemented** | `bins/ferrum-tui/` operator dashboard for health/readiness/deep probes exists under domainless/waiver scope | `bins/ferrum-tui/README.md` |
| **D.3 Terraform single-node module implemented** | `deploy/terraform/ferrumgate-single-node/` local artifact generator exists using `local_file` + `null_resource`; no cloud credentials | `deploy/terraform/ferrumgate-single-node/` |

---

## Forbidden claims (what this posture explicitly does NOT claim)

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — This posture is not production-ready. |
| **Tier 2** | **NOT ATTAINED** — Tier 2 requires real domain + revalidation + sustained SLO window + full G2 + operator final signoff. |
| **full G2** | **NOT COMPLETE** — G2.1–G2.8 remain signed for conditional pilot only. |
| **Block A** | **WAIVED/CONDITIONAL** — Real owned domain still required for Tier 2. |
| **HA-4 unattended automated failover** | **NOT COMPLETE** — Only operator-controlled fenced drills and detection-only watchdog exist. |
| **Sustained SLO observation window** | **NOT COMPLETE** — Only bounded dry-run rehearsal exists (~4 min, 5 samples). |
| **Multi-host production HA** | **NOT COMPLETE** — Same-VM HA evidence exists; multi-host production HA does not. |
| **D.2 web dashboard** | **DEFERRED** — TUI MVP only; web dashboard remains P2 deferred per `06-admin-operator-ux-plan.md`. |
| **D.3 cloud provider modules** | **DEFERRED** — Local Terraform artifact generator only; Pulumi and cloud provider modules remain deferred. |

---

## Relation to the tier model

```
Tier 0 (RC-ready/conditional) ──► Tier 1 (domainless production-candidate) ──► Tier 1.5 (domainless production infrastructure)
                                       ▲                                              ▲
                                       │                                              │
                    Operator-Accepted Domainless Operations posture runs PARALLEL to tiers;
                    it does not create a new tier or modify tier boundaries.
```

- **Does not insert a new subtier**: There is no Tier 1.75. The tier model in `00a-domainless-readiness-tier.md` remains unchanged.
- **Does not alter Tier 1 or Tier 1.5 status**: Both remain COMPLETE / ACKNOWLEDGED with all original non-claims preserved.
- **Does not advance toward Tier 2**: All Tier 2 gates (real domain, sustained SLO, full G2 re-signoff, operator final signoff) remain in place.
- **Captures operator risk acceptance**: This posture records that the operator has reviewed deferred items and accepted the current state as sufficient for planning, while explicitly preserving what is NOT COMPLETE.

---

## Active opened work D.2 / D.3

| Item | Status | Rationale | Next Action |
|------|--------|-----------|-------------|
| **D.2 — Web admin dashboard / TUI** | **PARTIAL / OPEN** | TUI MVP (`ferrum-tui`) implemented under domainless/waiver scope; web dashboard remains P2 deferred. Operator acceptance covers TUI MVP only. | Engineering: maintain TUI MVP; web dashboard deferred until operator requests it or Tier 2 planning begins. |
| **D.3 — Terraform single-node module** | **PARTIAL / OPEN** | Local artifact generator (`deploy/terraform/ferrumgate-single-node/`) implemented using `local_file` + `null_resource`; no cloud credentials required; not production-ready. Pulumi and cloud provider modules remain deferred. | Engineering: maintain local module; cloud provider expansion deferred until operator cluster target is defined. |

Both D.2 and D.3 are explicitly tracked as **opened work** in this posture. Their partial implementation is accepted under the delegated ship-fast waiver; full completion is not claimed.

---

## Non-claims summary

- **NOT production-ready**.
- **NOT Tier 2**.
- **NOT full G2**.
- **NOT Block A closure**.
- **NOT HA-4 completion**.
- **NOT sustained SLO window**.
- **NOT multi-host production HA**.
- **NOT a committed timeline** for deferred items.
- **NOT a waiver of future evidence requirements** — all waivers are risk acceptance, not substitution for missing evidence.

---

## Related docs

- [`00a-domainless-readiness-tier.md`](./00a-domainless-readiness-tier.md) — Canonical tiered readiness model.
- [`00b-tier-1.5-domainless-infrastructure.md`](./00b-tier-1.5-domainless-infrastructure.md) — Tier 1.5 canonical definition.
- [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md) — Scope boundaries and master non-claims table.
- [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md) — Blocker status; D.2/D.3 tracked in deferred table.
- [`06-admin-operator-ux-plan.md`](./06-admin-operator-ux-plan.md) — D.2 TUI MVP and web dashboard deferral.
- [`docs/implementation-path/artifacts/2026-05-28-delegated-ship-fast-waiver-signoff.md`](../implementation-path/artifacts/2026-05-28-delegated-ship-fast-waiver-signoff.md) — Delegated ship-fast waiver signoff.
- [`docs/implementation-path/01-current-state.md`](../implementation-path/01-current-state.md) — Current state with Tier 1 / Tier 1.5 status.

---

*End of file — Operator-Accepted Domainless Operations posture. Not a tier. Not production-ready. All non-claims preserved.*
