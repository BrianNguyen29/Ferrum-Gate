# Domainless Production-Candidate Plan — 2026-05-26

> **Artifact ID**: 2026-05-26-domainless-candidate-plan
> **Date**: 2026-05-26
> **Owner**: Engineering
> **Scope**: Tier 1 (domainless production-candidate) milestone for the confirmed B+C+HA-B plan.
> **Constraint**: Docs-only planning artifact. No code changes. No production-ready claim. No HA implementation claim.

---

## 1. Executive Summary

This artifact defines the Tier 1 **domainless production-candidate** milestone for FerrumGate v1. It is the planning companion to [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](../../production-readiness-v2/00a-domainless-readiness-tier.md).

The confirmed scope is **B+C+HA-B**:
- **B**: Domainless readiness semantics defined without weakening the legacy Tier 2 `production-ready` definition.
- **C**: PostgreSQL local hardening maximized with local Docker migration/restore/backup/resume/timer and sustained workload evidence.
- **HA-B**: Local Docker primary/standby streaming replication and manual failover simulation with RPO/RTO measurement.

Tier 1 attainment means engineering evidence for B+C+HA-B is complete and reviewable. It does **not** mean production-ready, full G2, Block A closed, PostgreSQL production deployed, or production HA implemented.

---

## 2. Tier 1 Scope Detail

### 2.1 B — Domainless readiness semantics

| Item | Evidence Required | Status |
|------|-------------------|--------|
| Three-tier readiness model | `00a-domainless-readiness-tier.md` defines Tier 0/Tier 1/Tier 2 | ✅ DEFINED |
| Tier 1 label | `domainless production-candidate` used consistently | ✅ DEFINED |
| Legacy production-ready preserved | `production-ready = NO` until Tier 2 real-domain gates | ✅ PRESERVED |
| BLK-A-DOM routing | Block A gates Tier 1→Tier 2, not Tier 0→Tier 1 | ✅ DOCUMENTED |

Historical blocker status feeding Tier 1 remains:

| Blocker ID | Description | Evidence Required | Status |
|------------|-------------|-------------------|--------|
| BLK-SLO-TGT | SLO target-host workload validation | Canonical SLO Run #3 PASS (max-valid); Runs #1/#2 documented as failure evidence | ✅ UNBLOCKED |
| BLK-SEC-PH4 | Phase 4 scoped token / RBAC model | Scoped token schema, RBAC middleware, SEC-1–SEC-6 tests pass | ✅ IMPLEMENTED |
| BLK-UX-4 | UX-4 token rotate / revoke CLI | `ferrumctl admin tokens` list/create/revoke/rotate wired and tested | ✅ IMPLEMENTED |
| BLK-MCP-TGT | Phase 3 MCP target-host smoke | 15/15 target MCP smoke pass; 19 tools validated | ✅ UNBLOCKED |
| BLK-DEP-5 | DEP-5 Helm / K8s packaging | `helm lint` + `helm template` PASS; live kind install PASS | ✅ LIVE KIND PASS |
| BLK-SLO-RAT | SLO operator ratification | Operator ratified baseline SLO targets | ✅ RATIFIED |

### 2.2 C — PostgreSQL local hardening maximized

| Item | Evidence Required | Status |
|------|-------------------|--------|
| PG migration/restore | Populated SQLite→PG migration + backup/restore drills pass | ✅ LOCAL EVIDENCE |
| PG backup/retention/offsite | Local wrapper prunes old matching backups, verifies offsite hash parity, restores copy | ✅ LOCAL EVIDENCE |
| PG partial-failure/resume | Deterministic resume simulation restores all checkpoints and row counts | ✅ LOCAL EVIDENCE |
| PG scheduled timer simulation | Text-only unit/timer due-skip simulation passes | ✅ LOCAL EVIDENCE |
| PG sustained workload | Local Docker PG + ferrumd short workload passes, readyz remains 200, PG pool metrics present | ✅ LOCAL EVIDENCE |
| PG production deployment | Live/operator PG deployment, TLS, PgBouncer, live alerts | ☐ NOT CLAIMED |

### 2.3 HA-B — Local HA/failover simulation

| Item | Evidence Required | Status |
|------|-------------------|--------|
| Local primary/standby setup | `make ha-local-setup` verifies streaming replication | ✅ LOCAL EVIDENCE |
| Local failover drill | `make ha-local-failover-drill` promotes standby and measures RPO/RTO | ✅ LOCAL EVIDENCE |
| Local teardown | `make ha-local-teardown` cleans containers/volumes | ✅ LOCAL EVIDENCE |
| Production HA | Multi-host, automated failover, fencing, live RPO/RTO | ☐ NOT CLAIMED |

**Explicit non-claim**: This is local single-host Docker simulation only. NOT production HA. NOT automated failover. NOT multi-host.

---

## 3. Expected Evidence Artifacts for Tier 1 Signoff

The following evidence artifacts must exist and be reviewable for Tier 1 acknowledgment:

| # | Artifact | Description |
|---|----------|-------------|
| E-1 | `2026-05-21-canonical-slo-helm-conditional-signoff.md` | Canonical SLO Run #3 PASS + Helm kind install |
| E-2 | `2026-05-21-target-slo-mcp-helm-domain-evidence.md` | Abbreviated target SLO + MCP smoke 15/15 + Helm static validation |
| E-3 | `2026-05-22-security-audit-evidence.md` | SEC-1–SEC-6 tests + scoped token RBAC + audit log |
| E-4 | `2026-05-22-compensate-path-evidence.md` | Compensate handler + R3 auto-commit suppression |
| E-5 | `2026-05-22-slo-default-config-evidence.md` | Default-config SLO failure documentation |
| E-6 | `2026-05-22-workload-model-refresh-evidence.md` | Workload model refresh with observed metrics |
| E-7 | `2026-05-22-mcp-target-live-workload-evidence.md` | MCP target lifecycle 10/10 pass |
| E-8 | `2026-05-17-sendgrid-rotation-evidence.md` | SendGrid key rotation + alert delivery verification |
| E-9 | `2026-05-17-escalation-matrix-acknowledgment.md` | Operator escalation matrix acknowledgment |
| E-10 | `2026-05-17-block-a-duckdns-conditional-pilot-waiver.md` | Block A conditional pilot waiver |
| E-11 | `2026-05-26-pg-local-sustained-workload-evidence.md` | Local Docker PG sustained workload drill PASS |
| E-12 | `2026-05-26-ha-local-failover-simulation-evidence.md` | Local Docker HA/failover simulation PASS with RPO/RTO measured |

---

## 4. Non-Claims for Tier 1

| Non-claim | Meaning |
|-----------|---------|
| **production-ready = NO** | Tier 1 is not production-ready. Do not deploy to unbounded production workloads. |
| **full G2 = NOT COMPLETE** | G2.1–G2.8 remain signed for conditional pilot only. |
| **Block A = WAIVED/CONDITIONAL** | Real domain is deferred. Block A is not closed. |
| **PostgreSQL production = NO** | Local PG runtime exists; production PG target deployment + evidence does not. |
| **HA/multi-node = NO** | Production HA is not implemented. HA-B is local Docker simulation only. |
| **Sustained SLO window = NO** | No 7–30 day observation window exists. All runs are bounded. |
| **Real domain = NO** | Tier 1 is explicitly domainless. Real domain remains required for Tier 2. |
| **Target-host MCP live workload = CONDITIONAL/EVIDENCE-BACKED** | Engineering evidence exists; operator signoff NOT obtained for full certification. |
| **Scoped auth/RBAC = PARTIAL** | Scoped tokens and RBAC middleware implemented; tenant model and OIDC deferred. |
| **Multi-tenant = NO** | No tenant isolation exists. |

---

## 5. Tier 1 → Tier 2 Gating Items

To move from Tier 1 (domainless production-candidate) to Tier 2 (production-ready / domain-backed), the following must occur:

| # | Step | Owner | Evidence Required |
|---|------|-------|-------------------|
| 1 | Operator procures real owned domain | Operator | WHOIS / registrar receipt |
| 2 | DNS A record configured | Operator | `dig +short` from ≥2 resolvers |
| 3 | HTTPS 200 from target host | Engineering + Operator | `curl -sf https://<domain>/v1/healthz` |
| 4 | L1–L5 target bridge re-run with real domain | Engineering | `YYYY-MM-DD-block-a-closure-evidence.md` |
| 5 | Production PostgreSQL deployment + drill | Engineering + Operator | `YYYY-MM-DD-pg-production-deployment-signoff.md` |
| 6 | Sustained SLO observation window (7–30 days) | Engineering + Operator | `YYYY-MM-DD-slo-sustained-window-evidence.md` |
| 7 | G2.1–G2.8 re-signed with new evidence | Operator | `TEMPLATE-full-g2-resignoff.md` filled + signed |
| 8 | Operator final production posture signoff | Operator | `TEMPLATE-final-production-readiness-signoff.md` filled + signed |

---

## 6. Relationship to Other Docs

| Document | Relationship |
|----------|--------------|
| [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](../../production-readiness-v2/00a-domainless-readiness-tier.md) | Canonical three-tier model definition |
| [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md) | Scope boundaries and master non-claims |
| [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md) | Blocker status; BLK-A-DOM gates Tier 1→Tier 2 |
| [`docs/implementation-path/01-current-state.md`](../01-current-state.md) | Current state with Tier 1 target |
| [`docs/ROADMAP.md`](../ROADMAP.md) | Milestone 0.5 (Domainless Production-Candidate) |
| [`2026-05-23-rc-ready-conditional-end-state.md`](./2026-05-23-rc-ready-conditional-end-state.md) | RC-ready conditional end state (Tier 0 terminal) |

---

## 7. What This Artifact Does NOT Claim

| Non-Claim | Reason |
|-----------|--------|
| **No production-ready** | Tier 1 is explicitly not production-ready. |
| **No full G2 closure** | Full G2 requires Tier 2 gates. |
| **No Block A closed** | Block A remains WAIVED/CONDITIONAL at Tier 1. |
| **No PostgreSQL production** | Local Docker/runtime only. |
| **No HA/multi-node** | HA-B is local Docker simulation only; no production HA, automated failover, or multi-host topology. |
| **No sustained SLO window** | Bounded runs only. |
| **No real domain** | Tier 1 is domainless by definition. |

---

*Artifact created: 2026-05-26. Domainless production-candidate plan — B+C+HA-B scope. No production-ready claim. No full G2 closure.*
