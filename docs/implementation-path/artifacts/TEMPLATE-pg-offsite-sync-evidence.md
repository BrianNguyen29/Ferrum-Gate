# TEMPLATE — PostgreSQL Backup Offsite Sync Validation Evidence

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
| **Offsite target type** | `GCS / S3 / rsync / SFTP / other` |
| **Sync method** | `gsutil rsync / aws s3 sync / rsync -avz / scp / other` |
| **Sync interval** | `hourly / every 15 min / daily` |
| **Evidence owner** | Operator |

---

## T-OFF-1 — Offsite Target Configuration

**Check**: Offsite target is accessible and correctly configured.

- **Target URI / path** (sanitized — no credentials):
  ```text
  gs://my-backup-bucket/ferrumgate-postgres/
  # or
  s3://my-backup-bucket/ferrumgate-postgres/
  # or
  backup-host:/backups/ferrumgate-postgres/
  ```
- **Credential method**: `service account / IAM role / SSH key / other`
- **Credential storage**: `secret manager / env file / systemd unit / other`
- **Credential permissions**: `write-only / read-write` (recommended: write-only for sync user)
- **Connectivity test command**:
  ```bash
  (paste command and output)
  ```
- **Connectivity test result**: `success / failure`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-OFF-2 — Sync Execution Observation

**Check**: Sync job transfers the latest backup to offsite storage.

- **Observation start**: `YYYY-MM-DD HH:MM:SS UTC`
- **Observation end**: `YYYY-MM-DD HH:MM:SS UTC`
- **Number of sync intervals observed**: `N`
- **Number of successful syncs**: `N`
- **Number of failed syncs**: `N`
- **Sync command**:
  ```bash
  (paste exact command)
  ```
- **Sync log location**: `/var/log/ferrumgate-offsite-sync.log`
- **Log excerpt (last successful sync)**:
  ```
  (paste log here — redact credentials if present)
  ```
- **Last synced file**: `ferrumgate_YYYYMMDD_HHMMSS.dump`
- **Transfer duration**: `N seconds`
- **Transfer size**: `N MiB`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-OFF-3 — Offsite File Integrity Verification

**Check**: Files on offsite storage match local files.

- **Local file hash** (latest dump):
  ```bash
  sha256sum /var/backups/ferrumgate-postgres/ferrumgate_YYYYMMDD_HHMMSS.dump
  ```
- **Local hash**: `sha256:...`
- **Offsite file hash**:
  ```bash
  (command depends on target — e.g., gsutil hash, aws s3api head-object, ssh backup-host sha256sum)
  ```
- **Offsite hash**: `sha256:...`
- **Hash match**: `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-OFF-4 — Offsite Restore Drill (Recommended)

**Check**: Operator can restore from offsite storage to a clean local directory or drill database.

- **Drill date**: `YYYY-MM-DD HH:MM:SS UTC`
- **Offsite file restored**: `ferrumgate_YYYYMMDD_HHMMSS.dump`
- **Download command**:
  ```bash
  (paste command)
  ```
- **Download success**: `yes / no`
- **Local integrity check after download**:
  ```bash
  pg_restore -l /path/to/downloaded.dump > /dev/null && echo "OK" || echo "FAIL"
  ```
- **Integrity result**: `OK / FAIL`
- **Optional full restore drill**: `performed / not performed` (if performed, link to restore drill evidence)

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT PERFORMED

---

## T-OFF-5 — Sync Timing and RPO Impact

**Check**: Sync frequency does not extend effective RPO beyond the 15-minute target.

- **Backup creation interval**: `15 minutes`
- **Offsite sync interval**: `hourly` (or as configured)
- **Maximum data not yet synced**: `≤ 1 hour` (or configured sync interval)
- **Effective offsite RPO**: `≤ 1 hour` (or configured sync interval)
- **Operator acknowledges offsite RPO is looser than local RPO**: `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: Offsite sync validation is one component of PostgreSQL hardening. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT disaster recovery tested**: This evidence validates file transfer and download integrity. Full disaster recovery testing (complete rebuild from offsite backup in a new region) is out of scope.
- **NOT encryption-at-rest verification**: This evidence does not verify whether the offsite storage provider encrypts data at rest. Operator must confirm this independently.
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
- [`docs/implementation-path/artifacts/TEMPLATE-pg-retention-pruning-evidence.md`](./TEMPLATE-pg-retention-pruning-evidence.md)
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
