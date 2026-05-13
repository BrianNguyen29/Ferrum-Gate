# 119 — PostgreSQL Readiness Operations Cadence

> **Status**: Process document. No execution claimed. No production-ready claim.  
> **Purpose**: Define the rehearsal artifact recording cadence (Track D.5) and failure-handling escalation process (Track D.6) for PostgreSQL readiness.  
> **Scope**: Local rehearsal only. No target-host execution. No secrets.  
> **Constraint**: This document does NOT authorize production PostgreSQL deployment. Production remains gated on P5b–P5e and P6.

---

## 1. D.5 — Rehearsal Artifact Recording Cadence

### 1.1 Purpose

Every Track D rehearsal (D.1–D.4) produces a dated evidence artifact in `docs/implementation-path/artifacts/`. This section defines the naming convention, required sections, and acceptance criteria so that artifacts are consistent and auditable.

### 1.2 Naming Convention

```text
docs/implementation-path/artifacts/
  YYYY-MM-DD-d<N>-<short-description>-rehearsal-evidence.md
```

| Component | Rule | Example |
|-----------|------|---------|
| Date | ISO-8601 date of rehearsal | `2026-05-12` |
| Track ID | `d1`, `d2`, `d3`, or `d4` | `d1` |
| Description | 2–5 words, kebab-case | `populated-local-migration` |
| Suffix | always `-rehearsal-evidence.md` | `-rehearsal-evidence.md` |

**Examples from prior work**:
- `2026-05-12-d1-populated-local-migration-rehearsal-evidence.md`
- `2026-05-12-d2-partial-resume-local-migration-rehearsal-evidence.md`
- `2026-05-12-d3-content-hash-validation-rehearsal-evidence.md`
- `2026-05-12-d4-large-dataset-streaming-rehearsal-evidence.md`

### 1.3 Required Sections

Every artifact MUST contain the following sections, in order:

| # | Section | Purpose |
|---|---------|---------|
| 1 | Header with status banner | `LOCAL-ONLY REHEARSAL [PASSED / FAILED]` plus explicit non-claims |
| 2 | Scope | Drill type, evidence target, and explicit `Production claim: NO` |
| 3 | Environment | Date, repo path, PostgreSQL container/image, fixture path (no secrets) |
| 4 | Commands Run | Exact commands used; passwords replaced with `<REDACTED>` |
| 5 | Validation | Exit codes, row counts, hash matches, timing (if applicable) |
| 6 | Overall Verdict | PASS / FAIL with explicit non-claims repeated |
| 7 | Cleanup | What was removed, what was retained, and why |
| 8 | Remaining Work | Links back to `117-postgresql-readiness-acceleration-plan.md` and open tracks |

### 1.4 Acceptance Criteria for Each Rehearsal

A rehearsal is considered **recorded** when the artifact meets all of the following:

| Criterion | Required | Evidence Location |
|-----------|----------|-------------------|
| Migration exits 0 (D.1/D.2/D.3/D.4) | Yes | Artifact §Validation |
| Row counts match source (±0) | Yes | Artifact §Validation |
| Content-hash validation passes (if exercised) | Yes, for D.3 | Artifact §Validation |
| No secrets in log output | Yes | Artifact §Commands Run and §Cleanup |
| Explicit non-claims repeated in header and verdict | Yes | Artifact §Header and §Overall Verdict |
| Artifact filename follows naming convention | Yes | Filename itself |

### 1.5 Cadence Schedule

| Track | Minimum Cadence | Owner | Trigger |
|-------|-----------------|-------|---------|
| D.1 — Populated local migration | Weekly during active pilot development | Engineering | Sprint planning or CI smoke failure |
| D.2 — Partial resume / idempotency | Bi-weekly | Engineering | Same as D.1, alternating weeks |
| D.3 — Content-hash validation | Weekly | Engineering | Same as D.1 |
| D.4 — Large-dataset streaming | Monthly | Engineering | First Monday of each month |
| D.5 — Artifact recording | Per run (same cadence as the rehearsal) | Engineering | Immediate, before closing rehearsal session |

### 1.6 Stop Condition for Cadence

If any rehearsal fails its acceptance criteria, **immediately pause the entire Track D cadence** and follow D.6 (Failure Handling) before resuming.

---

## 2. D.6 — Failure Handling and Escalation Process

### 2.1 Purpose

If a Track D rehearsal fails, the response is not "retry until it passes." It is: **stop, treat as a P1 bug, investigate root cause, fix, and re-run.** This section defines the exact steps.

### 2.2 Failure Definition

A rehearsal has **failed** when any of the following occur:

| Failure Mode | Example | Severity |
|--------------|---------|----------|
| Non-zero exit code | `ferrum-migrate` exits `1` | P1 — migration or schema bug |
| Row count mismatch | Source=50, Target=49 | P1 — data loss or silent skip |
| Content-hash mismatch | `source_hash != target_hash` | P1 — corruption or encoding drift |
| Memory exhaustion / OOM | Docker container killed during D.4 | P1 — streaming/chunking bug |
| Schema initialization failure | `sqlx migrate` fails on target PG | P1 — schema drift |
| Secrets leaked in artifact | Password found in evidence markdown | P0 — security incident (retract artifact immediately) |

### 2.3 Immediate Response (First 15 Minutes)

| Step | Action | Owner |
|------|--------|-------|
| 1 | **STOP** — Do not run additional rehearsals until root cause is understood. | Engineering |
| 2 | **Preserve** — Save logs, container state, and fixture. Do not clean up yet. | Engineering |
| 3 | **Classify** — Map failure to Failure Definition table above. | Engineering |
| 4 | **Redact** — If secrets were leaked, remove artifact from version control history immediately. | Engineering |
| 5 | **Notify** — Alert team channel: "Track D.<N> rehearsal failed; cadence paused." | Engineering |

### 2.4 Investigation Checklist

Before filing a bug, confirm the following:

| # | Check | Command / Method |
|---|-------|------------------|
| I1 | Failure is reproducible | Re-run the exact same command with the same fixture |
| I2 | Failure is not due to stale fixture | Regenerate fixture from latest schema and re-run |
| I3 | Failure is not due to environment drift | `docker compose down -v` and recreate PostgreSQL container |
| I4 | Failure is not due to transient resource exhaustion | Check `docker stats` for memory/disk limits during run |
| I5 | Schema matches latest migration | `sqlx migrate info` against empty target shows all migrations applied |
| I6 | ferrum-migrate binary is current | `git log --oneline -1` in repo matches expected commit |

If the failure is **not reproducible** after I1–I3, document the transient issue in the artifact, mark it as `FLAKY`, and schedule a retry within 24 hours. Do not resume full cadence until a clean pass is recorded.

### 2.5 Bug Filing Template

If the failure is reproducible, file a bug with this structure:

```markdown
## Bug: Track D.<N> Rehearsal Failure — <short description>

**Track**: D.1 / D.2 / D.3 / D.4
**Date**: YYYY-MM-DD
**Commit**: `<sha>`
**Severity**: P1 / P0

### Reproduction Steps
1. <step 1>
2. <step 2>

### Expected Result
<what the acceptance criteria require>

### Actual Result
<what happened>

### Logs / Evidence
<link to artifact or paste sanitized output>

### Investigation Done
- [ ] I1 reproducible
- [ ] I2 fixture fresh
- [ ] I3 environment recreated
- [ ] I4 resources sufficient
- [ ] I5 schema current
- [ ] I6 binary current

### Proposed Fix
<if known>

### Blocking
- Track D cadence paused until this bug is resolved and rehearsal re-run passes.
```

### 2.6 Fix and Re-Run Gate

A failed rehearsal track cannot resume until:

| Gate | Criterion | Verification |
|------|-----------|------------|
| F1 | Bug root cause identified and documented | Bug ticket updated with RCA |
| F2 | Fix merged to main | PR merged, CI green |
| F3 | Re-run of the **same** track passes all acceptance criteria | New artifact committed per D.5 |
| F4 | No secrets in new artifact | Manual review of artifact markdown |

Only after F1–F4 are satisfied may the normal cadence resume.

### 2.7 Escalation Path

| Scenario | Escalation | Owner |
|----------|------------|-------|
| Migration bug cannot be root-caused within 4 hours | Escalate to senior engineer / architect | Engineering Lead |
| Data loss or hash mismatch suspected in production-like data | Escalate to operator; freeze any target-host plans | Engineering Lead + Operator |
| Secrets leaked in artifact | Immediate security review; rotate any exposed credentials | Engineering Lead + Security |
| Schema drift blocks all Track D rehearsals | Escalate to schema owner; consider halting pilot work | Engineering Lead |

### 2.8 Non-Claims

- This process does **not** guarantee that all bugs will be found before target-host execution.
- This process does **not** authorize production PostgreSQL deployment.
- This process does **not** close operator-owned target-host blockers.
- Escalation to operator does **not** imply operator must switch to Option B.

---

## 3. Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `119-postgresql-readiness-operations-cadence.md` | `117-postgresql-readiness-acceleration-plan.md` | Parent plan (Track D) |
| `119-postgresql-readiness-operations-cadence.md` | `artifacts/2026-05-12-d1-populated-local-migration-rehearsal-evidence.md` | Example artifact (D.1) |
| `119-postgresql-readiness-operations-cadence.md` | `artifacts/2026-05-12-d4-large-dataset-streaming-rehearsal-evidence.md` | Example artifact (D.4) |
| `119-postgresql-readiness-operations-cadence.md` | `50-p4-postgres-store-facade-adr.md` | Schema and migration baseline |

---

## 4. Document History

| Date | Change | Author |
|------|--------|--------|
| 2026-05-13 | D.5 rehearsal artifact cadence process and D.6 failure handling/escalation process documented | Engineering |

---

*Document created: 2026-05-13. PostgreSQL Readiness Operations Cadence — process only. No execution claimed. No production-ready claim. No PostgreSQL production deployment claim. No HA/multi-node claim.*
