# 113 — Operator Path Selection Packet

> **Status**: Operator decision recorded — Option A (SQLite Path 2 pilot) selected 2026-05-12. No execution claimed.  
> **Purpose**: Formal operator decision on SQLite (continue Path 2) vs PostgreSQL (proceed to P5b–P5e) after P5c local smoke evidence.  
> **Scope**: Single decision gate. No live infra changes. No secrets.  
> **Constraint**: `production-ready = NO`. This packet does NOT authorize production deployment. P6 CONDITIONAL GO language applies. PostgreSQL remains an engineering/readiness track only; no PostgreSQL production deployment is claimed.

---

## 1. Purpose

This packet captures the **operator path decision** required to proceed past Phase 2 of `112-post-p5c-completion-execution-plan.md`.

The operator must select **one** of the following paths:

- **Option A — Continue SQLite (Path 2)**: FerrumGate remains on single-node SQLite. P5c PostgreSQL blockers (B6/B7) are waived. Target-host SQLite blockers (B1–B5, B8) must be closed.
- **Option B — Proceed to PostgreSQL (Path 3 / P5b–P5e)**: FerrumGate transitions to single-node PostgreSQL. P5c blockers (B6/B7) become active and must be closed on a real PostgreSQL target. SQLite target-host blockers (B1–B5) may be deferred or adapted.

**This decision does NOT make FerrumGate production-ready.** It determines which checklist blockers are active and which are waived.

---

## 2. Explicit Non-Claims

- **No production-ready claim**: Selecting a path does NOT make FerrumGate production-ready.
- **No P5b–P5e implementation authorization**: Option B selects the PostgreSQL evaluation path only. Implementation remains gated on engineering capacity, G3.6 real workload validation, and P6 assessment.
- **No HA/multi-node**: Both options are single-node only. HA/multi-node remains out of v1 scope.
- **No target-host blocker closure by this packet**: This packet records the decision only. Blocker closure requires separate operator execution and evidence.
- **No secret values**: Do not record passwords, tokens, or full DSNs in this packet.
- **No pre-filled signature**: Signature fields remain blank until operator signs.

---

## 3. Prerequisites

Before making this decision, confirm:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| R1 | P5c local smoke evidence reviewed | `artifacts/2026-05-12-p5c-local-docker-drill-evidence.md` read | ☑ Operator confirms |
| R2 | Post-P5c completion plan reviewed | `112-post-p5c-completion-execution-plan.md` read | ☑ Operator confirms |
| R3 | Workload model understood | Operator knows sustained write rate, peak concurrency, data growth | ☑ Operator confirms |
| R4 | Path 2 pilot status known | `59-pilot-readiness-evidence-packet.md` reviewed; conditional pilot understood | ☑ Operator confirms |
| R5 | P5a design reviewed (if considering PostgreSQL) | `50-p4-postgres-store-facade-adr.md` §3.5 read | ☐ N/A — Option A selected; PostgreSQL track remains readiness-only |

---

## 4. Decision Options

### Option A — Continue SQLite (Path 2)

> **Select this option if**: Single-node SQLite capacity is acceptable for the target workload; no PostgreSQL operational capacity exists; or PostgreSQL transition risk exceeds benefit.

#### A.1 Consequences

| Area | Consequence |
|---|---|
| **Blockers B6/B7** | Waived. P5c.V1/V2 target-host PostgreSQL drills are N/A. Operator must sign explicit waiver (§6). |
| **Blockers B1–B5** | Active. Target-host D1–D6, restore drill, backup automation, TLS, bearer token must be closed per `115-sqlite-path2-target-host-checklist.md`. |
| **Blocker B8** | Active. G3.6 real workload monitoring must be completed per `116-g36-monitoring-execution-plan.md`. |
| **Capacity ceiling** | ≤300 writes/s sustained. Exceeding this requires re-evaluating Option B. |
| **Backup format** | SQLite `.db` file copies via `ferrumctl backup create`. No `pg_dump`. |
| **Scope** | Single-node SQLite v1 RC-ready/conditional. No PostgreSQL. |

#### A.2 Engineering Recommendation

Select Option A if ALL of the following are true:
- Sustained write rate ≤ 250 writes/s with headroom
- Single-node topology acceptable
- Operator can manage SQLite file-based backups and restore drills
- No multi-node or read-replica requirements within 12 months

---

### Option B — Proceed to PostgreSQL (Path 3 / P5b–P5e)

> **Select this option if**: Workload exceeds SQLite capacity; operator prefers PostgreSQL operational tooling; or future multi-node/HA is anticipated.

#### B.1 Consequences

| Area | Consequence |
|---|---|
| **Blockers B6/B7** | Active. Operator must execute target-host P5c.V1 backup and P5c.V2 restore drills per `114-target-host-p5c-drill-checklist.md`. |
| **Blockers B1–B5** | Active but adapted. D1–D6 drills run against PostgreSQL store. Backup uses `pg_dump`. Restore uses `pg_restore`. |
| **Blocker B8** | Active. G3.6 real workload monitoring must be completed on PostgreSQL target per `116-g36-monitoring-execution-plan.md`. |
| **Engineering gates** | P5b–P5e implementation requires engineering go-ahead after this decision. Not automatic. |
| **P6 assessment** | Required before production PostgreSQL deployment. |
| **RPO/RTO** | Operator-approved targets remain 15min/30min (`109-p5c-postgresql-backup-restore-runbook.md`). |

#### B.2 Engineering Recommendation

Select Option B if ANY of the following are true:
- Sustained write rate > 250 writes/s or expected to grow beyond 300 writes/s
- Operator requires PostgreSQL operational tooling (pg_dump, pg_restore, SQL access)
- Future read-replica or multi-node evaluation is desired (post-v1)
- SQLite file-based backup/restore does not meet operational requirements

---

## 5. Option Comparison

| Criterion | Option A — SQLite | Option B — PostgreSQL |
|---|---|---|
| **Max sustained writes** | ~300/s | Higher (pool-tuned, post-P5b) |
| **Backup tool** | `ferrumctl backup create` | `pg_dump` |
| **Restore tool** | `ferrumctl backup restore` | `pg_restore` |
| **Backup format** | SQLite `.db` file | Custom archive (`.dump`) |
| **Scheduler** | External cron/systemd timer | External cron/systemd timer |
| **Operational familiarity** | File-based | SQL-based |
| **Future multi-node** | Not possible without migration | Possible (post-v1) |
| **P5b–P5e engineering** | N/A | Required |
| **P6 assessment** | N/A (SQLite already conditional) | Required before production PG |
| **Blocker count** | B1–B5, B8 active; B6/B7 waived | B1–B8 all active (adapted) |

---

## 6. Waiver Language (Option A Only)

If the operator selects **Option A**, the following waiver must be acknowledged and signed:

> **P5c Blocker Waiver (B6/B7)**
>
> I, the operator, acknowledge that:
> 1. P5c PostgreSQL backup/restore drills (B6/B7) are **not applicable** to the SQLite production path.
> 2. This waiver does NOT remove the requirement to complete SQLite backup/restore drills (B2/B3).
> 3. If FerrumGate is later transitioned to PostgreSQL, P5c drills must be executed before production PostgreSQL deployment.
> 4. This waiver does NOT constitute a production-ready claim.

| Waiver Item | Acknowledged |
|---|---|
| B6 (P5c.V1 target-host backup drill) | `☑ N/A — SQLite path` |
| B7 (P5c.V2 target-host restore drill) | `☑ N/A — SQLite path` |
| B6/B7 may become active if path changes to PostgreSQL | `☑ Acknowledged` |

---

## 7. Signoff

### 7.1 Decision Record

| Field | Value |
|---|---|
| Date | `2026-05-12` |
| Operator name | `User (via agent instruction)` |
| Organization | `FerrumGate` |
| Pilot environment | `Conditional single-node SQLite pilot` |

### 7.2 Path Selection

> **Select ONE and initial:**

- [x] **Option A — Continue SQLite (Path 2)**
  - Initials: `User authorization via agent instruction`
  - Rationale: `Close Conditional single-node SQLite pilot first; continue PostgreSQL readiness as a separate non-production track.`

- [ ] **Option B — Proceed to PostgreSQL (Path 3 / P5b–P5e)**
  - Initials: ______
  - Rationale: _________________________________

### 7.3 Acknowledgments

| # | Acknowledgment | Initials |
|---|---|---|
| K1 | I understand that this decision does NOT make FerrumGate production-ready | `Acknowledged via agent instruction` |
| K2 | I understand that Option B requires P5b–P5e engineering implementation + P6 assessment before production PostgreSQL deployment | `Acknowledged; PostgreSQL remains readiness-only` |
| K3 | I understand that Option A retains the ≤300 writes/s single-node SQLite ceiling | `Acknowledged` |
| K4 | I understand that blockers remain active for the selected path and require separate execution and evidence | `Acknowledged` |
| K5 | I understand that switching paths after signoff requires a new decision packet and may invalidate prior evidence | `Acknowledged` |

### 7.4 Signature

| Role | Name | Date | Signature |
|---|---|---|---|
| Operator / Decision Authority | `User (via agent instruction)` | `2026-05-12` | `Recorded by agent; no secret values` |
| Engineering Lead (acknowledgment of receipt) | `FerrumGate engineering` | `2026-05-12` | `Acknowledged` |
| Witness (optional) | | | |

---

## 8. Cross-References

| This Packet | Links To | Purpose |
|---|---|---|
| `113-operator-path-selection-packet.md` | `112-post-p5c-completion-execution-plan.md` | Phase 2 decision gate |
| `113-operator-path-selection-packet.md` | `66-path-2-operator-handoff.md` §B.0 | Consolidated blocker checklist |
| `113-operator-path-selection-packet.md` | `105-g3-5-operator-d1-d3-signoff-packet.md` | G3.5 D1–D3 context for Option B |
| `113-operator-path-selection-packet.md` | `114-target-host-p5c-drill-checklist.md` | Option B: target-host P5c drills |
| `113-operator-path-selection-packet.md` | `115-sqlite-path2-target-host-checklist.md` | Option A: SQLite target-host checklist |
| `113-operator-path-selection-packet.md` | `116-g36-monitoring-execution-plan.md` | G3.6 monitoring (both paths) |
| `113-operator-path-selection-packet.md` | `55-phase-3-go-no-go-review.md` | Decision recording location |

---

## 9. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial operator path selection packet | Engineering |
| 2026-05-12 | Recorded Option A SQLite Path 2 pilot selection; PostgreSQL remains readiness-only track | Agent per user instruction |

---

*Document updated: 2026-05-12. Operator Path Selection Packet — Option A selected. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim. No secret values.*
