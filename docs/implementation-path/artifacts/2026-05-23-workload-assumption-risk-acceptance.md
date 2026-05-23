# Workload Assumption Risk Acceptance — 2026-05-23

> **Artifact ID**: 2026-05-23-workload-assumption-risk-acceptance
> **Date**: 2026-05-23
> **Owner**: Operator (BrianNguyen)
> **Scope**: Single-node SQLite conditional pilot only. Does not extend to production-ready, full G2, PostgreSQL production, or HA/multi-node.
> **Constraint**: Docs-only. No code changes. No production-ready claim. No final signoff. Receipt and risk acknowledgment only.

---

## 1. Purpose

This artifact records that the operator (BrianNguyen) has reviewed the workload model refresh evidence and explicitly accepts the risk that the ≤300 writes/s signed assumption was never validated.

This is **not** a production-ready signoff, full G2 closure, or validation of capacity. It is a conservative risk acceptance for the conditional single-node SQLite pilot scope, pending future load testing.

---

## 2. Non-Claims

| Non-Claim | Status |
|-----------|--------|
| **300 writes/s is validated** | **NO** — never tested |
| **Production-ready** | **NO** |
| **Full G2 closed** | **NOT COMPLETE** |
| **Block A closed** | **WAIVED/CONDITIONAL** |
| **PostgreSQL production** | **NO** |
| **HA/multi-node** | **NO** |
| **This is final signoff** | **NO — receipt and risk acknowledgment only** |

---

## 3. Background

### 3.1 Original Signed Assumption (2026-05-09)

Source: [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) §1, §5

| Parameter | Signed Assumption |
|-----------|-------------------|
| Sustained write rate | ≤300 writes/s |
| Peak write rate | ≤300 writes/s |
| Daily write volume | ≤1,000,000 writes/day |
| SQLite single-node fit | CONFIRMED by operator (BrianNguyen, 2026-05-09) |

### 3.2 Workload Model Refresh Evidence (2026-05-22)

Source: [`docs/implementation-path/artifacts/2026-05-22-workload-model-refresh-evidence.md`](./2026-05-22-workload-model-refresh-evidence.md)

| Finding | Value |
|---------|-------|
| Original assumption | ≤300 writes/s |
| Highest observed target-host sustained rate | Far below 300 writes/s — canonical run served 2,380 requests total across all phases, zero errors |
| Highest observed local stress rate | ~258 RPS (in-memory SQLite, auth disabled; not representative) |
| 300 writes/s ceiling tested | **NEVER** |
| Recommended safe limits (engineering) | ≤10 req/s sustained, ≤50 req/s burst (conservative; not validated) |

**Key finding**: The signed 300 writes/s ceiling was a capacity planning assumption. No load test ever approached it. This artifact documents that gap and records operator acceptance of that untested risk.

---

## 4. Risk Acceptance

### 4.1 Operator Risk Acknowledgment

> **Operator**: BrianNguyen
> **Date**: 2026-05-23
> **Scope**: Single-node SQLite conditional pilot — `ferrumgate.duckdns.org` on GCP VM `ferrumgate-nonprod`
> **Not covered**: Production-ready, full G2, PostgreSQL production, HA/multi-node, any scope beyond single-node SQLite pilot

BrianNguyen acknowledges the following:

1. The ≤300 writes/s signed assumption was **never tested**. All observed target-host runs were far below that ceiling.

2. The engineering-recommended safe limits (≤10 req/s sustained, ≤50 req/s burst) are **conservative guesses**, not validated capacity ceilings.

3. Operating within these limits does **not** constitute a validated capacity claim. Real load testing is required to establish any validated ceiling.

4. This risk acceptance applies **only** to the current single-node SQLite conditional pilot scope. Any future expansion (PostgreSQL, multi-node, higher throughput) requires fresh risk acceptance.

5. This artifact is **not** a production-ready signoff, not a full G2 re-signoff, and does not close Block A.

---

## 5. Operator Decision

| Statement | Operator Accepts |
|-----------|-----------------|
| 300 writes/s assumption was never validated | ✅ Accepted as conditional-pilot risk |
| Engineering safe-limit recommendations (≤10 req/s sustained, ≤50 req/s burst) are accepted as interim conservative guidance pending load testing | ✅ Accepted |
| Real load testing is required before any throughput claim | ✅ Acknowledged |
| This risk acceptance does not extend beyond single-node SQLite conditional pilot | ✅ Acknowledged |
| Block A (real owned domain) remains open; DuckDNS is conditional pilot only | ✅ Acknowledged |

---

## 6. Conditions for Future Risk Acceptance Expansion

The following would require a new or updated risk acceptance artifact:

| Condition | Owner | Action |
|-----------|-------|--------|
| Expected workload exceeds ≤10 req/s sustained or ≤50 req/s burst | Operator | Commission load test or revise safe limits |
| Migration to PostgreSQL | Operator | New risk acceptance covering PG workload model |
| Multi-node or HA deployment | Operator | New risk acceptance covering HA topology |
| Real owned domain acquisition | Operator | Block A closure artifact + new risk acceptance |
| Any claim of validated capacity above current safe limits | Engineering + Operator | Load test evidence artifact |

---

## 7. Relationship to Other Artifacts

| Document | Relationship |
|----------|--------------|
| [`2026-05-22-workload-model-refresh-evidence.md`](./2026-05-22-workload-model-refresh-evidence.md) | Source evidence for this risk acceptance |
| [`2026-05-23-operator-review-packet.md`](./2026-05-23-operator-review-packet.md) | Bundled with operator review packet; RN-4 and D-2 in that packet reference this risk acceptance |
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase 2 evidence checklist; P1/P2 acknowledgment |
| [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) | NO→YES completion map; G2.1 re-signoff prerequisites |
| [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md) | Scope boundaries and non-claims |

---

## 8. Acknowledgment

| Role | Name | Date | Ack |
|------|------|------|-----|
| Engineering | | | |
| Operator | BrianNguyen | 2026-05-23 | Acknowledged — receipt and risk acceptance only; not final signoff |

---

*Artifact created: 2026-05-23. Workload assumption risk acceptance — receipt and risk acknowledgment only. No production-ready claim. No final signoff. Single-node SQLite conditional pilot scope only.*
