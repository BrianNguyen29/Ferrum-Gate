# Phase B — PostgreSQL Production Foundation Prep — 2026-05-21

> **Status**: Engineering planning/runbook artifact. No production PostgreSQL deployment claimed.
> **Purpose**: Consolidate Phase B documentation progress for TLS/SSL DSN guidance, PgBouncer/pooling story, scheduled backup/retention runbook, alert deployment validation runbook, and evidence templates.
> **Scope**: Single-node SQLite v1 conditional pilot. PostgreSQL production foundation planning only.
> **Constraint**: `production-ready = NO` throughout. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.

> **Delegated signoff (planning-only — template readiness)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: Phase B runbooks and evidence templates reviewed and accepted as ready for operator execution.
> - **Nature**: Planning/template-readiness signoff only. This does not constitute evidence of live execution, deployment, or production readiness. Does not substitute for missing evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Block A remains conditional; no real owned domain; single-node SQLite pilot only |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Conditional re-signoff documented elsewhere; full closure requires real domain + revalidation |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted for single-node SQLite pilot only |
| **PostgreSQL production deployment** | **NO** | Runbooks complete; live deployment evidence pending operator environment |
| **HA / multi-node** | **NO** | Planning artifacts only; no implementation |
| **Live TLS-encrypted PG connection** | **NO** | Guidance documented; not validated on live target |
| **Live PgBouncer deployment** | **NO** | Guidance documented; not validated on live target |
| **Live scheduled backup automation** | **NO** | Runbook documented; not executed on live target |
| **Live Prometheus alert deployment** | **NO** | Runbook documented; `promtool` syntax check passed locally; live evaluation pending |

---

## 1. Phase B Deliverables

| ID | Deliverable | Location | Status |
|----|-------------|----------|--------|
| B.1 | TLS/SSL DSN guidance | `docs/production-readiness-v2/02-postgres-production-plan.md` §PG-2.5 + `docs/guides/operator.md` §PostgreSQL TLS/SSL DSN configuration | ✅ RUNBOOK COMPLETE |
| B.2 | PgBouncer / connection pooling story | `docs/production-readiness-v2/02-postgres-production-plan.md` §PG-2.6 + `docs/guides/operator.md` §PgBouncer / connection pooling | ✅ RUNBOOK COMPLETE |
| B.3 | Scheduled backup/retention/offsite runbook | `docs/production-readiness-v2/02-postgres-production-plan.md` §PG-3 + `docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md` §P5c.5 | ✅ RUNBOOK COMPLETE |
| B.4 | Alert deployment validation runbook | `configs/monitoring/README.md` §Alert Deployment Validation Runbook + `docs/guides/operator.md` §Alert deployment validation | ✅ RUNBOOK COMPLETE |
| B.5 | Evidence checklist updated | `docs/production-readiness-v2/10-evidence-checklist.md` §Phase 1 | ✅ COMPLETE |
| B.6 | PostgreSQL production plan gaps updated | `docs/production-readiness-v2/02-postgres-production-plan.md` §Gaps | ✅ COMPLETE |
| B.7 | Evidence templates created | `docs/implementation-path/artifacts/TEMPLATE-pg-*.md` (6 templates) | ✅ COMPLETE — see §10 below |

---

## 2. TLS/SSL DSN Guidance (B.1)

**What was done**:
- Documented four TLS modes (`disable`, `require`, `verify-ca`, `verify-full`) with a decision table.
- Provided five DSN examples covering: basic TLS, CA verification, full verification, and client certificate authentication.
- Documented file permissions (`chmod 600` for client keys).
- Documented certificate rotation procedure (maintenance window restart).
- Added to operator guide for discoverability.

**What remains pending**:
- Operator must procure CA/client certificates.
- Operator must configure PostgreSQL server for TLS.
- Operator must validate `FERRUMD_STORE_DSN` with TLS parameters against live PG.
- Evidence artifact: `pg-tls-dsn-evidence.md` (pending operator).

---

## 3. PgBouncer / Connection Pooling Story (B.2)

**What was done**:
- Documented when PgBouncer adds value vs. direct `sqlx::PgPool` (decision table).
- Recommended `transaction` pool mode as default for FerrumGate.
- Provided connection count math (ferrumd pool max × instances → PgBouncer pool size).
- Provided example `pgbouncer.ini` with key settings explained.
- Documented ferrumd DSN format pointing at PgBouncer.
- Documented caveats: two-tier pooling, `SET` command behavior in `transaction` mode, single point of failure.
- Documented enable triggers (instance count, connection limit, churn).

**What remains pending**:
- Operator must deploy PgBouncer in their environment.
- Operator must test `statement_timeout` behavior through PgBouncer.
- Operator must validate ferrumd connectivity and performance under load.
- Evidence artifact: `pg-pgbouncer-evidence.md` (pending operator).

---

## 4. Scheduled Backup / Retention / Offsite Runbook (B.3)

**What was done**:
- Cron example (15-minute interval) with inline retention pruning.
- Systemd timer + service example with post-backup verification.
- Offsite target comparison table (GCS/S3, rsync/SFTP, managed PG backup).
- Recommended default: local `pg_dump` every 15 min → 4-day retention → hourly offsite sync → monthly restore drill.
- Evidence artifact format defined.

**What remains pending**:
- Operator must configure and enable scheduler (cron or systemd timer) on live host.
- Operator must verify backup files are created at the configured interval.
- Operator must verify retention pruning removes old files as expected.
- Operator must configure and verify offsite sync.
- Operator must perform monthly restore drill and verify row counts + `/v1/readyz/deep`.
- Evidence artifacts: `pg-scheduled-backup-evidence.md`, `pg-retention-pruning-evidence.md`, `pg-offsite-sync-evidence.md` (all pending operator).

---

## 5. Alert Deployment Validation Runbook (B.4)

**What was done**:
- 6-step validation procedure:
  1. `promtool check rules` syntax validation.
  2. Deploy to Prometheus rules directory and reload.
  3. Verify rule evaluation state via `/api/v1/rules`.
  4. Validate PG-specific alert behavior (if PG backend active).
  5. Optional simulation in non-production.
  6. Evidence artifact template.
- Added to `configs/monitoring/README.md` and `docs/guides/operator.md`.
- `promtool check rules` already passed locally on 2026-05-21 (`SUCCESS: 21 rules found`).

**What remains pending**:
- Operator must deploy `ferrumgate-alerts.yaml` to their live Prometheus.
- Operator must verify rules load without error.
- Operator must verify PG-specific alerts behave correctly with their ferrumd backend.
- Operator must validate AlertManager routing (if applicable).
- Evidence artifact: `pg-alert-deployment-evidence.md` (pending operator).

---

## 6. Gap Closure Summary

| Gap | Previous Status | Phase B Status |
|-----|-----------------|----------------|
| No TLS/SSL DSN guidance | Open | ✅ CLOSED — runbook complete; live validation pending |
| No PgBouncer/connection pooling story | Open | ✅ CLOSED — runbook complete; live validation pending |
| PG-3 scheduled backup/retention | NOT STARTED | ✅ RUNBOOK COMPLETE — execution pending operator |
| PG alert templates live deployment | Template prepared | ✅ RUNBOOK COMPLETE — live evaluation pending operator |

---

## 7. Remaining Open Gaps (Post-Phase B)

| Gap | Severity | Why |
|-----|----------|-----|
| No target-host PG drills | High | No evidence of production PG behavior on operator infrastructure |
| No PG restore drill evidence on live DB | High | Local Docker drill complete; live drill pending |
| No CI for postgres feature | Medium | Drift risk |
| No HA/failover | Critical | No production HA; deferred to Phase 9 |
| No replication configs | High | No standby/read replica; deferred to Phase 9 |
| No split-brain prevention | High | HA claim impossible without this; deferred to Phase 9 |

---

## 8. Cross-References

| Document | Purpose |
|----------|---------|
| `docs/production-readiness-v2/02-postgres-production-plan.md` | Authoritative PG production plan with all Phase B sections |
| `docs/guides/operator.md` | Operator-facing quick reference for TLS, PgBouncer, and alert validation |
| `configs/monitoring/README.md` | Alert template documentation and deployment validation runbook |
| `docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md` | Detailed backup/restore procedures and scheduler examples |
| `docs/production-readiness-v2/10-evidence-checklist.md` | Phase 1 evidence checklist with Phase B rows |
| `docs/production-readiness-v2/11-blockers-and-unblock-plan.md` | Blocker tracking |

---

## 9. Evidence Templates

The following templates are ready for operator execution. Each includes environment, commands/checks, expected outputs, pass/fail criteria, sanitized evidence blocks, operator signoff, and non-claims.

| Template | Purpose | Location |
|----------|---------|----------|
| `TEMPLATE-pg-tls-dsn-evidence.md` | TLS DSN validation: cert permissions, connectivity, startup, readiness, rotation drill | `docs/implementation-path/artifacts/` |
| `TEMPLATE-pg-pgbouncer-evidence.md` | PgBouncer validation: config review, health, connectivity, startup, readiness, session compatibility, load observation | `docs/implementation-path/artifacts/` |
| `TEMPLATE-pg-scheduled-backup-evidence.md` | Scheduled backup validation: scheduler config, execution observation, integrity, timing, RPO compliance | `docs/implementation-path/artifacts/` |
| `TEMPLATE-pg-retention-pruning-evidence.md` | Retention pruning validation: policy definition, pre/post inventory, pruning execution, edge cases | `docs/implementation-path/artifacts/` |
| `TEMPLATE-pg-offsite-sync-evidence.md` | Offsite sync validation: target config, sync execution, integrity, restore drill, RPO impact | `docs/implementation-path/artifacts/` |
| `TEMPLATE-pg-alert-deployment-evidence.md` | Alert deployment validation: promtool syntax, deployment, rule presence, state check, PG behavior, simulation | `docs/implementation-path/artifacts/` |

**Usage instruction**: Operator copies the relevant template, fills all sections with real execution output, and renames to `YYYY-MM-DD-<template-base>.md`. Do not sign until all pass/fail checkboxes are marked.

---

## 10. Engineering Review Statement

> This artifact accurately records Phase B documentation progress as of 2026-05-21. Four runbooks and six evidence templates were completed: TLS/SSL DSN guidance, PgBouncer/pooling story, scheduled backup/retention/offsite path, and alert deployment validation. All are planning/runbook/template artifacts only. No live deployment, no production PostgreSQL, no production-ready claim. Operator action is required to execute runbooks, fill templates, and produce evidence artifacts. Production-ready remains **NO**. Block A remains **WAIVED/CONDITIONAL**. Full G2 remains **NOT COMPLETE**.

---

*Artifact created: 2026-05-21. Phase B PostgreSQL Production Foundation Prep — runbook artifact only.*
