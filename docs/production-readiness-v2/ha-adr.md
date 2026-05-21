# HA-ADR-001 — High Availability Architecture Decision Record

> **Status**: APPROVED AS PLANNING DECISION — operator delegate signoff recorded 2026-05-21. No implementation claim; no HA claim.
> **Owner**: Engineering + Operator  
> **Last updated**: 2026-05-21  
> **Parent**: [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md)  
> **Scope**: [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md)  

---

## 1. Context and problem statement

### 1.1 Current topology

| Layer | State |
|-------|-------|
| Runtime | Single-node only (SQLite or PostgreSQL) |
| Store backend | One DSN per ferrumd process (`FERRUMD_STORE_DSN`) |
| Connection pool | `sqlx::PgPool` with transparent reconnect; no circuit breaker |
| Replication | None |
| Read routing | None |
| Failover | None (manual restart only) |
| Load balancer | None |

### 1.2 Problem

FerrumGate v1 is a single-node service. If the PostgreSQL primary fails:

- ferrumd cannot write or read.
- There is no standby to promote.
- There is no read replica to serve stale reads.
- Recovery requires manual intervention (restart ferrumd with a new DSN or wait for PG restart).

Single-node SQLite is even more constrained (no replication path at all).

### 1.3 Decision drivers (ranked)

| Rank | Driver | Weight |
|------|--------|--------|
| 1 | **Operational simplicity** — The operator team is small; complex HA stacks (Patroni) increase incident risk. | High |
| 2 | **RTO/RPO bounds** — How fast must failover be, and how much data can be lost? | High |
| 3 | **Cost / vendor dependency** — Managed PG HA reduces ops burden but creates cloud dependency. | Medium |
| 4 | **ferrumd code changes** — Minimal changes are preferred; ferrumd is architected around a single DSN. | Medium |
| 5 | **Split-brain prevention** — Any automated failover must guarantee no split-brain. Unsolved = no HA claim. | Critical for automated failover |
| 6 | **Tenant/security model stability** — Automated HA should not be built before the tenant model is finalized (risk of data consistency issues across roles). | Medium |

---

## 2. Options considered

### Option A — Managed PostgreSQL HA (cloud/vendor)

**Description**: Use AWS RDS Multi-AZ, GCP Cloud SQL HA, Azure Flexible Server, or equivalent.

**Pros**:
- Minimal operational burden.
- Built-in failover, backup, patching.
- No extra infrastructure to run (no etcd, no Patroni).

**Cons**:
- Vendor/cloud dependency.
- Cost scales with instance size.
- Failover is still not instantaneous (RTO typically 60–120 s).
- Network latency between ferrumd and managed PG matters.

**Verdict**: **Recommended as Step 1** for operators who already use a supported cloud. Not applicable for on-premise or multi-cloud deployments.

### Option B — Patroni + etcd/ZooKeeper

**Description**: Open-source HA stack for PostgreSQL using DCS (Distributed Consensus Store) for leader election.

**Pros**:
- Mature, widely used.
- Automatic failover with watchdog.
- Supports synchronous replication for zero-RPO.

**Cons**:
- High operational complexity (etcd cluster, Patroni agents, HAProxy/PgBouncer).
- Significant learning curve for the operator.
- Failure modes in etcd = total cluster stall.

**Verdict**: **Viable for Step 3 (automated failover)** only after operator expertise and infrastructure exist. Not for v1.

### Option C — repmgr

**Description**: Simpler PostgreSQL replication manager with manual and semi-automatic failover.

**Pros**:
- Simpler than Patroni.
- Good for manual failover workflows.

**Cons**:
- Less mature automatic failover than Patroni.
- Still requires replication expertise.
- Split-brain prevention weaker than Patroni + synchronous replication.

**Verdict**: **Viable alternative to Patroni** if operator prefers lighter weight. Defer to Step 3 evaluation.

### Option D — Manual failover runbook

**Description**: Documented procedure: detect primary failure, promote standby manually, update ferrumd DSN or restart.

**Pros**:
- Simplest to implement.
- No new infrastructure.
- Operator remains in control.

**Cons**:
- Higher RTO (minutes, not seconds).
- Human error risk during incident.
- Not "true HA" in the automated sense.

**Verdict**: **Recommended as Step 2** — manual failover is the safest first step before automation.

### Option E — Read replicas only (no write HA)

**Description**: Keep a single primary for writes; add read replicas for read scaling. Reads can route to replicas; writes always go to primary.

**Pros**:
- Easier than full HA.
- Offloads read traffic.
- Useful for reporting/auditor workloads.

**Cons**:
- Write path still has a single point of failure.
- Replica lag must be monitored and documented.
- ferrumd must distinguish read vs write connections (DSN split or proxy).

**Verdict**: **Recommended as Step 2 alongside manual failover** — provides read scaling without the complexity of automated write failover.

### Option F — Status quo (single-node PostgreSQL)

**Description**: No replication, no standby. Rely on PgPool reconnect + backups.

**Pros**:
- Zero additional infrastructure.
- Current v1 scope.

**Cons**:
- No failover.
- RTO = however long it takes to restore from backup or restart PG.

**Verdict**: **Current state**. Acceptable for conditional pilot, insufficient for production-candidate.

---

## 3. Selected strategy — phased approach

| Step | Option | Scope | When |
|------|--------|-------|------|
| 1 | **Managed PostgreSQL HA** (Option A) or **Status quo + hardened backup** (Option F) | Primary store | First production-candidate posture |
| 2 | **Manual failover runbook** (Option D) + **Read replicas** (Option E) | Documented procedure + read scaling | After Step 1 is stable |
| 3 | **Patroni or repmgr** (Option B or C) | Automated failover | After tenant/security model is stable and operator is ready |

**Why phased?**
- HA complexity should match operator maturity and infrastructure.
- Automated failover without split-brain prevention is worse than manual failover.
- ferrumd architecture is currently single-DSN; splitting reads/writes or supporting dynamic DSN changes requires design work that should be informed by real operational experience.

---

## 4. ferrumd architecture impact

### 4.1 Current state — single DSN

ferrumd uses one `FERRUMD_STORE_DSN` environment variable (or config field). The `PgPool` is initialized once at startup.

### 4.2 Step 1 impact (managed PG HA)

**Minimal**.
- If the managed PG provides a single endpoint that handles failover internally (e.g., RDS Multi-AZ DNS endpoint), ferrumd needs no code changes.
- PgPool reconnect + `/v1/readyz/deep` pool saturation readiness is sufficient.
- PG-2.3b circuit breaker, if ever implemented, would help during managed failover blips.

### 4.3 Step 2 impact (manual failover + read replicas)

**Low to medium**.
- Manual failover: operator updates DSN and restarts ferrumd. No code change required if documented.
- Read replicas: ferrumd would need a **secondary read DSN** or a **proxy** (e.g., PgBouncer, HAProxy) to route reads. This requires:
  - Config change: `FERRUMD_STORE_READ_DSN` optional field.
  - StoreFacade change: `read_pool` vs `write_pool`.
  - Endpoint audit: which endpoints are read-only? Most GET endpoints; all mutating endpoints use write pool.
  - Readiness extension: report replica lag if relevant.

**Decision**: Read replica support is **deferred to Step 2 design**; this ADR does not specify the split. A follow-up ADR is required before implementation.

### 4.4 Step 3 impact (automated failover)

**Medium**.
- If using Patroni/repmgr with a virtual IP or proxy, ferrumd may still use a single DSN (the proxy/VIP). Code changes may be minimal.
- If using multiple endpoints (e.g., primary DSN + standby DSN list), ferrumd needs connection-routing logic.
- Circuit breaker (PG-2.3b B.3) becomes valuable here to fail fast during failover windows.

---

## 5. Schema migration and failover

### 5.1 Forward migrations

- Migrations run at ferrumd startup via `apply_embedded_migrations()`.
- In an HA topology, only the **primary** should run migrations.
- If using read replicas, replicas must be recreated or re-synced after schema changes.

### 5.2 Failover and schema version

- After a standby promotion, the new primary's schema version must match ferrumd's expected version.
- The manual failover runbook (Step 2) must include a schema version check.

### 5.3 Read replica lag

- Replicas may lag behind the primary.
- ferrumd must document that reads from replicas can be stale.
- Policy/approval state must never be read from a lagging replica if it could cause inconsistent decisions.

---

## 6. Split-brain prevention

**Status**: DEFERRED to Step 3.

**Why deferred**:
- Split-brain prevention is only required for **automated failover** (Step 3).
- Manual failover (Step 2) is human-controlled; the operator decides which node is primary.
- Managed PG HA (Step 1) delegates split-brain prevention to the vendor.

**Future requirements for Step 3**:
- Synchronous replication or a consensus mechanism (etcd/ZooKeeper) for leader election.
- Fencing / STONITH mechanism to prevent the old primary from accepting writes after failover.
- ferrumd must be able to detect and report split-brain (e.g., two primaries detected via DSN probe).

---

## 7. Dependency chain

| Dependency | Status | Blocks |
|------------|--------|--------|
| PostgreSQL production foundation stable (PG-1 through PG-4) | ✅ Local fallback complete; target-host PG deferred | Step 1 |
| SLO metrics available | ✅ Baseline measured; target-host sustained deferred | Step 1 |
| Backup/restore evidence exists | ✅ Local drill complete; scheduled backup deferred | Step 1 |
| Security/tenant model decided | ✅ Single-tenant approved; multi-tenant deferred | Step 3 |
| **BLK-A-DOM** — real owned domain | ☐ WAIVED/CONDITIONAL | Full G2 closure; production-ready claim |
| Operator HA infrastructure readiness | ☐ Not assessed | Step 2 and Step 3 |
| PG-2.3b circuit breaker ADR (B.3) | ☐ DEFERRED | Step 3 (if circuit breaker desired) |

**Note**: BLK-A-DOM is explicitly **out of scope** for this ADR. It remains an external operator blocker. This ADR does not close BLK-A-DOM.

---

## 8. Non-claims

- **NOT production-ready**: This ADR is a planning draft. No HA code exists.
- **APPROVED AS PLANNING DECISION ONLY**: Operator delegate signoff recorded 2026-05-21. This approval authorizes the phased planning approach only. No HA implementation is authorized.
- **NOT implementation-ready**: No Step 2 or Step 3 work should begin without a follow-up ADR.
- **NOT HA yet**: FerrumGate remains single-node. HA is explicitly out of scope for the current conditional pilot.
- **NOT automated failover soon**: Manual failover and read replicas come first.
- **NOT closing BLK-A-DOM**: Real owned domain remains required for production-ready or full G2 closure.
- **NOT a committed timeline**: Dates are placeholders; actual execution is operator-dependent.

---

## 9. Operator review and signoff request

This ADR is ready for operator review. The operator should read §2 (Options), §3 (Selected strategy), and §7 (Dependency chain), then decide:

1. **Preferred HA path**: Does the operator accept the phased approach (managed PG/manual failover → read replicas → automated failover)?
2. **Step 1 preference**: Managed PostgreSQL HA (cloud) or status quo + hardened backup for the first production-candidate posture?
3. **RTO/RPO bounds**: What are the operator's acceptable bounds for Step 2 manual failover?
4. **Step 3 readiness**: When should Step 3 (automated failover) be evaluated?
5. **BLK-A-DOM acknowledgment**: Operator acknowledges that real owned domain remains required for production-ready or full G2 closure, and this ADR does not close Block A.

**Signoff block**:

```
Operator name: Authorized operator delegate (per user instruction in current session)
Date: 2026-05-21
Decision: [x] Approve phased approach  [ ] Request changes  [ ] Defer
Notes: Planning decision only. HA implementation remains NOT STARTED.
HA/multi-node = NO. PostgreSQL production deployment = NO.
production-ready = NO. BLK-A-DOM remains external/WAIVED-CONDITIONAL.
No Step 2 or Step 3 work should begin without a follow-up ADR.
```

No implementation should begin until this ADR is reviewed and the operator confirms the preferred path.

## 10. Cross-references

- [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md) — HA roadmap and phased plan
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PG hardening prerequisites (PG-1 through PG-5)
- [`docs/implementation-path/artifacts/2026-05-21-pg-2.3b-reconnect-circuit-breaker-backlog.md`](../implementation-path/artifacts/2026-05-21-pg-2.3b-reconnect-circuit-breaker-backlog.md) — B.3 circuit breaker deferral decision
- [`docs/production-readiness-v2/10-evidence-checklist.md`](./10-evidence-checklist.md) — Phase 9 evidence checklist
- [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md) — BLK-A-DOM and other blockers
- [`docs/ROADMAP.md`](../ROADMAP.md) — §3.3.3 Phase PG-5, §4 Phase 9

---

*End of HA ADR draft — planning artifact only (2026-05-21).*
