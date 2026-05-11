# 105 — G3.5 Operator D1–D3 Signoff Packet

> **Status**: Signed via user chat authorization on 2026-05-11. G3.5 is satisfied with Option A defaults for D1–D3. G3.6 is conditionally accepted (BrianNguyen, 2026-05-11) for initial P5b planning with conservative defaults + post-deploy monitoring.
> **Scope**: Operator decisions D1–D3 for P5b–P5e prerequisites only. No P5b–P5e implementation authorization.
> **Constraint**: Do not choose decisions on behalf of the operator. Do not pre-fill signoff fields.
> **Purpose**: Structured operator decision packet for G3.5 per `31-release-paths-todo.md` §Path 3 Gate.

---

## Purpose

This packet captures the operator decisions (D1–D3) required to satisfy G3.5:

> **G3.5**: Operator D1–D3 signoff obtained for P5b–P5e — P5a ADR §Operator Decisions D1–D3

D1–D3 are operator-owned decisions that determine the target PostgreSQL topology,
backup strategy, and failover posture. These decisions gate P5b–P5e implementation.
**Signing this packet does NOT authorize P5b–P5e implementation.** G3.6 (pilot data)
and engineering capacity confirmation are also required before implementation begins.

**Operator-owned**: D1–D3 require explicit operator selection and signature.
Engineering provides recommendations and impact analysis only.

---

## Explicit Non-Claims

- **No production-ready claim**: D1–D3 signoff does NOT make FerrumGate production-ready.
- **No P5 implementation authorization**: P5b–P5e remains gated on G3.6 and engineering go-ahead.
- **No HA/multi-node commitment**: D1 may select HA topology but P5d implementation is explicitly out of v1 scope.
- **No PostgreSQL production deployment**: D1–D3 are prerequisites only; production deployment requires P5b–P5e completion + P6 assessment.
- **No budget/capacity commitment**: Effort estimates are planning figures, not contracts.
- **Signed per explicit user instruction**: Selections, acknowledgments, and signature below were recorded by assistant per user chat authorization on 2026-05-11.

---

## Prerequisites for D1–D3 Decision

Before making D1–D3 decisions, confirm the following:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | G3.4 (P5a design) approved | `104-g3-4-p5a-adr-approval-packet.md` signed | ☑ DONE |
| R2 | P5a design doc reviewed | `50-p4-postgres-store-facade-adr.md` §3.5 read and understood | ☐ Pending (operator) |
| R3 | G2 pilot data available (if relevant to D1/D4) | Path 2 pilot metrics/logs | ☐ Pending (relevant for D4; D1–D3 can proceed without) |
| R4 | Current workload constraints understood | Operator knowledge of sustained write rate, peak load, topology needs | ☐ Pending (operator) |
| R5 | Backup/restore objectives reviewed | `27-production-evaluation-plan.md` §Operator Signoff Packet §3 | ☐ Pending (operator) |

---

## D1 — Target Topology

### Decision Question
What PostgreSQL deployment topology is the target for P5b–P5e implementation?

### Options

#### Option A: Single-node PostgreSQL (Recommended Default)

| Attribute | Value |
|---|---|
| Description | One PostgreSQL instance per FerrumGate node. No read replica. No clustering. |
| Effort | Lowest (~100-200 LOC for P5b pool tuning; P5d skipped or minimal) |
| Throughput ceiling | Bound by single-node PostgreSQL capacity (~1000–3000+ writes/s depending on hardware) |
| HA | None. Single point of failure. |
| Operational complexity | Low |
| Best fit | Workloads ≤1000 writes/s sustained; single-node acceptable; no HA requirement |

**Pros**: Simplest to implement, test, and operate. Lowest engineering effort. Proven path (P1–P4.4 used single-node Docker).  
**Cons**: No redundancy. Failover = manual restore from backup. Write ceiling = single-node capacity.  
**P5 impact**: P5b required. P5c required. P5d minimal or deferred. P5e required if migration upgrade needed.

---

#### Option B: Read Replica (Warm Standby)

| Attribute | Value |
|---|---|
| Description | One primary PostgreSQL + one or more read replicas. Writes go to primary; reads can be served from replicas. |
| Effort | Medium (~200-400 LOC + replication configuration) |
| Throughput ceiling | Higher read throughput; write throughput still bound by primary |
| HA | Partial. Read replica can be promoted to primary manually. RPO depends on replication lag. |
| Operational complexity | Medium |
| Best fit | Read-heavy workloads; need for read scaling; can tolerate manual failover |

**Pros**: Read scaling without primary load. Warm standby reduces RTO.  
**Cons**: Adds replication lag complexity. StoreFacade must distinguish read/write routes. Manual failover = downtime. Write capacity unchanged.  
**P5 impact**: P5b required. P5c required. P5d required (replication config + failover procedure). P5e required.

---

#### Option C: Full HA Cluster (Automated Failover)

| Attribute | Value |
|---|---|
| Description | Multi-node PostgreSQL cluster with automated failover (e.g., Patroni, repmgr, cloud-managed HA). |
| Effort | Highest (~400-700 LOC + cluster infrastructure + significant testing) |
| Throughput ceiling | Write throughput bound by leader; read throughput scalable |
| HA | Full automated failover. RPO near-zero; RTO seconds to minutes. |
| Operational complexity | High |
| Best fit | Mission-critical workloads requiring near-zero downtime; operator has SRE capacity |

**Pros**: Highest availability. Near-zero RPO/RTO. Production-grade posture.  
**Cons**: Significant engineering and operational effort. Complex to test and validate. Explicitly out of v1 scope.  
**P5 impact**: P5b required. P5c required. P5d required (major effort). P5e required. **Not recommended for v1.**

---

### D1 Selection

| Selection | Operator Check | Rationale (operator fills in) |
|---|---|---|
| [x] Option A — Single-node PostgreSQL | Default; lowest effort | Operator authorizes Option A per chat instruction on 2026-05-11. Lowest effort, proven P1–P4.4 path. |
| [ ] Option B — Read Replica | Read scaling needed | Not selected. |
| [ ] Option C — Full HA Cluster | Mission-critical HA required | Not selected. Explicitly out of v1 scope. |

**Operator acknowledgment**: I understand that Option C (Full HA Cluster) is explicitly out of v1 scope and requires significant additional engineering effort beyond P5 estimates.

- [x] Acknowledged

---

## D2 — Backup Strategy

### Decision Question
What backup and recovery strategy will be used for PostgreSQL?

### Options

#### Option A: `pg_dump` Logical Backup (Recommended Default)

| Attribute | Value |
|---|---|
| Description | Periodic `pg_dump` to SQL or custom format. Restored via `pg_restore`. |
| Effort | Low (~50-100 LOC + external scheduler config) |
| RPO | Backup interval dependent (e.g., 15 min = 15 min RPO) |
| RTO | Restore time + restart + verification (~5–15 min for small DBs) |
| Consistency | Point-in-time consistent if `pg_dump` uses `--snapshot` or runs during low-write window |
| Operational complexity | Low |
| Best fit | Small-to-medium databases; acceptable RPO/RTO; operator manages scheduling |

**Pros**: Simple, well-understood, portable. Works with any PostgreSQL setup.  
**Cons**: Larger databases = longer backup/restore. RPO = backup interval. Not suitable for very large datasets.  
**P5 impact**: P5c required. Operator must define schedule and retention externally.

---

#### Option B: Streaming Replication / Physical Backup

| Attribute | Value |
|---|---|
| Description | Continuous streaming replication to standby + periodic base backups (e.g., WAL archiving, `pg_basebackup`). |
| Effort | Medium (~100-200 LOC + WAL archive configuration + standby management) |
| RPO | Near-zero if synchronous replication; seconds if async |
| RTO | Minutes (promote standby to primary) |
| Consistency | Always consistent (physical copy of data files) |
| Operational complexity | Medium |
| Best fit | Lower RPO/RTO required; operator has infrastructure for WAL archiving and standby management |

**Pros**: Near-continuous protection. Fast recovery via standby promotion.  
**Cons**: Requires standby infrastructure. WAL archiving needs storage and monitoring. More complex than logical backup.  
**P5 impact**: P5c required (more complex). P5d may be required if standby is used for failover.

---

#### Option C: External Backup Tool (e.g., pgBackRest, Barman)

| Attribute | Value |
|---|---|
| Description | Dedicated PostgreSQL backup tool with incremental backup, compression, and point-in-time recovery (PITR). |
| Effort | Medium-High (~100-300 LOC + tool deployment + configuration) |
| RPO | Near-zero with WAL archiving |
| RTO | Minutes with incremental restore |
| Consistency | Always consistent |
| Operational complexity | Medium-High |
| Best fit | Large databases; need for incremental backup, compression, or PITR; operator has tool expertise |

**Pros**: Production-grade features (incremental, compression, PITR). Industry-standard tools.  
**Cons**: Additional dependency to deploy and maintain. Learning curve for operators.  
**P5 impact**: P5c required. Operator must deploy and configure tool externally.

---

### D2 Selection

| Selection | Operator Check | Rationale (operator fills in) |
|---|---|---|
| [x] Option A — `pg_dump` logical | Default; simplest | Operator authorizes Option A per chat instruction on 2026-05-11. Simplest, well-understood, portable. |
| [ ] Option B — Streaming replication / physical | Lower RPO/RTO needed | Not selected. |
| [ ] Option C — External tool (pgBackRest/Barman) | Large DB or PITR needed | Not selected. |

**Operator acknowledgment**: I understand that backup scheduling and retention are operator-owned and external to FerrumGate.

- [x] Acknowledged

---

## D3 — Failover Requirement

### Decision Question
What level of automated failover is required?

### Options

#### Option A: None — Single-node, Manual Recovery Only (Recommended Default)

| Attribute | Value |
|---|---|
| Description | No automated failover. If primary fails, operator manually restores from backup or promotes standby (if D1=B/C). |
| Effort | None beyond D1/D2 |
| RTO | Depends on D2 (minutes to hours) |
| HA | None |
| Best fit | Single-node PostgreSQL (D1=A); acceptable downtime; operator-driven recovery |

**Pros**: No additional complexity. Failover procedure = restore drill (already required for G2).  
**Cons**: Downtime during failure. Operator must be available to respond.  
**P5 impact**: P5d minimal or skipped.

---

#### Option B: Manual Failover (Standby Promotion)

| Attribute | Value |
|---|---|
| Description | Operator manually promotes a standby to primary. No automation. |
| Effort | Low (~50-100 LOC + runbook documentation) |
| RTO | Minutes (depends on operator response time + promotion time) |
| HA | Partial |
| Best fit | Read replica or HA cluster (D1=B/C); operator can respond within SLA; automation not yet trusted |

**Pros**: Faster than restore-from-backup. Operator controls promotion timing.  
**Cons**: Requires operator presence. Promotion procedure must be tested and documented.  
**P5 impact**: P5d required (failover procedure + runbook + staging test).

---

#### Option C: Automated Failover

| Attribute | Value |
|---|---|
| Description | Automated detection of primary failure + automated promotion of standby (e.g., Patroni, repmgr, cloud failover). |
| Effort | High (~200-400 LOC + cluster tooling + significant testing) |
| RTO | Seconds to minutes |
| HA | Full |
| Best fit | Full HA cluster (D1=C); mission-critical; operator has SRE/automation capacity |

**Pros**: Lowest RTO. Minimal operator intervention.  
**Cons**: High complexity. Risk of split-brain or false-positive failover. Must be thoroughly tested in staging. Explicitly out of v1 scope unless operator accepts extended timeline.  
**P5 impact**: P5d required (major effort). **Not recommended for v1.**

---

### D3 Selection

| Selection | Operator Check | Rationale (operator fills in) |
|---|---|---|
| [x] Option A — None, manual recovery | Default; simplest | Operator authorizes Option A per chat instruction on 2026-05-11. No additional complexity; failover = restore drill. |
| [ ] Option B — Manual failover | Operator-controlled promotion | Not selected. |
| [ ] Option C — Automated failover | Mission-critical, lowest RTO | Not selected. Explicitly out of v1 scope. |

**Operator acknowledgment**: I understand that automated failover (Option C) is explicitly out of v1 scope and requires significant additional engineering effort.

- [x] Acknowledged

---

## Combined Decision Impact Matrix

The combination of D1, D2, and D3 determines the P5b–P5e effort and posture:

| D1 Topology | D2 Backup | D3 Failover | P5b Effort | P5c Effort | P5d Effort | P5e Effort | Total Estimated LOC |
|---|---|---|---|---|---|---|---|
| A Single-node | A `pg_dump` | A None | Low | Low | Minimal | Low | ~200-400 |
| A Single-node | B Streaming | A None | Low | Medium | Minimal | Low | ~250-500 |
| A Single-node | A `pg_dump` | B Manual | Low | Low | Low | Low | ~250-500 |
| B Read replica | A `pg_dump` | B Manual | Medium | Low | Medium | Medium | ~350-700 |
| B Read replica | B Streaming | B Manual | Medium | Medium | Medium | Medium | ~400-800 |
| C HA Cluster | B Streaming | C Automated | High | Medium | High | High | ~700-1200 |

> **Note**: Estimates are planning figures, not commitments. Actual effort depends on operator environment, tooling, and testing requirements. P5d for HA/clustering is explicitly out of v1 scope.

---

## Risk Register (D1–D3 Specific)

| Risk ID | Risk | Trigger | Impact | Mitigation | Owner |
|---|---|---|---|---|---|
| D1-R1 | Single-node write ceiling exceeded | D1=A and workload grows beyond ~1000 writes/s sustained | Performance degradation, queue buildup | Monitor throughput; evaluate D1=B/C if sustained >800 writes/s | Operator |
| D1-R2 | Read replica adds complexity without write benefit | D1=B chosen for write scaling | Replicas unused; complexity not justified | Replicas should be chosen for read scaling, not write scaling | Operator |
| D1-R3 | HA cluster effort exceeds capacity | D1=C chosen without SRE capacity | P5d incomplete; project blocked | Select D1=C only with confirmed SRE/automation capacity | Operator + Engineering |
| D2-R1 | `pg_dump` RPO too large for workload | D2=A with infrequent backups | Data loss exceeds SLA | Reduce backup interval; evaluate D2=B/C | Operator |
| D2-R2 | Streaming replication lag unacceptable | D2=B with async replication | RPO > target | Enable synchronous replication (with throughput tradeoff) or accept risk | Operator |
| D2-R3 | External tool not maintained | D2=C with tool dependency | Backup failures; tool obsolescence | Choose well-supported tool; document operator maintenance responsibility | Operator |
| D3-R1 | Manual failover too slow | D3=A/B with tight RTO SLA | Downtime exceeds SLA | Evaluate D3=C or accept risk with compensating controls | Operator |
| D3-R2 | Automated failover causes split-brain | D3=C with misconfigured cluster | Data divergence; corruption | Thorough staging testing; use proven tooling (Patroni/repmgr) | Engineering + Operator |
| D3-R3 | Failover untested in staging | Any D3 with no staging drill | Failover fails in production | Mandatory staging drill before production deployment | Operator |

---

## Prerequisites for P5b–P5e Implementation (Post D1–D3)

Even after D1–D3 are signed, the following must be satisfied before P5b–P5e implementation begins:

| Gate | Criterion | Owner | Status |
|---|---|---|---|
| G3.5 | D1–D3 signed (this packet) | Operator | ☑ DONE (Option A defaults via chat authorization on 2026-05-11) |
| G3.6 | G2 pilot data available for P5b pool tuning | Operator | ☑ CONDITIONALLY ACCEPTED (2026-05-11; BrianNguyen; see `106-g3-6-pilot-metrics-evidence-packet.md` for caveats) |
| Eng.1 | Engineering capacity confirmed for selected topology effort | Engineering lead | ☑ DONE (via chat authorization on 2026-05-11) |
| Eng.2 | P5b–P5e implementation plan drafted per D1–D3 selections | Engineering lead | ☑ DONE (via chat authorization on 2026-05-11) |

---

## Signoff Checklist

> **Operator instruction**: Review D1–D3 options, select one option per decision, fill in rationale,
> check all acknowledgments, and sign below. **Do not sign if any decision is unselected or any risk
> is unacceptable without compensating control.**

### Operator Information

| Field | Value |
|---|---|
| Operator name | BrianNguyen |
| Organization | Operator (chat-authorized) |
| Date | 2026-05-11 |
| Review duration | Async review; selections recorded by assistant per explicit user instruction |

### Decision Verification

| # | Check | Status |
|---|---|---|
| V1 | I have reviewed D1 options (A/B/C) and selected one with rationale | [x] |
| V2 | I have reviewed D2 options (A/B/C) and selected one with rationale | [x] |
| V3 | I have reviewed D3 options (A/B/C) and selected one with rationale | [x] |
| V4 | I have reviewed the Combined Decision Impact Matrix for my selections | [x] |
| V5 | I have reviewed the Risk Register (9 risks) and find mitigations acceptable or have documented compensating controls | [x] |
| V6 | I understand that D1–D3 signoff does NOT authorize P5b–P5e implementation (G3.6 and engineering planning also required) | [x] |
| V7 | I understand that full production-ready requires P5b–P5e completion + P6 assessment | [x] |

### Approval Statement

> **Select ONE:**

- [x] **APPROVED** — D1–D3 decisions are selected and approved. P5b–P5e implementation remains gated on G3.6 and engineering planning.
- [ ] **APPROVED WITH CONDITIONS** — D1–D3 approved subject to the following conditions:
  - Condition 1: _____________________________________________________________
  - Condition 2: _____________________________________________________________
- [ ] **DECLINED** — D1–D3 not approved. Reason: __________________________________

### Signature

| Role | Signature | Date |
|---|---|---|
| Operator / Decision Authority | BrianNguyen (authorized via user chat instruction; recorded by assistant) | 2026-05-11 |
| Engineering Lead (acknowledgment) | _________________________ | _________________________ |
| Witness (optional) | _________________________ | _________________________ |

---

## Cross-References

| This Doc | Links To | Purpose |
|---|---|---|
| `105-g3-5-operator-d1-d3-signoff-packet.md` | `50-p4-postgres-store-facade-adr.md` §3.5 P5a | Canonical P5a design with D1–D6 defaults |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | `31-release-paths-todo.md` §Path 3 Gate | G3.5 gate definition |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | `104-g3-4-p5a-adr-approval-packet.md` | G3.4 approval prerequisite |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | `59-pilot-readiness-evidence-packet.md` | G2 signed conditional pilot evidence |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | `27-production-evaluation-plan.md` | Production evaluation framework |
| `31-release-paths-todo.md` | This doc | G3.5 evidence reference |
| `50-p4-postgres-store-facade-adr.md` | This doc | P5a D1–D3 signoff packet cross-reference |
| `104-g3-4-p5a-adr-approval-packet.md` | This doc | Next step after G3.4 approval |
| `106-g3-6-pilot-metrics-evidence-packet.md` | This doc | G3.6 pilot metrics evidence collection (pending operator data) |
| `107-eng-1-capacity-confirmation-packet.md` | This doc | Eng.1 capacity confirmation packet (signed via chat authorization) |
| `108-eng-2-p5b-p5e-implementation-planning-packet.md` | This doc | Eng.2 P5b–P5e implementation planning packet (approved via chat authorization) |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-11 | Initial G3.5 Operator D1–D3 signoff packet created | Engineering |
| 2026-05-11 | G3.5 signed via user chat authorization — Option A selected for D1/D2/D3 | Assistant (recorded per user instruction) |

---

*Document created: 2026-05-11. G3.5 operator decision packet — SIGNED via user chat authorization on 2026-05-11 with Option A defaults for D1–D3. G3.6 conditionally accepted (BrianNguyen, 2026-05-11). No production-ready claim. P5b may proceed ONLY with conservative defaults and post-deploy monitoring.*
