# 54 — Operator Signoff Packet

> **Status**: Documentation-only. No production deployment performed.
> **Purpose**: Standalone fillable operator signoff form for FerrumGate v1 single-node SQLite production pilot.
> **Scope**: Single-node SQLite only. No PostgreSQL/multi-node. No production-ready claim.
> **RC status**: v0.1.0-rc.2 published as GitHub prerelease (Path 1 complete; Path 2 signed by BrianNguyen on 09/05/2026 for conditional single-node SQLite pilot scope)

---

## Prepared-Signoff Note (Operator Action Required)

> **Updated status**: BrianNguyen signed this document on 09/05/2026. Values copied from signed doc 99 worksheet. All G2 gates signed for conditional single-node SQLite pilot scope only.
>
> **Current state**: Signed by BrianNguyen for conditional single-node pilot.
> **Scope**: Conditional single-node SQLite pilot only. NOT full production-ready. PostgreSQL/HA not in scope.
> **Next action**: None required for Phase 1 conditional pilot. Canonical docs 63/65 may be updated with target values as needed.
> **Boundary**: This signoff authorizes limited conditional single-node SQLite pilot only. No full production-ready claim.

---

## Purpose

This packet is the formal operator acceptance checklist before any production pilot deployment. It is **documentation-only** — completing these items does not claim production readiness; it confirms the operator has evaluated and accepted the known constraints.

**Do not mark these items complete on behalf of the operator.** Each item requires explicit operator acknowledgment and, where indicated, documented accepted-risk signoff.

---

## Evidence Attachment Fields

Attach the following evidence to each section before signing:

| Section | Evidence Item | File / Reference |
|---|---|---|
| §1 SQLite limits | Write workload model showing ≤300 writes/s sustained | Operator workload analysis document |
| §1 SQLite limits | Confirmation single-node topology confirmed acceptable | Operator statement |
| §2 Auth/TLS | Bearer token configuration confirmed | Config file excerpt (redacted token) |
| §2 Auth/TLS | TLS termination confirmed at reverse proxy | Network/firewall documentation |
| §3 Backup/restore | Backup schedule evidence (cron, CI job, or manual log) | Scheduled job or manual run log |
| §3 Backup/restore | Restore drill evidence with `PRAGMA integrity_check` passing | Restore drill report |
| §4 PostgreSQL deferred | Operator acknowledgment of PostgreSQL deferral | This document signed |
| §5 Pilot prerequisites | All 8 prerequisites verified | This document signed |

---

## Pilot Acceptance Statement (Final Signoff)

Before the production pilot begins, the operator must sign the following statement:

> **Operator acceptance**: "I, BrianNguyen, acting in my capacity as Operator/Owner, have evaluated FerrumGate v1 single-node SQLite against the production evaluation plan (`27-production-evaluation-plan.md`). I have reviewed and accepted all accepted risks documented in `19-v1-single-node-support-contract.md` §4 and the Weak Spots documented in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`. I confirm the workload fits within Phase 1 SQLite constraints, all G2 gates have been satisfied, and I accept the conditional production posture as described in `23-production-readiness-assessment.md`. I authorize the limited conditional single-node SQLite production pilot deployment as described in `31-release-paths-todo.md` §Path 2."
>
> **Scope**: Conditional single-node SQLite pilot only. NOT full production-ready. PostgreSQL/HA not in scope.
>
> Operator signature: BrianNguyen Date: 09/05/2026
>
> Owner/Supervisor countersignature (if required): N/A

---

## 1. SQLite Single-Node Limits — Operator Must Acknowledge

| Item | Required Action | Reference |
|---|---|---|
| Write throughput ceiling | Operator confirms expected sustained writes ≤300 writes/s; above this requires PostgreSQL (Phase 3) | `27-production-evaluation-plan.md` §1.2 |
| Single-node only | Operator acknowledges no multi-node/HA/replica support in v1 | Support contract §3 |
| Bounded execution history | Operator acknowledges SQLite file size and lineage traversal limits at scale | Support contract §3 |

**Signoff phrase required**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

**Evidence from doc 99**:
- Sustained write rate: ≤300 writes/s
- Peak write rate: ≤300 writes/s
- Daily write volume: ≤1M writes/day
- SQLite single-node fit: CONFIRMED

Operator signature: BrianNguyen Date: 09/05/2026

---

## 2. Authentication and Transport Security

| Item | Required Action | Reference |
|---|---|---|
| Bearer token mode | Operator confirms `auth_mode = "Bearer"` with operator-managed token; `FERRUMD_BEARER_TOKEN` or config file | `27-production-evaluation-plan.md` §2.1 |
| TLS/reverse proxy | Operator confirms FerrumGate is deployed behind a TLS-terminating reverse proxy (not exposed bare on internet) | `27-production-evaluation-plan.md` §2.1 |
| Health endpoints unauthenticated | Operator acknowledges `/v1/healthz` and `/v1/readyz` are intentionally unauthenticated; governance routes require auth | `27-production-evaluation-plan.md` §2.1 |

**Signoff phrase required**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

**Evidence from doc 99**:
- GCP non-prod: Caddy v2.11.2 active, TLS via Let's Encrypt on `34-158-51-8.nip.io` (temporary, non-prod only)
- Auth probe: no token → HTTP 401, with token → HTTP 200 (confirmed)
- Production note: nip.io temporary; real domain required for production

Operator signature: BrianNguyen Date: 09/05/2026

---

## 3. Backup, Restore, and Recovery Objectives

| Item | Required Action | Reference |
|---|---|---|
| Backup schedule outside FerrumGate | Operator implements backup scheduling external to FerrumGate (cron, CI job, etc.); `ferrumctl backup` does not support automated scheduling | `27-production-evaluation-plan.md` §3.5 |
| Backup retention | Operator defines retention policy; opt-in CLI retention pruning (`--retention-days N`) available | `27-production-evaluation-plan.md` §3.5 |
| Restore drill performed | Operator has run `ferrumctl backup restore` in a non-production environment and verified data integrity with `PRAGMA integrity_check` | Operations runbook §4 |
| RPO accepted | Operator understands RPO = time since last backup; any writes after last backup are lost on restore | `27-production-evaluation-plan.md` §3.5 |
| RTO accepted | Operator understands RTO includes backup restore time + re-start + verification; FerrumGate has no automated recovery | `27-production-evaluation-plan.md` §3.5 |

**Signoff phrase required**: "Operator has performed a restore drill, confirmed RPO/RTO fit for the target workload, and backup retention policy (including scheduling and offsite needs) is operator-defined."

**Evidence from doc 99**:
- Backup schedule: 15-minute systemd timer (`OnUnitActiveSec=15min`), timer enabled and active
- Retention policy: 7 days + offsite copy required before production
- Restore drill: `PRAGMA integrity_check=ok`, 14 tables, copy removed
- RPO accepted: 15 minutes
- RTO accepted: 15 minutes

Operator signature: BrianNguyen Date: 09/05/2026

---

## 4. PostgreSQL / Multi-Node Deferred Status

| Item | Required Action | Reference |
|---|---|---|
| PostgreSQL runtime support (local) | Operator acknowledges local PostgreSQL runtime support exists (`postgres://` DSNs connect at startup). P4.4 MVP complete (dry-run default, --apply, empty-target safety, count+ID validation); P5 production readiness (HA, multi-node, operator signoff) deferred | ADR-50, `31-release-paths-todo.md` |
| Multi-node/HA not implemented | Operator acknowledges v1 is single-node only; scale-out requires Phase 3 | ADR-50, Production roadmap |
| Phase 3 local runtime complete | Operator acknowledges local PostgreSQL runtime support (P3/P4.1–P4.3) is implemented. P4.4 MVP complete (dry-run default, --apply, empty-target safety, count+ID validation); P5 production readiness (HA, multi-node, operator signoff) deferred and not part of the current pilot | ADR-50 Phase P1, `31-release-paths-todo.md` |

**Signoff phrase required**: "Operator acknowledges PostgreSQL/multi-node is deferred and not part of the current production pilot scope."

**Scope note**: Conditional single-node SQLite pilot only. PostgreSQL/HA not in scope for Phase 1.

Operator signature: BrianNguyen Date: 09/05/2026

---

## 5. Production Pilot Prerequisites

Before the first production pilot deployment, the following must be all confirmed:

| # | Prerequisite | Verification |
|---|---|---|
| 1 | Write workload modeled against SQLite capacity (≤300 writes/s sustained) | §1 signoff above |
| 2 | Bearer auth + TLS/reverse proxy confirmed | §2 signoff above |
| 3 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` |
| 4 | Restore drill completed with `PRAGMA integrity_check` passing | Operator evidence of successful restore |
| 5 | RPO/RTO formally accepted for target workload | §3 signoff above |
| 6 | All production evaluation dimensions SATISFIED or CONDITIONAL (with controls) | `27-production-evaluation-plan.md` Evaluation Decision Framework completed |
| 7 | Accepted-risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed |
| 8 | Compensate noop risk formally accepted | Operator acknowledges compensate may be noop-backed for target adapters |

**This is not a full production-ready claim.** FerrumGate v1 is RC-ready/conditional for single-node SQLite only. Production pilot deployment is conditional on all eight prerequisites above being satisfied and is explicitly scoped to single-node SQLite. PostgreSQL/HA/multi-node are not in scope for Phase 1.

---

## Disclaimer

**FerrumGate v1 is RC-ready/conditional for single-node SQLite only.**

- No production-ready claim is made in this document
- PostgreSQL production deployment, multi-node, and HA are not implemented and not in scope; local PostgreSQL runtime support exists behind the non-default `postgres` feature
- Phase 2 transaction batching was deferred/regressed
- This signoff packet confirms operator evaluation only — it does not authorize deployment or claim production readiness

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `27-production-evaluation-plan.md` | Canonical production evaluation framework |
| `19-v1-single-node-support-contract.md` | Accepted risks §4, support constraints §3 |
| `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` | Weak Spots 1–4 resolved |
| `23-production-readiness-assessment.md` | RC-ready declaration |
| `31-release-paths-todo.md` §Path 2 | Full production pilot path with G2 gates |
| `58-workload-compensation-drill-evidence-template.md` | D1–D6 drill evidence capture template |
| `59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence sections |
| `60-bounded-hardening-examples.md` | Bounded hardening drill examples |

---

*Document generated: 2026-04-28. Documentation-only — no production deployment performed.*

---

## Appendix: G2 Evidence Packet Templates

> **Purpose**: These templates provide structured pre-fill forms for the G2 gates in Path 2 (Conditional Production Pilot) of `31-release-paths-todo.md`. All templates are **repo-side tooling validation only** — the actual G2 gates require explicit **operator signoff still required** before any production pilot begins. Do not mark G2 items complete on behalf of the operator.

### Template 1 — Workload Model

```
════════════════════════════════════════════════════════════════
WORKLOAD MODEL  (Template 1 of 5 — G2.1 Pre-fill)
════════════════════════════════════════════════════════════════
Repo-side tooling validation only | Operator signoff still required

Operator name: _______________________________
Target deployment environment: _______________________________
Date pre-filled: _______________________________

Expected sustained write rate: _____ writes/s (max: 300 writes/s for Phase 1 SQLite)
Expected peak write rate: _____ writes/s
Expected daily write volume: _____ writes/day
Expected execution history size at steady state: _____ records

SQLite single-node capacity assessment:
  [ ] Fits within ≤300 writes/s sustained  → Proceed to G2.1 signoff
  [ ] Exceeds 300 writes/s sustained      → Requires Path 3 PostgreSQL evaluation
  [ ] Single-node topology confirmed       → Proceed to G2.1 signoff
  [ ] Multi-node/HA/replica required      → Requires Path 3 PostgreSQL evaluation

Pre-fill completed by (engineer): _______________________________
Pre-fill date: _______________________________
Notes: _______________________________

Operator acknowledgment: _______________________________ (signature) Date: ___________
════════════════════════════════════════════════════════════════
```

### Template 2 — Evaluation Framework Pre-Fill

```
════════════════════════════════════════════════════════════════
EVALUATION FRAMEWORK PRE-FILL  (Template 2 of 5 — G2.6 Pre-fill)
════════════════════════════════════════════════════════════════
Repo-side tooling validation only | Operator signoff still required

Operator name: _______________________________
Date pre-filled: _______________________________

Dimension 1 — Performance:
  [ ] SATISFIED   [ ] CONDITIONAL   [ ] NOT MET   [ ] N/A
  Notes / compensating controls: _______________________________

Dimension 2 — Security:
  [ ] SATISFIED   [ ] CONDITIONAL   [ ] NOT MET   [ ] N/A
  Notes / compensating controls: _______________________________

Dimension 3 — Reliability:
  [ ] SATISFIED   [ ] CONDITIONAL   [ ] NOT MET   [ ] N/A
  Notes / compensating controls: _______________________________

Dimension 4 — Operations:
  [ ] SATISFIED   [ ] CONDITIONAL   [ ] NOT MET   [ ] N/A
  Notes / compensating controls: _______________________________

Dimension 5 — Release Confidence:
  [ ] SATISFIED   [ ] CONDITIONAL   [ ] NOT MET   [ ] N/A
  Notes / compensating controls: _______________________________

Overall: All critical items SATISFIED or CONDITIONAL (with controls)?
  [ ] YES — Proceed to G2.6 signoff
  [ ] NO  — NOT MET item blocks pilot; resolve before proceeding

Pre-fill completed by (engineer): _______________________________
Pre-fill date: _______________________________

Operator final signoff: _______________________________ (signature) Date: ___________
════════════════════════════════════════════════════════════════
```

### Template 3 — Restore Drill Report

```
════════════════════════════════════════════════════════════════
RESTORE DRILL REPORT  (Template 3 of 5 — G2.4 Pre-fill)
════════════════════════════════════════════════════════════════
Repo-side tooling validation only | Operator signoff still required

Operator name: _______________________________
Drill environment: _______________________________
Date of drill: _______________________________
Backup file used: _______________________________
Backup timestamp: _______________________________

Drill steps performed:
  1. [ ] Non-production environment confirmed isolated from live store
  2. [ ] `ferrumctl backup restore --confirm` executed
  3. [ ] Exclusive lock detection triggered correctly (refused if server running)
  4. [ ] Pre-restore copy preserved
  5. [ ] `PRAGMA integrity_check` passed on restored DB
  6. [ ] Execution lineage queryable after restore
  7. [ ] Approval queue readable after restore

Restore drill outcome:
  [ ] SUCCESS — All steps passed; proceed to G2.4 signoff
  [ ] PARTIAL — Issues encountered: _______________________________
  [ ] FAILED  — Drill failed; do not use this backup; fix procedure before G2.4 signoff

RPO confirmation:
  [ ] Operator confirms RPO = time since last backup; writes after last backup are lost on restore
  [ ] RPO acceptable for target workload SLA

RTO confirmation:
  [ ] Operator confirms RTO = restore time + restart + verification; no automated recovery in FerrumGate
  [ ] RTO acceptable for target workload SLA

Pre-fill completed by (engineer): _______________________________
Pre-fill date: _______________________________

Operator signoff: _______________________________ (signature) Date: ___________
════════════════════════════════════════════════════════════════
```

### Template 4 — Compensate Behavior Matrix

```
════════════════════════════════════════════════════════════════
COMPENSATE BEHAVIOR MATRIX  (Template 4 of 5 — G2.8 Pre-fill)
════════════════════════════════════════════════════════════════
Repo-side tooling validation only | Operator signoff still required

Operator name: _______________________________
Date pre-filled: _______________________________
Target adapters in scope: _______________________________

Adapter / Rollback Class | Compensate performs real undo? | Verified by |
------------------------|----------------------------------|-------------|
(adapter name) R0        | [ ] YES  [ ] NO  [ ] UNKNOWN   | ___________ |
(adapter name) R1        | [ ] YES  [ ] NO  [ ] UNKNOWN   | ___________ |
(adapter name) R2        | [ ] YES  [ ] NO  [ ] UNKNOWN   | ___________ |
(adapter name) R3        | [ ] YES  [ ] NO  [ ] UNKNOWN   | ___________ |

Compensate behavior summary:
  [ ] All target adapters verified as performing real undo — proceed to G2.8 signoff
  [ ] Some adapters are noop-backed — compensate noop risk formally accepted (see below)
  [ ] Guaranteed external undo required but not yet verified — adapter implementation required before G2.8 signoff

Compensate noop risk acceptance (only if some adapters are noop-backed):
  Operator acknowledges that `POST /v1/executions/{execution_id}/compensate` may return 200
  without performing external undo for the following adapters:
  List adapters: _______________________________

  Manual verification procedure for noop-backed compensate:
  Step 1: _______________________________
  Step 2: _______________________________
  Step 3: _______________________________

  [ ] Operator accepts compensate noop risk with manual verification procedure above

Pre-fill completed by (engineer): _______________________________
Pre-fill date: _______________________________

Operator signoff: _______________________________ (signature) Date: ___________
════════════════════════════════════════════════════════════════
```

### Template 5 — Accepted-Risk Verification Checklist

```
════════════════════════════════════════════════════════════════
ACCEPTED-RISK VERIFICATION CHECKLIST  (Template 5 of 5 — G2.7 Pre-fill)
════════════════════════════════════════════════════════════════
Repo-side tooling validation only | Operator signoff still required

Operator name: _______________________________
Date pre-filled: _______________________________

Review required documents:
  - `19-v1-single-node-support-contract.md` §4 (Accepted Risks)
  - `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` (Weak Spots 1–4)

Weak Spot 1 — Rollback class handling (RESOLVED):
  [ ] Operator confirms callers set `rollback_class` correctly at intent creation
  [ ] R3 `auto_commit=false` control is correctly applied by gateway prepare
  Notes: _______________________________

Weak Spot 2 — Draft-only revalidation (RESOLVED):
  [ ] Operator confirms prepare handler revalidates draft-only status
  [ ] HTTP 403 returned for `intent.approval_mode == DraftOnly`
  Notes: _______________________________

Weak Spot 3 — Scope-bounds enforcement (RESOLVED):
  [ ] Scope-bounds mismatch control implemented in PDP engine
  [ ] Single-use capability enforcement wired via `mark_capability_used_durable`
  Notes: _______________________________

Weak Spot 4 — Provenance completeness (RESOLVED):
  [ ] Full lineage chain verified: authorize → prepare → execute → verify
  [ ] All 6 event kinds appear in lineage query response linked to execution_id
  Notes: _______________________________

Additional accepted risks from `19-v1-single-node-support-contract.md` §4:
  Risk 1: _______________________________ [ ] Accepted  [ ] Not applicable
  Risk 2: _______________________________ [ ] Accepted  [ ] Not applicable
  Risk 3: _______________________________ [ ] Accepted  [ ] Not applicable
  Risk 4: _______________________________ [ ] Accepted  [ ] Not applicable

Pre-fill completed by (engineer): _______________________________
Pre-fill date: _______________________________

Operator final signoff (all weak spots reviewed and accepted risks acknowledged):
  Signature: _______________________________ Date: ___________
════════════════════════════════════════════════════════════════
```


---

## Appendix: Repo-Side G2 Validation Evidence (Tooling Only)

> **Status**: repo-side tooling validation only. Operator signoff still required.
> These checks validate local tooling and selected controls in the release-preparation
> workspace. They do **not** complete any G2 gate and do **not** authorize a
> production pilot.

| Gate(s) informed | Local check | Result | Evidence |
|---|---|---|---|
| G2.3/G2.4 | `cargo test --package ferrumctl -- backup` | PASS | 8 backup tests passed; restore guardrails, corruption detection, pre-restore copy, and locked DB refusal covered |
| G2.2 | `cargo test --package ferrumd -- test_resolve_config_rejects_bearer_mode_without_token` | PASS | bearer mode without token rejected |
| G2.2/G3 guardrail | `cargo test --package ferrumd -- test_resolve_config_rejects_postgres_dsn_without_feature` | PASS | PostgreSQL DSN remains rejected for v1 single-node scope without `--features postgres` |
| G2.7 | `cargo test --package ferrum-integration-tests -- test_scope_mismatch_deny_on_empty_scope_with_mutation` | PASS | scope-mismatch deny verified |
| G2.7 | `cargo test --package ferrum-integration-tests -- test_r3_contracts_have_auto_commit_false` | PASS | R3 no-auto-commit invariant verified |
| G2.8 | `cargo test --package ferrum-integration-tests -- compensate_execution_flow` | PASS | compensate flow exercised; operator still must accept noop-backed adapter risk |
| G2.7 | `cargo test --test integration_lineage_chain -- test_lineage_chain_full_provenance_events` | PASS | full provenance lineage chain verified |
| G2.4 | Local `/tmp` restore drill with `ferrumctl backup create/verify/restore --confirm` and direct `PRAGMA integrity_check` | PASS | backup created, verified, restored; post-restore `integrity_check=ok`; restored rows matched pre-backup state |

### Local Restore Drill Record

```text
Repo-side tooling validation only | Operator signoff still required
Source DB: /tmp/ferrumgate-g2-restore-drill-8627/source.db
Backup DB: /tmp/ferrumgate-g2-restore-drill-8627/backups/source.db_1777378684.db
Create: Backup created (8192 bytes)
Verify backup: Database integrity check passed / OK
Restore: Pre-restore snapshot saved; Database restored successfully / Restore complete
Verify restored DB: Database integrity check passed / OK
Direct PRAGMA: integrity_check=ok
Restored rows: [(1, 'before-backup')]
```

### Operator Follow-Up Required

The operator must still provide environment-specific evidence before any G2 gate
can be marked complete:

- G2.1 target workload model for the actual production pilot.
- G2.2 production bearer auth, TLS/reverse proxy, and firewall evidence.
- G2.3 production backup schedule evidence.
- G2.4 restore drill in the production-adjacent environment.
- G2.5 RPO/RTO acceptance for the target workload.
- G2.6 operator-completed production evaluation framework.
- G2.7 accepted-risk review/signature.
- G2.8 compensate noop risk acceptance for target adapters.

---

## Addendum: B3/B4/B5 Checklist Closure via Delegated Authority (2026-05-15)

On 2026-05-15, the user authorized the assistant to close evidence-backed B3/B4/B5 checklist items. The closure is recorded in [`artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md`](./artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md) and reflected in `115-sqlite-path2-target-host-checklist.md` and `66-path-2-operator-handoff.md`.

- **B3**: Run id `20260515T1606Z-b3-retention`; old matching sentinel pruned; nonmatching sentinel preserved; new backup `ferrumgate.db_1778861166.db` verified OK rc=0; service active; local readyz 200; SSH firewall restored `118.69.4.63/32`; pre-existing `ferrumgate.db_1778783894.db` within retention window preserved.
- **B4**: Public HTTPS `healthz`/`readyz`/`readyz/deep` returned 200; HTTP→HTTPS redirect returned 308; with-token auth through proxy returned 200 after target-env remediation.
- **B5**: No-token `GET /v1/approvals` returned 401; with-token `GET /v1/approvals` returned 200 after target-env remediation; token not recorded.

**Conditional single-node SQLite pilot readiness**: ACCEPTABLE/YES (scoped only). Production-ready remains NO. PostgreSQL/HA/multi-node remain NO.

Original operator signature by BrianNguyen on 09/05/2026 is preserved unchanged.

---

*Appendix generated: 2026-04-28. All templates are repo-side tooling validation only. Operator signoff still required before production pilot begins.*
