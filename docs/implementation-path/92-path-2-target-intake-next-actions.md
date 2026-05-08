# 92 — Path 2 Target Intake: Next Actions Plan

> **Status**: Created 2026-05-08 — actionable Path 2 target intake plan
> **Purpose**: Convert Path 2 target-value intake into an executable todo-list with explicit owners, blockers, and completion criteria
> **Scope**: Single-node SQLite Path 2 only. RC-ready/conditional. No G2/pilot/production-ready claim.
> **Constraint**: Do not fill real target values here. Do not add secrets. Engineering must not populate canonical docs 54/58/59/63/65 with dummy values or unverified real values; operator-owned working copies may be filled only from real target inputs.

---

## 1. Overview

This document provides an actionable next-steps plan for the Path 2 target-value intake phase. It translates the intake checklist in doc71 into a phased todo-list with explicit owners, blockers, and stop conditions.

**Recommended next action after MCP approve/reject completion**: Proceed with Path 2 target intake per this plan.

**Non-goals**: This plan does not execute Path 2. It does not populate docs 63/65 with real values. It does not authorize G2, pilot, or production-ready.

---

## 2. Phase Structure

| Phase | Focus | Owner |
|-------|-------|-------|
| Phase A | Intake Preparation | Engineering + Operator |
| Phase B | Operator Input Collection | Operator |
| Phase C | Validation | Engineering + Operator |
| Phase D | Target Execution Readiness | Operator |
| Phase E | Evidence Capture Readiness | Operator |
| Phase F | G2/Signoff Blocked Gates | Operator |

---

## 3. Phase A — Intake Preparation

**Purpose**: Prepare the repo-side materials the operator needs before collecting real target values.

### A.1 Confirm doc71 Completeness

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | P1 |
| **Status** | ✅ Complete |
| **Inputs needed** | doc71 (current version) |
| **Target docs/files** | doc71, doc65, doc63 |
| **Completion criteria** | All Critical and High fields in doc71 are present and legible |
| **Verification** | Engineering review of doc71 completeness |
| **Blockers** | None |
| **Evidence** | Artifact: [`2026-05-08-path2-phase-a-pre-target-gate.md`](./artifacts/2026-05-08-path2-phase-a-pre-target-gate.md) |

### A.2 Confirm doc66 Phase A is Complete

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | P1 |
| **Status** | ✅ Complete |
| **Inputs needed** | doc66 |
| **Target docs/files** | doc66 |
| **Completion criteria** | Phase A completion criteria in doc66 §A.3 are satisfied |
| **Verification** | All artifacts listed in doc66 §A.1 are present; no real secrets in docs |
| **Blockers** | None |
| **Evidence** | Artifact: [`2026-05-08-path2-phase-a-pre-target-gate.md`](./artifacts/2026-05-08-path2-phase-a-pre-target-gate.md) |

### A.3 Confirm Pre-Target Gate Passes

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | P2 |
| **Status** | ✅ Complete |
| **Inputs needed** | `scripts/run_pre_target_gate.sh` |
| **Target docs/files** | configs/examples/*.toml, configs/examples/*.service, configs/examples/*.conf |
| **Completion criteria** | `bash scripts/run_pre_target_gate.sh` passes locally |
| **Verification** | Script exit code 0; "ALL LOCAL CHECKS PASSED" |
| **Blockers** | None (local validation only) |
| **Evidence** | Artifact: [`2026-05-08-path2-phase-a-pre-target-gate.md`](./artifacts/2026-05-08-path2-phase-a-pre-target-gate.md) |

### A.4 Verify Local Dummy Rehearsal Still Passes (Optional)

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | P3 |
| **Status** | ☐ Optional |
| **Inputs needed** | `scripts/run_dummy_path2_rehearsal.sh` |
| **Target docs/files** | doc69, path2-dummy-rehearsal-bundle/ |
| **Completion criteria** | Dummy rehearsal script runs without error |
| **Verification** | Script exit code 0 |
| **Blockers** | None (local-only sanity check) |
| **Note** | This is a local rehearsal, NOT evidence of target readiness |

---

## 4A. Alternative Path — No Real Target Values Available

If real target values (host, credentials, paths) are not yet available from doc71, the **local target profile** provides a local-only alternative to validate tooling and runbook steps:

| Item | Detail |
|------|--------|
| **Script** | `bash scripts/run_local_path2_target_profile.sh --keep-output` |
| **Documentation** | [`doc93`](./93-local-path2-target-profile-plan.md) |
| **Artifact** | [`artifacts/2026-05-08-local-path2-target-profile.md`](./artifacts/2026-05-08-local-path2-target-profile.md) |
| **What it validates** | Profile structure, config/env, ferrumd start, probes, auth checks, backup/restore, auth smoke |
| **What it does NOT produce** | Real target values, G2 evidence, operator signoff, production-ready |

**This remains LOCAL-ONLY.** It does NOT constitute target evidence, G2 completion, or pilot authorization. Real target values from doc71 are still required for Phase B on a real target.

**When real target values become available**, proceed with standard Phase B using doc71.

---

## 4. Phase B — Operator Input Collection

**Purpose**: Operator provides all Critical and High severity inputs from doc71.

### B.1 Collect Critical — Operator Identity

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | Operator name, role, email |
| **Target docs/files** | doc65 §A, doc71 Critical fields |
| **Completion criteria** | Operator name + role + contact collected |
| **Verification** | Recorded in operator's working copy |
| **Blockers** | None |
| **Stop condition** | If operator declines to provide identity, Path 2 cannot proceed |

### B.2 Collect Critical — Target Host Access

| Item | Detail |
|------|--------|
| **Owner** | Operator / Infra |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | Target host FQDN or IP, SSH access method, key path or bastion |
| **Target docs/files** | doc65 §B, doc71 Critical fields |
| **Completion criteria** | Target host confirmed reachable via SSH |
| **Verification** | Successful SSH connection to target |
| **Blockers** | None |
| **Stop condition** | If no target host access, Path 2 cannot proceed |

### B.3 Collect Critical — Service Configuration

| Item | Detail |
|------|--------|
| **Owner** | Operator / Infra |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | OS + version, systemd availability, service user/group, install directory, config file path |
| **Target docs/files** | doc63 §Service, doc71 Critical fields |
| **Completion criteria** | All service configuration values confirmed |
| **Verification** | systemd available on target; paths confirmed writable |
| **Blockers** | None |

### B.4 Collect Critical — Auth and Storage

| Item | Detail |
|------|--------|
| **Owner** | Operator / Security |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | SQLite store path, bearer token generation, auth_mode confirmation |
| **Target docs/files** | doc63 §Storage, doc65 §Auth, doc71 Critical fields |
| **Completion criteria** | Bearer token generated (`openssl rand -hex 32`); token never committed; SQLite path confirmed |
| **Verification** | Token generated outside repo; path confirmed writable |
| **Blockers** | None |
| **Stop condition** | If bearer token cannot be generated securely, Path 2 cannot proceed |

### B.5 Collect High — TLS and Proxy Configuration

| Item | Detail |
|------|--------|
| **Owner** | Operator / Security / Infra |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | TLS certificate + key paths, public domain, DNS status, nginx config |
| **Target docs/files** | doc63 §Proxy, doc65 §C + §H, doc71 High fields |
| **Completion criteria** | TLS certs available; domain resolves; nginx config prepared |
| **Verification** | TLS certs readable by nginx; DNS A/AAAA records confirmed |
| **Blockers** | Deferred if TLS certs are not yet available; target execution remains constrained to the operator-approved access model until TLS/proxy evidence exists |

### B.6 Collect High — Backup and Recovery Configuration

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | Backup output directory, backup schedule mechanism, backup retention policy, RPO/RTO targets |
| **Target docs/files** | doc65 §Backup, doc71 High fields |
| **Completion criteria** | Backup output directory confirmed; schedule mechanism defined; RPO/RTO agreed |
| **Verification** | Backup directory writable; schedule mechanism confirmed |
| **Blockers** | Deferred if backup output directory not yet available |

### B.7 Collect High — Workload Model

| Item | Detail |
|------|--------|
| **Owner** | Operator / Product |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | Expected and peak writes/sec; workload model confirming single-node SQLite is acceptable |
| **Target docs/files** | doc65 §Workload, doc71 High fields |
| **Completion criteria** | Workload model confirms ≤300 writes/s sustained |
| **Verification** | Workload model documented and accepted |
| **Blockers** | None |
| **Stop condition** | If workload exceeds SQLite capacity, Path 3 (Phase 3 PostgreSQL) should be considered |

---

## 5. Phase C — Validation

**Purpose**: Validate collected inputs before populating any target documents.

### C.1 Validate No Secrets in Repo

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | Collected bearer token (outside repo), operator's working copies |
| **Target docs/files** | All docs in repo |
| **Completion criteria** | No bearer tokens, private keys, or secrets appear in any repo file |
| **Verification** | `git status` shows no modified secret files; token stored only in operator's env/secrets manager |
| **Blockers** | None |
| **Note** | This is the operator's responsibility; engineering can advise |

### C.2 Validate SQLite Capacity Fit

| Item | Detail |
|------|--------|
| **Owner** | Engineering + Operator |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | Workload model (writes/sec peak and sustained) |
| **Target docs/files** | doc27 §1.2, doc71 High fields |
| **Completion criteria** | Peak writes/sec ≤ 300 sustained; or explicit operator acknowledgment of SQLite ceiling |
| **Verification** | Documented acceptance |
| **Blockers** | None |

### C.3 Validate Target Environment Readiness

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | Target host, SSH access, service user, install directory, SQLite path |
| **Target docs/files** | doc63, doc65 |
| **Completion criteria** | All paths confirmed writable by service user |
| **Verification** | Operator confirms via SSH |
| **Blockers** | None |

---

## 6. Phase D — Target Execution Readiness

**Purpose**: Confirm readiness to execute on target host before beginning deployment.

### D.1 Confirm All Critical Intake Fields Complete

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | All Critical fields from doc71 |
| **Target docs/files** | doc71, operator's working copy |
| **Completion criteria** | 100% of Critical fields collected |
| **Verification** | Operator sign-off checklist |
| **Blockers** | Any missing Critical field blocks Phase D |

### D.2 Confirm All High Intake Fields Complete or Deferred

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | All High fields from doc71 |
| **Target docs/files** | doc71, operator's working copy |
| **Completion criteria** | 100% of High fields collected OR explicitly deferred with rationale |
| **Verification** | Operator documents deferred High fields with rationale |
| **Blockers** | None (deferral allowed with rationale) |

### D.3 Confirm Pre-Deployment Checklist Complete

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | doc71 §Pre-Deployment Intake Checklist |
| **Target docs/files** | doc71, operator's working copy |
| **Completion criteria** | All boxes in doc71 §Pre-Deployment Intake Checklist are checked |
| **Verification** | Operator signs off on checklist |
| **Blockers** | None |

---

## 7. Phase E — Evidence Capture Readiness

**Purpose**: Confirm evidence capture mechanisms are in place before target execution.

### E.1 Confirm Evidence Output Directory

| Item | Detail |
|------|--------|
| **Owner** | Operator |
| **Priority** | Critical |
| **Status** | ☐ Pending |
| **Inputs needed** | Evidence output directory path |
| **Target docs/files** | doc71 Critical fields |
| **Completion criteria** | Evidence output directory exists and is writable |
| **Verification** | Operator confirms directory exists |
| **Blockers** | None |

### E.2 Confirm Evidence Skeleton Generator Available

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | `scripts/generate_evidence_skeleton.py` |
| **Target docs/files** | scripts/generate_evidence_skeleton.py |
| **Completion criteria** | Script exists and runs without error |
| **Verification** | `python3 scripts/generate_evidence_skeleton.py --help` or equivalent |
| **Blockers** | None |

### E.3 Confirm G2 Evidence Packet Templates Available

| Item | Detail |
|------|--------|
| **Owner** | Engineering |
| **Priority** | High |
| **Status** | ☐ Pending |
| **Inputs needed** | doc59 (G2.1–G2.8 templates), doc54 (operator signoff) |
| **Target docs/files** | doc59, doc54 |
| **Completion criteria** | Templates exist and are operator-fillable (no pre-populated values) |
| **Verification** | Engineering review confirms templates are blank |
| **Blockers** | None |

---

## 8. Phase F — G2/Signoff Blocked Gates

**Purpose**: Explicitly document that G2 and operator signoff cannot occur until after target execution.

### F.1 G2 Evidence Gates — Explicitly Blocked

| Gate | Blocker | Resolution |
|------|---------|------------|
| G2.1 Write workload modeled | Pending target workload data | Operator provides after pilot |
| G2.2 Bearer auth + TLS confirmed | Pending target deployment | Operator confirms after deploy |
| G2.3 Backup schedule implemented | Pending operator config | Operator configures after deploy |
| G2.4 Restore drill completed | Pending target execution | Operator executes after deploy |
| G2.5 RPO/RTO accepted | Pending workload model | Operator accepts after modeling |
| G2.6 Production evaluation satisfied | Pending all G2 evidence | Operator reviews after drills |
| G2.7 Accepted risks documented | Pending operator review | Operator reviews doc 19 §4 |
| G2.8 Compensate risk accepted | Pending target adapter drills | Operator confirms per adapter |

**No G2 gate can be marked complete before target execution.**

### F.2 Operator Signoff — Explicitly Blocked

| Item | Blocker | Resolution |
|------|---------|------------|
| Doc 54 signoff | G2 evidence must exist first | Operator signs only after G2 complete |
| Pilot authorization | Doc 54 must be signed | Pilot authorized only after operator signoff |

**No pilot authorized until operator explicitly signs doc 54.**

### F.3 Stop Conditions

Path 2 must stop/abort if any of the following occur:

| Condition | Action |
|-----------|--------|
| Operator declines to provide identity | Stop — accountability required |
| No target host access available | Stop — deployment impossible |
| Bearer token cannot be generated securely | Stop — auth required |
| Workload exceeds SQLite capacity (>300 writes/s sustained) | Evaluate Path 3 PostgreSQL |
| RPO/RTO cannot be met with single-node SQLite | Evaluate Path 3 PostgreSQL |
| Any Critical intake field cannot be collected | Stop — deployment blocked |

---

## 9. Canonical Docs 54/58/59/63/65 Population Guardrails

These documents must NOT be populated with dummy or real target values by engineering.

| Doc | Purpose | Population Rule |
|-----|---------|-----------------|
| doc54 | Operator signoff | Operator fills only; no pre-population |
| doc58 | Compensation drill template | Operator fills only; no pre-population |
| doc59 | G2 evidence packet | Operator fills only; no pre-population |
| doc63 | Target environment spec | Operator fills from doc71 inputs |
| doc65 | Target questionnaire | Operator fills from doc71 inputs |

**Dummy values in docs 69/path2-dummy-rehearsal-bundle are LOCAL-TEST ONLY and must never be copied into canonical docs 54/58/59/63/65.**

---

## 10. Summary: Recommended Next Action

With Phase A complete (A.1 ✅ doc71 complete, A.2 ✅ doc66 Phase A complete, A.3 ✅ pre-target gate passed — evidence in [`2026-05-08-path2-phase-a-pre-target-gate.md`](./artifacts/2026-05-08-path2-phase-a-pre-target-gate.md)), the recommended next action is:

1. **Operator**: Begin Phase B — collect Critical fields from doc71 (operator identity, target host access, service configuration, auth/storage)
2. **Operator**: Complete doc71 Pre-Deployment Intake Checklist
3. **Operator**: Once all Critical fields are collected, proceed to Phase D readiness checks before target execution

Engineering bridge work is complete: doc71 links this action plan, doc66 links this action plan at Phase B, and the local pre-target gate artifact is recorded.

**Phase A is complete. Next action is operator-owned Phase B Critical field collection from doc71.**

**Path 2 is the recommended next step.** Hardening tracks (MCP error sanitization, DLP semantic scanning, HTTP retry/backoff) remain deferred per doc91 §8.

---

## 11. Linked Documents

| This Doc | Links To | Purpose |
|----------|----------|---------|
| This doc | [doc71](./71-path-2-target-values-intake-packet.md) | Source intake checklist |
| This doc | [doc66](./66-path-2-operator-handoff.md) | Phase A/B boundary |
| This doc | [doc91](./91-proposal-todo-status-after-mcp-approve-reject.md) | Proposal status |
| This doc | [doc65](./65-path-2-target-questionnaire.md) | Operator input template |
| This doc | [doc63](./63-path-2-target-environment-spec.md) | Target spec template |
| This doc | [doc54](./54-operator-signoff-packet.md) | Final signoff (operator-only) |
| This doc | [doc59](./59-pilot-readiness-evidence-packet.md) | G2 evidence (operator-only) |
| This doc | [README.md](./README.md) | Entry point |

---

*Document created: 2026-05-08. Path 2 target intake next actions plan. No G2/pilot/production-ready claim.*
