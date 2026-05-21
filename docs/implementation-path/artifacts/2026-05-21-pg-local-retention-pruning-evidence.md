# Local PostgreSQL Retention Pruning Simulation Evidence — 2026-05-21

> **Status**: LOCAL EVIDENCE — non-production simulation only.
> **Purpose**: Simulate retention pruning behavior using `find -mtime` against local backup files.
> **Scope**: Local filesystem simulation. NOT a production backup target.
> **Constraint**: `production-ready = NO`. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Local simulation only |
| **Production retention pruning** | **NO** | Simulated with `touch -d` backdating; not a live production scheduler |
| **Full G2** | **NOT COMPLETE** | Conditional pilot only |
| **Block A** | **WAIVED/CONDITIONAL** | No real domain |

---

## Metadata

| Field | Value |
|-------|-------|
| **Timestamp** | 2026-05-21 |
| **Environment** | Local filesystem (`/tmp/opencode/ferrumgate-pg-evidence/backups/`) |
| **Retention policy tested** | `4 days` |
| **Pruning command** | `find /tmp/opencode/ferrumgate-pg-evidence/backups/ -name "ferrumgate_*.dump" -mtime +4 -delete` |
| **Evidence owner** | Engineering |

---

## T-RET-1 — Pre-Pruning Inventory

**Directory**: `/tmp/opencode/ferrumgate-pg-evidence/backups/`

**Files created for simulation**:
1. `ferrumgate_local_20260521.dump` — current dump (created 2026-05-21)
2. `ferrumgate_old_20260501.dump` — simulated old dump (backdated to 2026-05-01)

**Pre-pruning listing**:
```bash
ls -la /tmp/opencode/ferrumgate-pg-evidence/backups/
```
**Result**:
```
ferrumgate_local_20260521.dump  919 bytes
ferrumgate_old_20260501.dump    919 bytes
```

**Pass/Fail**: ✅ PASS

---

## T-RET-2 — Pruning Execution

**Command**:
```bash
find /tmp/opencode/ferrumgate-pg-evidence/backups/ -name "ferrumgate_*.dump" -mtime +4 -delete
```
**Exit code**: `0`
**Output**: *(no output — silent success)*

**Pass/Fail**: ✅ PASS

---

## T-RET-3 — Post-Pruning Verification

**Post-pruning listing**:
```bash
ls -la /tmp/opencode/ferrumgate-pg-evidence/backups/
```
**Result**:
```
ferrumgate_local_20260521.dump  919 bytes
```

**Observations**:
- `ferrumgate_old_20260501.dump` was correctly removed (age > 4 days).
- `ferrumgate_local_20260521.dump` was correctly preserved (age ≤ 4 days).
- Only `.dump` files matching the pattern were affected.

**Pass/Fail**: ✅ PASS

---

## T-RET-4 — Edge Case — No Accidental Deletion

**Check**: Non-dump files in the directory were not affected.

**Result**: No non-dump files were present in the test directory. In a production scenario, the operator should verify that the `find` pattern does not match logs, READMEs, or other metadata files.

**Recommendation**: Use a dedicated backup directory that contains only dump files, or refine the `find` pattern to be more specific.

**Pass/Fail**: ✅ PASS (within simulation limits)

---

## Limitations and Non-Production Caveats

| Limitation | Why it matters |
|------------|---------------|
| **Simulated backdating** | The old dump was created with `touch -d` backdating, not a real 4-day-old backup. |
| **No scheduler integration** | Pruning was executed manually, not by a cron job or systemd timer post-backup hook. |
| **Small file count** | Only 2 files were present. Production environments may have hundreds of dumps. |
| **Local filesystem only** | No network filesystem, no cloud storage, no permission edge cases. |
| **No concurrent access** | No other process was reading/writing the backup directory during pruning. |

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | 2026-05-21 | Local simulation |
| Operator | | | *(blank — operator signoff requires production execution)* |

---

## Related Docs

- [`docs/implementation-path/artifacts/TEMPLATE-pg-retention-pruning-evidence.md`](./TEMPLATE-pg-retention-pruning-evidence.md) — Full template for operator production execution
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-3
- [`docs/implementation-path/artifacts/2026-05-21-pg-local-scheduled-backup-evidence.md`](./2026-05-21-pg-local-scheduled-backup-evidence.md)
