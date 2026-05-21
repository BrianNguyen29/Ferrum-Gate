# Local PostgreSQL Offsite Sync Simulation Evidence — 2026-05-21

> **Status**: LOCAL EVIDENCE — non-production simulation only.
> **Purpose**: Simulate offsite sync by copying a backup to a separate local directory and verifying hash integrity.
> **Scope**: Local filesystem copy simulation. NOT a real GCS/S3/rsync offsite target.
> **Constraint**: `production-ready = NO`. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local simulation only |
| **Real offsite sync** | **NO** | Copied to local directory only; no GCS, S3, rsync, or SFTP used |
| **Full G2** | **NOT COMPLETE** | Conditional pilot only |
| **Block A** | **WAIVED/CONDITIONAL** | No real domain |

---

## Metadata

| Field | Value |
|-------|-------|
| **Timestamp** | 2026-05-21 |
| **Environment** | Local filesystem |
| **Source directory** | `/tmp/opencode/ferrumgate-pg-evidence/backups/` |
| **Simulated offsite directory** | `/tmp/opencode/ferrumgate-pg-evidence/offsite/` |
| **Sync method** | `cp` (local filesystem copy simulating offsite sync) |
| **Evidence owner** | Engineering |

---

## T-OFF-1 — Simulated Offsite Target Setup

**Command**:
```bash
mkdir -p /tmp/opencode/ferrumgate-pg-evidence/offsite/
```
**Result**: Directory created successfully.

**Pass/Fail**: ✅ PASS

---

## T-OFF-2 — Sync Execution

**Command**:
```bash
cp /tmp/opencode/ferrumgate-pg-evidence/backups/ferrumgate_local_20260521.dump /tmp/opencode/ferrumgate-pg-evidence/offsite/
```
**Result**: File copied successfully.

**Transferred file**: `ferrumgate_local_20260521.dump`
**Transfer size**: `919 bytes`
**Transfer duration**: `< 1 second` (local filesystem)

**Pass/Fail**: ✅ PASS

---

## T-OFF-3 — File Integrity Verification

**Local file hash**:
```bash
sha256sum /tmp/opencode/ferrumgate-pg-evidence/backups/ferrumgate_local_20260521.dump
```
**Result**: `23a59e64e5c3337d79e179679ab563dfb0bb1dec4630873a09919831694e32f3`

**Offsite file hash**:
```bash
sha256sum /tmp/opencode/ferrumgate-pg-evidence/offsite/ferrumgate_local_20260521.dump
```
**Result**: `23a59e64e5c3337d79e179679ab563dfb0bb1dec4630873a09919831694e32f3`

**Hash match**: ✅ YES

**Pass/Fail**: ✅ PASS

---

## T-OFF-4 — Offsite Restore Drill (Simulated)

**Check**: Verify the offsite copy can be listed by `pg_restore`.

**Command**:
```bash
pg_restore -l /tmp/opencode/ferrumgate-pg-evidence/offsite/ferrumgate_local_20260521.dump > /dev/null && echo "OK" || echo "FAIL"
```
**Result**: `OK`

**Note**: A full restore drill from offsite to a clean database was not performed in this simulation. The restore drill from the local backup was already validated in `2026-05-21-pg-local-scheduled-backup-evidence.md` §T-BAK-4.

**Pass/Fail**: ✅ PASS (integrity verification only)

---

## Limitations and Non-Production Caveats

| Limitation | Why it matters |
|------------|---------------|
| **Local `cp` only** | No `gsutil rsync`, `aws s3 sync`, `rsync -avz`, or SFTP was used. Network transfer behavior is not tested. |
| **No credential testing** | No service account keys, IAM roles, or SSH keys were involved. |
| **No bandwidth or latency testing** | Local filesystem copy completes instantly. Real offsite sync may take minutes or hours. |
| **No retry or resume logic** | `cp` does not retry on failure. Real offsite tools may handle transient errors differently. |
| **Single file only** | Production environments may sync hundreds of files. |
| **No encryption-at-rest verification** | Local filesystem may not encrypt at rest. Operator must verify offsite storage encryption independently. |
| **No periodic sync schedule tested** | This was a one-time copy. Hourly or periodic sync was not exercised. |

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | 2026-05-21 | Local simulation |
| Operator | | | *(blank — operator signoff requires production execution)* |

---

## Related Docs

- [`docs/implementation-path/artifacts/TEMPLATE-pg-offsite-sync-evidence.md`](./TEMPLATE-pg-offsite-sync-evidence.md) — Full template for operator production execution
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-3
- [`docs/implementation-path/artifacts/2026-05-21-pg-local-scheduled-backup-evidence.md`](./2026-05-21-pg-local-scheduled-backup-evidence.md)
