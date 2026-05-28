# FerrumGate Non-Claims and Readiness Boundaries

> **Status**: Canonical non-claims and readiness boundary document.
> **Owner**: Operator
> **Last updated**: 2026-05-28
> **Parent**: [`docs/plan.md`](../plan.md)

---

## 1. Purpose

This document is the **canonical, centralized source** for FerrumGate non-claims and readiness boundaries. It exists to prevent accidental or implicit overclaiming and must be referenced by any public-facing or internal document that discusses production status, tiering, or readiness.

**Rule of thumb**: If a statement implies production readiness, Tier 2 completion, GA, or enterprise certification without qualification, it violates this document and must be corrected.

---

## 2. Current-State Summary

| Boundary | Status | Notes |
|----------|--------|-------|
| `production-ready` | **NO** | FerrumGate is not production-ready. |
| `Tier 2` | **NOT COMPLETE** | Tier 1 and Tier 1.5 acknowledged; Tier 2 not achieved. |
| `full G2` | **NOT COMPLETE** | Conditional signoff only; full G2 closure pending. |
| Real owned domain / public endpoint | **MISSING** | All current SLO evidence is **domainless** only. |
| `Block A` | **WAIVED / CONDITIONAL** | Accepted for pilot; not closed for production. |
| `HA-4` unattended automated failover | **NOT COMPLETE** | Manual / operator-controlled failover only. |
| Sustained SLO window (7–30 days) | **NOT COMPLETE** | No completed sustained window evidence yet. |
| Multi-host production HA | **NO / NOT COMPLETE** | Same-VM topology evidence exists; multi-host production HA does not. |
| D.2 Web dashboard | **DEFERRED** | TUI/operator console is a read-only convenience only. |
| D.3 Cloud / Pulumi provider modules | **DEFERRED** | Local Terraform artifact generator exists; cloud modules deferred. |

---

## 3. What FerrumGate Is Allowed to Claim

FerrumGate may truthfully claim the following when stated with appropriate qualifiers:

- **Self-hosted execution-governance gateway for MCP / agentic operations** — this is the core positioning.
- **Domainless Tier 1 / Tier 1.5 operator-accepted evidence** exists (if documented elsewhere), but **not** Tier 2 or production-ready status.
- **Read-only operator TUI convenience** and supporting docs / tooling.
- **Policy-evaluated, capability-bounded, approval-aware, rollback-classified, provenance-tracked execution lifecycle** as a design goal and partial implementation.

---

## 4. What FerrumGate Must Not Claim Without Future Evidence

Unless accompanied by explicit evidence artifacts and signoff, FerrumGate must **never** claim:

- Production-ready / Tier 2 / GA / enterprise-ready status.
- Full G2 complete.
- Real domain-backed production validation.
- Unattended automated failover (`HA-4`).
- Sustained SLO closure (7–30 days).
- Multi-tenant SaaS capability.
- OIDC / SSO / compliance suite completeness.
- SOC 2, ISO 27001, or any formal compliance certification.

---

## 5. Required Qualifier Language

### Safe (required)

| Phrase | Context |
|--------|---------|
| `production-ready = NO` | Any readiness summary |
| `Tier 2 = NOT COMPLETE` | Tier discussions |
| `domainless production-candidate only` | SLO or deployment evidence |
| `operator-accepted domainless operations` | Tier 1 / 1.5 references |
| `WAIVED / CONDITIONAL` | Block A and similar exceptions |
| `NOT COMPLETE` | HA-4, sustained SLO, full G2 |

### Unsafe (prohibited without explicit future evidence)

| Phrase | Why it is unsafe |
|--------|------------------|
| `production-ready` | Unqualified; implies Tier 2 + domain + SLO |
| `GA` | Unqualified; implies general availability |
| `enterprise-ready` | Unqualified; implies compliance + HA + support |
| `Tier 2 complete` | Without qualifier; current status is NOT COMPLETE |
| `fully automated failover` | HA-4 is NOT COMPLETE |
| `production validated` | Without domain and sustained window evidence |

---

## 6. Evidence and Reference Links

| Document | Purpose |
|----------|---------|
| [`../production-readiness-v2/00-scope-and-nonclaims.md`](../production-readiness-v2/00-scope-and-nonclaims.md) | Original scope and non-claims for the production-readiness path |
| [`../production-readiness-v2/00a-domainless-readiness-tier.md`](../production-readiness-v2/00a-domainless-readiness-tier.md) | Tier 1 domainless readiness definition |
| [`../production-readiness-v2/00c-operator-accepted-domainless-operations.md`](../production-readiness-v2/00c-operator-accepted-domainless-operations.md) | Operator-accepted domainless operations criteria |
| [`../implementation-path/artifacts/2026-05-28-phase0.1-overclaim-audit-evidence.md`](../implementation-path/artifacts/2026-05-28-phase0.1-overclaim-audit-evidence.md) | Phase 0.1 overclaim audit evidence |
| [`../plan.md`](../plan.md) | Strategic execution checklist and roadmap |

---

## 7. Next Review Trigger

This document must be reviewed and updated when **any** of the following occur:

1. Real domain validation is performed and evidence is produced.
2. Sustained SLO window (7–30 days) evidence is completed.
3. Full G2 re-signoff is obtained.
4. `HA-4` unattended automated failover evidence is produced.
5. Tier 2 signoff is obtained.
6. Any new deferred item becomes implemented, or any implemented item is removed.

Until then, the current state stands and all claims must remain qualified per Section 5.

---

*End of canonical non-claims document.*
