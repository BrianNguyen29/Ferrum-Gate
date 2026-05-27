# Domainless Tier 1 Completion Evidence — 2026-05-26

> **Artifact ID**: 2026-05-26-domainless-tier1-completion-evidence
> **Date**: 2026-05-26
> **Owner**: Engineering
> **Scope**: Tier 1 (domainless production-candidate) B+C+HA-B evidence pack
> **Constraint**: Local evidence only. No production-ready claim. No HA implementation claim.

---

## 1. Summary

This artifact records the completion of Tier 1 (domainless production-candidate) engineering evidence for the confirmed B+C+HA-B scope. All required evidence artifacts exist and are reviewable. Operator acknowledgment is recorded separately in [`2026-05-26-domainless-tier1-operator-acknowledgment.md`](./2026-05-26-domainless-tier1-operator-acknowledgment.md).

---

## 2. Evidence Artifact Inventory

| # | Artifact | Description | Status |
|---|----------|-------------|--------|
| E-1 | `2026-05-21-canonical-slo-helm-conditional-signoff.md` | Canonical SLO Run #3 PASS + Helm kind install | ✅ COMPLETE |
| E-2 | `2026-05-21-target-slo-mcp-helm-domain-evidence.md` | Abbreviated target SLO + MCP smoke 15/15 + Helm static validation | ✅ COMPLETE |
| E-3 | `2026-05-22-security-audit-evidence.md` | SEC-1–SEC-6 tests + scoped token RBAC + audit log | ✅ COMPLETE |
| E-4 | `2026-05-22-compensate-path-evidence.md` | Compensate handler + R3 auto-commit suppression | ✅ COMPLETE |
| E-5 | `2026-05-22-slo-default-config-evidence.md` | Default-config SLO failure documentation | ✅ COMPLETE |
| E-6 | `2026-05-22-workload-model-refresh-evidence.md` | Workload model refresh with observed metrics | ✅ COMPLETE |
| E-7 | `2026-05-22-mcp-target-live-workload-evidence.md` | MCP target lifecycle 10/10 pass | ✅ COMPLETE |
| E-8 | `2026-05-17-sendgrid-rotation-evidence.md` | SendGrid key rotation + alert delivery verification | ✅ COMPLETE |
| E-9 | `2026-05-17-escalation-matrix-acknowledgment.md` | Operator escalation matrix acknowledgment | ✅ COMPLETE |
| E-10 | `2026-05-17-block-a-duckdns-conditional-pilot-waiver.md` | Block A conditional pilot waiver | ✅ COMPLETE |
| E-11 | `2026-05-26-pg-local-sustained-workload-evidence.md` | Local Docker PG sustained workload drill PASS (30s @ 1 rps) | ✅ COMPLETE |
| E-12 | `2026-05-26-ha-local-failover-simulation-evidence.md` | Local Docker HA/failover simulation PASS with RPO/RTO measured | ✅ COMPLETE |
| E-13 | `2026-05-26-pg-local-batch-timer-evidence.md` | `make pg-local-batch` full local run PASS | ✅ COMPLETE |
| E-14 | `2026-05-26-pg-local-automation-resume-evidence.md` | Backup/retention/offsite wrapper + deterministic resume PASS | ✅ COMPLETE |
| E-15 | `2026-05-26-domainless-candidate-plan.md` | Tier 1 B+C+HA-B scope and non-claims planning artifact | ✅ COMPLETE |
| E-16 | `2026-05-26-ha-local-ferrumd-reconnect-evidence.md` | HA ferrumd reconnect drill PASS with app-level RTO measured | ✅ COMPLETE |
| E-17 | `2026-05-26-pg-local-sustained-workload-extended-evidence.md` | Extended PG sustained workload drill PASS (120s @ 1 rps) | ✅ COMPLETE |

---

## 3. Make Targets Added for Tier 1

| Target | Purpose | Status |
|--------|---------|--------|
| `make domainless-tier1-fast` | Lightweight gate: docs + validate only | ✅ ADDED |
| `make domainless-tier1-gate` | Full gate: docs/validate + pg-local-batch + HA setup/failover/reconnect/teardown | ✅ ADDED |
| `make ha-local-ferrumd-reconnect-drill` | ferrumd reconnect against promoted standby with RTO measurement | ✅ ADDED |
| `make pg-sustained-workload-extended` | Extended 120s @ 1 rps sustained workload | ✅ ADDED |

Latest full-gate verification run (`make domainless-tier1-gate`, 2026-05-26):

- Result: `DOMAINLESS TIER 1 GATE: ALL TARGETS PASSED`
- Included docs/validate checks, `pg-local-batch`, HA local setup, HA DB failover, HA ferrumd reconnect, and HA teardown.
- Latest extended PG sub-drill: `110` successful HTTP 200 responses, `0` non-2xx responses, `0` errors.
- Latest HA ferrumd reconnect sub-drill app-level RTO: `5 s`.

---

## 4. Non-Claims

| Non-claim | Status |
|-----------|--------|
| **production-ready = NO** | Preserved. Tier 1 is not production-ready. |
| **full G2 = NOT COMPLETE** | Preserved. G2.1–G2.8 remain signed for conditional pilot only. |
| **Block A = WAIVED/CONDITIONAL** | Preserved. Real domain still required for Tier 2. |
| **PostgreSQL production = NO** | Preserved. Local Docker/runtime only. |
| **HA/multi-node = NO** | Preserved. Local simulation only. No automated failover. |
| **Sustained SLO window = NO** | Preserved. Bounded runs only. |
| **Real domain = NO** | Preserved. Tier 1 is explicitly domainless. |

---

## 5. Tier 1 → Tier 2 Gating Items

See [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](../../production-readiness-v2/00a-domainless-readiness-tier.md) §"Tier 1 → Tier 2 Gating Items" and [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md).

---

*Artifact created: 2026-05-26. Domainless Tier 1 completion evidence. No production-ready claim.*
