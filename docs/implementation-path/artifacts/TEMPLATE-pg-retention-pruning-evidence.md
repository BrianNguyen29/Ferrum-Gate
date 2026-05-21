# TEMPLATE — PostgreSQL Backup Retention Pruning Validation Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) Phase PG-3 and [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) §P5c.5 for the runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / production / local-docker-compose` |
| **Operator** | `name` |
| **Retention policy** | `4 days (384 dumps at 15-minute interval)` |
| **Pruning method** | `find -mmin / cron / systemd ExecStartPost / external script` |
| **Evidence owner** | Operator |

---

## T-RET-1 — Retention Policy Definition

**Check**: Retention policy is documented and approved.

- **Backup interval**: `15 minutes`
- **Retention target**: `4 days`
- **Expected max dump count**: `384` (4 days × 24 hours × 4 dumps/hour)
- **Pruning command**:
  ```bash
  (paste exact pruning command)
  ```
- **Policy approved by**: `operator name`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-RET-2 — Pre-Pruning Inventory

**Check**: Record backup directory state before pruning.

- **Directory**: `/var/backups/ferrumgate-postgres/`
- **Total dump files**: `N`
- **Oldest dump file**: `ferrumgate_YYYYMMDD_HHMMSS.dump`
- **Oldest dump age**: `N hours / N days`
- **Disk usage**: `N MiB`
- **Inventory command**:
  ```bash
  ls -la /var/backups/ferrumgate-postgres/ | wc -l
  du -sh /var/backups/ferrumgate-postgres/
  ```

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-RET-3 — Pruning Execution

**Check**: Pruning command executes without error.

- **Pruning trigger**: `scheduled / manual`
- **Execution time**: `YYYY-MM-DD HH:MM:SS UTC`
- **Command executed**:
  ```bash
  (paste exact command with output)
  ```
- **Exit code**: `0 / non-zero`
- **Files removed count**: `N`
- **Files preserved count**: `N`
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-RET-4 — Post-Pruning Verification

**Check**: Only files within retention window remain.

- **Post-pruning total dump files**: `N`
- **Post-pruning oldest dump file**: `ferrumgate_YYYYMMDD_HHMMSS.dump`
- **Post-pruning oldest dump age**: `N hours / N days`
- **Post-pruning disk usage**: `N MiB`
- **Retention window satisfied**: `yes / no` (oldest dump must be ≤ 4 days old)
- **Expected dumps preserved**: `≤ 384`
- **Actual dumps preserved**: `N`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-RET-5 — Edge Case — No Accidental Deletion

**Check**: Pruning does not remove non-dump files or directories.

- **Non-dump files present before pruning**: `yes / no` (e.g., `.pgpass`, README, log files)
- **Non-dump files after pruning**: `all present / some missing / all missing`
- **Accidental deletions observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: Retention pruning validation is one component of PostgreSQL hardening. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT backup verification**: This evidence validates retention pruning only. Backup creation and integrity are covered by `TEMPLATE-pg-scheduled-backup-evidence.md`.
- **NOT restore verification**: Restore drill is covered by `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md` or a new live restore drill artifact.
- **NOT Block A closed**: Block A (real owned domain + DNS) remains WAIVED/CONDITIONAL.
- **NOT full G2**: G2 operator signoff requires real domain and final evidence pack review.

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Operator | | | |
| Engineering (witness) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-3
- [`docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md`](../../implementation-path/109-p5c-postgresql-backup-restore-runbook.md) §P5c.5
- [`docs/implementation-path/artifacts/TEMPLATE-pg-scheduled-backup-evidence.md`](./TEMPLATE-pg-scheduled-backup-evidence.md)
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
