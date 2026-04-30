# 59 — Pilot Readiness Evidence Packet

> **Status**: Created 2026-04-29 — documentation-only template
> **Scope**: Single-node v1 SQLite production pilot readiness
> **Constraint**: RC-ready/conditional single-node SQLite only. Not production-ready. G2 gates require explicit operator signoff. Do not mark complete on behalf of operator.
> **Purpose**: G2.1–G2.8 evidence sections for Path 2 (Conditional Production Pilot) per `31-release-paths-todo.md`

---

## Local Staging-Like Readiness Prefill (Pending Operator Review/Signoff)

> **Operator review required**: This section summarizes local smoke evidence captured on 2026-04-29. It does **not** complete G2 and does **not** authorize production pilot deployment. All G2 items remain pending operator signoff.
>
> **Automated Drill Runner**: `python3 scripts/run_d1_d6_drills.py` automates local D1–D6 evidence collection. Run with `--server-url http://127.0.0.1:8080` to include optional server smoke probes. Output is labeled "local/test-drill" and requires operator review per docs 58/59.

### Local Smoke Summary

| Item | Local Evidence | Status |
|------|----------------|--------|
| Readiness helper | `scripts/check_pilot_readiness.py --server-url http://127.0.0.1:18080` | PASS locally |
| Shallow readiness | `/v1/readyz` via `ferrumctl server readiness` | PASS locally |
| Deep readiness | `/v1/readyz/deep` via `ferrumctl server readiness --deep` | PASS locally |
| Functional readiness | `/v1/approvals?limit=1` via `ferrumctl server readiness --functional` | PASS locally |
| Metrics endpoint | `/v1/metrics` contained expected v1 metrics | PASS locally |
| D1–D6 drill prefill | See `58-workload-compensation-drill-evidence-template.md` local prefill | Pending operator review |

**Local smoke environment**: `ferrumd 127.0.0.1:18080`, SQLite in-memory, `auth_mode=disabled`, repo `d7f19ea44a530ef6d7982402862c855fa1ea0849`.

### G2 Status After Local Prefill

| Gate | Current status | Reason |
|------|----------------|--------|
| G2.1 Workload Model | Pending operator signoff | Requires operator workload model for target deployment |
| G2.2 Auth/TLS Configuration | Pending operator signoff | Local smoke used `auth_mode=disabled`; target bearer/TLS still operator-owned |
| G2.3 Backup Schedule | Pending operator signoff | External backup scheduling remains operator-owned |
| G2.4 Restore Drill | Partially prefilled / pending operator signoff | Backup dry-run/local helper exists; real restore drill evidence must be reviewed |
| G2.5 RPO/RTO Acceptance | Pending operator signoff | Requires target workload SLA acceptance |
| G2.6 Production Evaluation | Pending operator signoff | Evaluation framework remains operator-owned |
| G2.7 Accepted-Risk Review | Pending operator signoff | Risks require explicit operator acceptance |
| G2.8 Compensate Noop Acceptance | Pending operator signoff | D1–D6 prefill requires operator review and signature |

**No G2 complete claim. No production-ready claim. No pilot authorization is made by this prefill.**

---

## Purpose

This packet provides fillable evidence sections for the G2 gates defined in Path 2 of `31-release-paths-todo.md`. Each section captures evidence of operator-verified readiness dimensions before the production pilot begins.

**This document is documentation-only.** Completing these sections does not claim production readiness and does not authorize deployment. It confirms the operator has evaluated and accepted the known constraints documented in `19-v1-single-node-support-contract.md`.

**Do not mark G2 items complete on behalf of the operator.** Each item requires explicit operator acknowledgment and, where indicated, documented accepted-risk signoff.

---

## G2 Readiness Overview

> **Optional Automated Prefill/Probe Helper**: `python3 scripts/check_pilot_readiness.py` runs shallow, deep, functional, and metrics probes via `ferrumctl` or HTTP and reports pass/fail status. This is an **optional prefill/probe aid only** — it does **NOT** complete G2/operator signoff. Operator review and explicit signoff is still required for all G2 gates.

| Gate | Title | Evidence Required | Status |
|------|-------|-------------------|--------|
| G2.1 | Workload Model | Write workload modeled against SQLite capacity | [ ] Pending operator signoff |
| G2.2 | Auth/TLS Configuration | Bearer auth + TLS/reverse proxy confirmed | [ ] Pending operator signoff |
| G2.3 | Backup Schedule | External backup scheduling implemented | [ ] Pending operator signoff |
| G2.4 | Restore Drill | Restore drill with `PRAGMA integrity_check` passing | [ ] Pending operator signoff |
| G2.5 | RPO/RTO Acceptance | Backup/restore objectives formally accepted | [ ] Pending operator signoff |
| G2.6 | Production Evaluation | Evaluation framework completed (all dimensions SATISFIED or CONDITIONAL) | [ ] Pending operator signoff |
| G2.7 | Accepted-Risk Review | Weak Spots 1–4 reviewed; risks accepted | [ ] Pending operator signoff |
| G2.8 | Compensate Noop Acceptance | Compensate noop risk accepted for target adapters | [ ] Pending operator signoff |

---

## G2.1 — Workload Model

### Evidence Reference
`54-operator-signoff-packet.md` §Template 1 + `27-production-evaluation-plan.md` §1.2

### Evidence Fields

**Operator Information:**
| Field | Value |
|-------|-------|
| Operator name | _______________________________ |
| Target deployment environment | _______________________________ |
| Date | _______________________________ |

**Workload Metrics:**
| Metric | Expected Value | SQLite Phase 1 Limit |
|-------|---------------|---------------------|
| Expected sustained write rate | _____ writes/s | ≤300 writes/s |
| Expected peak write rate | _____ writes/s | — |
| Expected daily write volume | _____ writes/day | — |
| Expected execution history size at steady state | _____ records | Bounded by file size |

**Workload Fit Assessment:**
| Assessment | Result |
|------------|--------|
| Sustained write rate fits within ≤300 writes/s | [ ] YES  [ ] NO |
| Single-node topology confirmed acceptable | [ ] YES  [ ] NO |
| Workload requires PostgreSQL (Path 3) | [ ] YES  [ ] NO |

**Pre-fill completed by (engineer)**: _______________________________ Date: _________________

### Operator Signoff
> **G2.1 Signoff phrase**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

Operator signature: _________________ Date: _________

---

## G2.2 — Authentication and Transport Security

### Evidence Reference
`54-operator-signoff-packet.md` §2 + `27-production-evaluation-plan.md` §2.1

### Evidence Fields

**Bearer Token Configuration:**
| Item | Evidence Required | Captured Value |
|------|-------------------|----------------|
| `auth_mode` setting | Config file excerpt showing `"Bearer"` | _________________ |
| `FERRUMD_BEARER_TOKEN` or config token | Token present (redacted in evidence) | [ ] YES  [ ] NO |
| Token generation command | `openssl rand -hex 32` or equivalent | _________________ |

**TLS/Reverse Proxy Configuration:**
| Item | Evidence Required | Status |
|------|-------------------|--------|
| FerrumGate behind TLS-terminating reverse proxy | Network/firewall documentation | [ ] Confirmed  [ ] Not confirmed |
| Reverse proxy TLS certificate | Certificate details | _________________ |
| Exposed endpoints | Only `/v1/healthz`, `/v1/readyz` (unauthenticated); all other routes require auth | [ ] Confirmed  [ ] Not confirmed |

**Health Endpoints Acknowledgment:**
| Endpoint | Intentionally Unauthenticated | Operator Acknowledged |
|----------|--------------------------------|----------------------|
| `/v1/healthz` | [ ] YES | [ ] YES |
| `/v1/readyz` | [ ] YES | [ ] YES |

### Operator Signoff
> **G2.2 Signoff phrase**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

Operator signature: _________________ Date: _________

---

## G2.3 — Backup Schedule Evidence

### Evidence Reference
`54-operator-signoff-packet.md` §3 + `27-production-evaluation-plan.md` §3.5

### Evidence Fields

**Backup Scheduling (External to FerrumGate):**
| Item | Evidence Required | Captured Evidence |
|------|-------------------|-------------------|
| Backup tool | `ferrumctl backup create` confirmed available | [ ] Confirmed |
| Scheduling method | [ ] cron  [ ] CI job  [ ] manual  [ ] other: _____ | _________________ |
| Schedule frequency | Backup runs every _____ | _________________ |
| Backup job evidence | Cron entry, CI job log, or manual run log | _________________ |

**Cron Entry Example (if applicable):
```bash
# FerrumGate backup schedule
0 */6 * * * /usr/local/bin/ferrumctl backup create --output /backups/ferrumgate-$(date +\%Y\%m\%d\%H\%M\%S).db
```

**CI Job Evidence (if applicable):
```yaml
# .gitlab-ci.yml or similar
backup:
  script:
    - ferrumctl backup create --output "backups/$(date +%Y%m%d%H%M%S).db"
  only:
    - schedules
  ...
```

### Operator Signoff
> **G2.3 Signoff phrase**: "Operator has implemented external backup scheduling for FerrumGate."

Operator signature: _________________ Date: _________

---


### Local Non-Prod Restore Drill Prefill (Pending Operator Review)

> **Operator review required**: This local restore drill used temporary SQLite files under `/tmp/ferrum-restore-drill`. It demonstrates the backup/restore workflow but does **not** complete G2.4 for a target deployment.

| Step | Evidence | Status |
|------|----------|--------|
| Backup create | `ferrumctl backup create --db-path /tmp/ferrum-restore-drill/source.db --output-dir /tmp/ferrum-restore-drill/backups` | PASS |
| Backup verify | `ferrumctl backup verify --db-path <backup>` returned `OK` | PASS |
| Restore with confirm | `ferrumctl backup restore --db-path /tmp/ferrum-restore-drill/target.db --from <backup> --confirm` | PASS |
| Exclusive lock precheck | Restore reported `Exclusive lock check passed` | PASS |
| Pre-restore copy | Restore saved `/tmp/ferrum-restore-drill/target.db.pre_restore` | PASS |
| Restored DB verify | `ferrumctl backup verify --db-path /tmp/ferrum-restore-drill/target.db` returned `OK` | PASS |
| Data verification | Query returned `backup-source` after restore | PASS |

**Raw evidence log**: `/tmp/ferrum-restore-drill/restore-drill.log`

**Boundary**: Target-environment restore drill remains operator-owned before G2.4 signoff.

## G2.4 — Restore Drill Evidence

### Evidence Reference
`54-operator-signoff-packet.md` §Template 3 + `27-production-evaluation-plan.md` §3.5

### Evidence Fields

**Drill Environment:**
| Field | Value |
|-------|-------|
| Drill environment | _______________________________ |
| Date of drill | _______________________________ |
| Backup file used | _______________________________ |
| Backup timestamp | _______________________________ |

**Drill Steps Performed:**
| Step | Action | Status |
|------|--------|--------|
| 1 | Non-production environment confirmed isolated from live store | [ ] DONE |
| 2 | `ferrumctl backup restore --confirm` executed | [ ] DONE |
| 3 | Exclusive lock detection triggered correctly (refused if server running) | [ ] DONE |
| 4 | Pre-restore copy preserved | [ ] DONE |
| 5 | `PRAGMA integrity_check` passed on restored DB | [ ] DONE |
| 6 | Execution lineage queryable after restore | [ ] DONE |
| 7 | Approval queue readable after restore | [ ] DONE |

**Restore Drill Output Capture:**
```bash
# Pre-restore: Server must be stopped
$ ferrumctl backup restore --backup /backups/ferrumgate-20260429.db --confirm
Stopping server... done
Pre-restore copy saved: /tmp/pre-restore-20260429.db
Restoring from: /backups/ferrumgate-20260429.db
Restore complete.

# Post-restore verification
$ sqlite3 /backups/ferrumgate-20260429.db "PRAGMA integrity_check;"
ok
```

**Drill Outcome:**
| Outcome | Status |
|---------|--------|
| SUCCESS — All steps passed | [ ] |
| PARTIAL — Issues encountered | [ ] |
| FAILED — Drill failed | [ ] |

If PARTIAL or FAILED, describe issues: _______________________________

### Operator Signoff
> **G2.4 Signoff phrase**: "Operator has performed a restore drill, confirmed `PRAGMA integrity_check` passes, and verified pre-restore copy is preserved."

Operator signature: _________________ Date: _________

---

## G2.5 — RPO/RTO Acceptance

### Evidence Reference
`54-operator-signoff-packet.md` §3 + `27-production-evaluation-plan.md` §3.5

### Evidence Fields

**RPO (Recovery Point Objective):**
| Item | Value |
|------|-------|
| Backup interval | _____ minutes/hours |
| RPO = time since last backup | _____ minutes/hours |
| Writes lost on restore (worst case) | All writes after last backup |
| RPO acceptable for target workload SLA | [ ] YES  [ ] NO |

**RTO (Recovery Time Objective):**
| Item | Value |
|------|-------|
| Estimated restore time | _____ minutes |
| Estimated restart time | _____ minutes |
| Estimated verification time | _____ minutes |
| Total RTO | _____ minutes |
| FerrumGate automated recovery available | [ ] NO — operator-driven |
| RTO acceptable for target workload SLA | [ ] YES  [ ] NO |

**Acceptance Statement:**
> I understand that:
> - RPO = time since last backup; any writes after last backup are lost on restore
> - RTO = restore time + restart + verification; FerrumGate has no automated recovery
> - I am responsible for defining and testing backup/restore procedures

### Operator Signoff
> **G2.5 Signoff phrase**: "Operator confirms RPO and RTO fit for the target workload and backup retention policy (including scheduling and offsite needs) is operator-defined."

Operator signature: _________________ Date: _________

---

## G2.6 — Production Evaluation Framework

### Evidence Reference
`54-operator-signoff-packet.md` §Template 2 + `27-production-evaluation-plan.md` Evaluation Decision Framework

### Evidence Fields

**Evaluation Dimensions:**

| Dimension | Rating | Notes / Compensating Controls |
|-----------|--------|------------------------------|
| 1 — Performance | [ ] SATISFIED  [ ] CONDITIONAL  [ ] NOT MET  [ ] N/A | _______________________________ |
| 2 — Security | [ ] SATISFIED  [ ] CONDITIONAL  [ ] NOT MET  [ ] N/A | _______________________________ |
| 3 — Reliability | [ ] SATISFIED  [ ] CONDITIONAL  [ ] NOT MET  [ ] N/A | _______________________________ |
| 4 — Operations | [ ] SATISFIED  [ ] CONDITIONAL  [ ] NOT MET  [ ] N/A | _______________________________ |
| 5 — Release Confidence | [ ] SATISFIED  [ ] CONDITIONAL  [ ] NOT MET  [ ] N/A | _______________________________ |

**Overall Assessment:**
| Item | Status |
|------|--------|
| All critical items SATISFIED or CONDITIONAL (with controls)? | [ ] YES  [ ] NO |
| NOT MET items blocking pilot? | [ ] NONE  [ ] YES — resolve before proceeding |

If NOT MET items exist, list and remediation: _______________________________

**Full evaluation framework reference**: `27-production-evaluation-plan.md` Evaluation Decision Framework

### Operator Signoff
> **G2.6 Signoff phrase**: "All production evaluation dimensions are SATISFIED or CONDITIONAL (with controls)."

Operator signature: _________________ Date: _________

---

## G2.7 — Accepted-Risk Review

### Evidence Reference
`54-operator-signoff-packet.md` §Template 5 + `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`

### Evidence Fields

**Review Required Documents:**
- `19-v1-single-node-support-contract.md` §4 (Accepted Risks)
- `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` (Weak Spots 1–4)

**Weak Spot Review:**

| Weak Spot | Resolution | Operator Acknowledged |
|-----------|------------|----------------------|
| Weak Spot 1 — Rollback class handling | [ ] RESOLVED | [ ] YES |
| Weak Spot 2 — Draft-only revalidation | [ ] RESOLVED | [ ] YES |
| Weak Spot 3 — Scope-bounds enforcement | [ ] RESOLVED | [ ] YES |
| Weak Spot 4 — Provenance completeness | [ ] RESOLVED | [ ] YES |

**Additional Accepted Risks from `19-v1-single-node-support-contract.md` §4:**
| Risk | Accepted | Not Applicable |
|------|----------|---------------|
| Risk 1: _______________________________ | [ ] | [ ] |
| Risk 2: _______________________________ | [ ] | [ ] |
| Risk 3: _______________________________ | [ ] | [ ] |
| Risk 4: _______________________________ | [ ] | [ ] |

### Operator Signoff
> **G2.7 Signoff phrase**: "Operator has reviewed all Weak Spots and accepted risks documented in the support contract."

Operator signature: _________________ Date: _________

---

## G2.8 — Compensate Noop Acceptance

### Evidence Reference
`54-operator-signoff-packet.md` §Template 4 + `56-adapter-compensation-evidence-matrix.md`

### Evidence Fields

**Target Adapters in Scope:**
Adapter list: _______________________________

**Compensate Behavior Matrix:**

| Adapter / Action | Compensate performs real undo? | Verified by |
|-----------------|------------------------------|-------------|
| (adapter name) R0 | [ ] YES  [ ] NO  [ ] UNKNOWN | ___________ |
| (adapter name) R1 | [ ] YES  [ ] NO  [ ] UNKNOWN | ___________ |
| (adapter name) R2 | [ ] YES  [ ] NO  [ ] UNKNOWN | ___________ |
| (adapter name) R3 | [ ] YES  [ ] NO  [ ] UNKNOWN | ___________ |

**Compensate Noop Risk Acceptance** (only if some adapters are noop-backed):
> I acknowledge that `POST /v1/executions/{execution_id}/compensate` may return `200` with `recovered=true` without performing external undo for the following noop-backed adapters:
>
> List adapters: _______________________________
>
> I accept the compensate noop risk with the following manual verification procedure:
> Step 1: _______________________________
> Step 2: _______________________________
> Step 3: _______________________________

### Operator Signoff
> **G2.8 Signoff phrase**: "Operator accepts compensate noop risk for target adapters where compensation is not guaranteed external undo."

Operator signature: _________________ Date: _________

---

## G2 Final Gate Signoff

All G2.1–G2.8 gates above require operator signoff. Once all eight gates are signed:

### Pilot Acceptance Statement

> **Operator acceptance**: "I, [Operator Name], acting in my capacity as [Role], have evaluated FerrumGate v1 single-node SQLite against the production evaluation plan (`27-production-evaluation-plan.md`). I have reviewed and accepted all accepted risks documented in `19-v1-single-node-support-contract.md` §4 and the Weak Spots documented in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`. I confirm the workload fits within Phase 1 SQLite constraints, all G2 gates have been satisfied, and I accept the conditional production posture as described in `23-production-readiness-assessment.md`. I authorize the limited production pilot deployment as described in `31-release-paths-todo.md` §Path 2."

| Role | Signature | Date |
|------|-----------|------|
| Operator | _________________ | _________ |
| Owner/Supervisor (if required) | _________________ | _________ |

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- No production-ready claim is made in this document
- PostgreSQL/multi-node/HA are not implemented and not in scope
- Phase 2 transaction batching was deferred/regressed
- This evidence packet confirms operator evaluation only — it does not authorize deployment or claim production readiness

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `59-pilot-readiness-evidence-packet.md` | `31-release-paths-todo.md` §Path 2 | G2 gate definitions |
| `59-pilot-readiness-evidence-packet.md` | `54-operator-signoff-packet.md` | Signoff templates and phrases |
| `59-pilot-readiness-evidence-packet.md` | `27-production-evaluation-plan.md` | Evaluation framework |
| `59-pilot-readiness-evidence-packet.md` | `56-adapter-compensation-evidence-matrix.md` | Compensate classification |
| `59-pilot-readiness-evidence-packet.md` | `57-workload-compensation-drill-plan.md` | Drill procedures |
| `59-pilot-readiness-evidence-packet.md` | `58-workload-compensation-drill-evidence-template.md` | Drill evidence template |
| `59-pilot-readiness-evidence-packet.md` | `60-bounded-hardening-examples.md` | Bounded hardening examples |
| `59-pilot-readiness-evidence-packet.md` | `63-path-2-target-environment-spec.md` | Target environment spec (bridging from local simulation) |
| `59-pilot-readiness-evidence-packet.md` | `64-local-staging-simulation-guide.md` | Local simulation guide (bridging to target) |
| `59-pilot-readiness-evidence-packet.md` | `19-v1-single-node-support-contract.md` | Accepted risks |
| `59-pilot-readiness-evidence-packet.md` | `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` | Weak Spots |
| `59-pilot-readiness-evidence-packet.md` | `23-production-readiness-assessment.md` | RC-ready declaration |

---

*Document generated: 2026-04-29. Documentation-only — no production deployment performed. G2 gates require explicit operator signoff.*
