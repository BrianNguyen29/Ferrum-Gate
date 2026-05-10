# Phase 4A Live Hardening Evidence

**Date**: 2026-05-10

**Scope**: Live execution evidence for Phase 4A backup cadence audit, offsite restore drill, and metrics baseline capture. Documents helper script bugs discovered and fixed during live execution.

**Status**: **NON-PROD scaffold evidence only**. Helper scripts are read-only audit/drill tools. SendGrid rotate remains pending operator-provided key. NOT production-ready, NOT production alerting, NOT PostgreSQL, NOT HA.

---

## Non-Claims

This Phase 4A live artifact does **not** claim:

- production-ready status
- full production posture
- production alerting capability
- PostgreSQL runtime (SQLite single-node)
- HA/multi-node deployment
- Phase 4A operator signoff

---

## Overview

Phase 4A live execution completed three helper script runs:

1. **Backup Cadence Audit** — 3 iterations; URL parsing bug found and fixed; age bug found and fixed; final pass PASSED
2. **Offsite Restore Drill** — multi-line COPY_SUCCESS bug found and fixed; final pass PASSED
3. **Metrics Baseline Capture** — PASSED

SendGrid rotate remains **BLOCKED** — operator must provide rotated key created in SendGrid dashboard.

---

## Helper Script Fixes Discovered During Live Execution

### Fix 1: URL Parsing Bug (phase4a_audit_backup_cadence.sh)

**Bug**: `gsutil ls -l` output format is `SIZE DATE gs://URL` (URL not at line start). Original script assumed URL was at a fixed field position.

**Initial incorrect parsing**:
```bash
# Incorrect: assumed URL was field 3 at line start
GCS_TIMESTAMP=$(echo "$OBJECT_LINE" | awk '{print $2}')
MOST_RECENT=$(echo "$OBJECT_LINE" | awk '{print $3}')
```

**Fix**: Use `grep` to find the line containing `gs://` first, then extract fields from that line:
```bash
# Correct: extract URL and timestamp from the line containing gs://
OBJECT_LINE=$(echo "$GCS_OUTPUT" | grep -m1 -E 'gs://[^[:space:]]+' || echo "")
GCS_TIMESTAMP=$(echo "$OBJECT_LINE" | awk '{print $2}' | sed 's/Z$//')
MOST_RECENT=$(echo "$OBJECT_LINE" | awk '{print $3}')
```

**Affected script**: `scripts/gcp/phase4a_audit_backup_cadence.sh` (lines 172–178)

---

### Fix 2: Age Computation Bug (phase4a_audit_backup_cadence.sh)

**Bug**: RPO age initially used filename timestamp (from backup object name) instead of GCS object mtime.

**Initial incorrect approach**: Extracted timestamp from filename pattern `ferrumgate_backup_YYYYMMDD_HHMMSS.db`

**Fix**: Use GCS mtime from `gsutil ls -l` field 2 (the ISO timestamp):
```bash
GCS_TIMESTAMP=$(echo "$OBJECT_LINE" | awk '{print $2}' | sed 's/Z$//')
```

**Affected script**: `scripts/gcp/phase4a_audit_backup_cadence.sh` (lines 176–177)

---

### Fix 3: Multi-line COPY_SUCCESS Bug (phase4a_offsite_restore_drill.sh)

**Bug**: `gsutil cp` outputs multi-line logs. Original check `echo 'COPY_SUCCESS' || echo 'COPY_FAILED'` relied on exit code only, but did not account for COPY_SUCCESS appearing anywhere in the multi-line output vs. final line position.

**Initial incorrect check**:
```bash
# Incorrect: relied on echo appending COPY_SUCCESS/COPY_FAILED as final line
COPY_RESULT=$(gcloud compute ssh ... "gsutil cp ... 2>&1 && echo 'COPY_SUCCESS' || echo 'COPY_FAILED'")
# Check assumed COPY_SUCCESS was at end of output
```

**Fix**: Check for COPY_SUCCESS anywhere in output using `grep`:
```bash
# Correct: check for COPY_SUCCESS anywhere in multi-line output
COPY_RESULT=$(gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
    --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
    --quiet -- \
    "gsutil cp '${MOST_RECENT}' '${TEMP_FILE}' 2>&1 && echo 'COPY_SUCCESS' || echo 'COPY_FAILED'" \
    2>/dev/null || echo "COPY_FAILED")

if ! echo "$COPY_RESULT" | grep -q 'COPY_SUCCESS'; then
    echo "ERROR: Failed to copy backup from GCS." >&2
    # Cleanup temp file on copy failure
    gcloud compute ssh ubuntu@"$GCP_VM_NAME" \
        --zone="$GCP_ZONE" --project="$GCP_PROJECT_ID" \
        --quiet -- \
        "rm -f '${TEMP_FILE}'" 2>/dev/null || true
    exit 1
fi
```

**Additional fix**: Added cleanup on copy failure (lines 194–199).

**Affected script**: `scripts/gcp/phase4a_offsite_restore_drill.sh` (lines 183–200)

---

## Live Execution Evidence

### 1. Backup Cadence Audit — Final Pass (PASSED)

**Command**:
```bash
bash scripts/gcp/phase4a_audit_backup_cadence.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
```

**Result**:
| Field | Value |
|-------|-------|
| Timer found | YES |
| GCS objects | 1 |
| GCS mtime | 2026-05-10T03:07:21Z |
| Age | 9 seconds |
| RPO threshold | 3600s |
| RPO compliance | ✅ WITHIN RPO |
| Script | read-only (no state modified) |

**Syntax check**:
```
bash -n scripts/gcp/phase4a_audit_backup_cadence.sh
→ PASSED (no syntax errors)
```

---

### 2. Offsite Restore Drill — Final Pass (PASSED)

**Command**:
```bash
bash scripts/gcp/phase4a_offsite_restore_drill.sh --confirm \
  --project-id fairy-b13f4 \
  --zone asia-southeast1-a \
  --vm-name ferrumgate-nonprod \
  --gcs-bucket gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
```

**Result**:
| Field | Value |
|-------|-------|
| GCS object | Restored to temp file only |
| INTEGRITY | ok |
| TABLE_COUNT | 14 |
| SIZE_BYTES | 241664 |
| Cleanup | CLEANUP_SUCCESS |
| Production DB | NOT modified |

**Syntax check**:
```
bash -n scripts/gcp/phase4a_offsite_restore_drill.sh
→ PASSED (no syntax errors)
```

---

### 3. Metrics Baseline Capture — Final Pass (PASSED)

**Captured baseline**: `/tmp/ferrumgate_metrics_baseline_20260510_031004.txt`

| Field | Value |
|-------|-------|
| TLS domain | ferrumgate.duckdns.org |
| readiness deep status | ok, healthy=true |
| store | ok |
| write_queue | depth=0, threshold=100, healthy=true |
| metrics lines | 100 |
| ferrumgate_store_health_up | 1 |
| ferrumgate_write_queue_depth | 0 |

**Syntax check**:
```
bash -n scripts/gcp/phase4a_capture_metrics_baseline.sh
→ PASSED (no syntax errors)
```

---

## SendGrid Rotate — PENDING

**Status**: BLOCKED

**Blocker**: Operator must provide rotated key created in SendGrid dashboard. Current bridge evidence already committed in `artifacts/2026-05-10-phase4a-sendgrid-bridge-evidence.md`.

**Next action**: Operator creates new SendGrid API key in SendGrid dashboard → stores at `/etc/ferrumgate/secrets/sendgrid-api-key` → reloads AlertManager.

---

## Target Environment (Phase 4A Live)

| Field | Value | Notes |
|-------|-------|-------|
| Project | `fairy-b13f4` | GCP project |
| Region | `asia-southeast1` | GCP region |
| Zone | `asia-southeast1-a` | GCP zone |
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Static IP | `34.158.51.8` | External IP |
| TLS Domain | `ferrumgate.duckdns.org` | DuckDNS — TLS SUCCESS (Phase 3J) |
| HTTPS URL | `https://ferrumgate.duckdns.org` | Primary endpoint |
| Database | SQLite single-node | Not PostgreSQL |
| Monitoring | Local-only (Prometheus + AlertManager on VM) | Phase 3H deployed |
| GCS Bucket | `gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/` | Phase 3H configured |
| SendGrid Bridge | **Not rotated** | Pending operator action |

---

## What Phase 4A is NOT

Phase 4A does NOT:

- Claim production-ready or full production posture
- Claim production alerting YES
- Include PostgreSQL or HA
- Modify Rust code
- Make live GCP mutations (scripts are read-only audit/drill helpers)

---

## Remaining Blockers

| Item | Status | Blocker |
|------|--------|---------|
| SendGrid API key rotate | **BLOCKED** | Operator must create rotated key in SendGrid dashboard |
| Production alerting | **BLOCKED** | No alert contact; local-only mode; SendGrid bridge template only |
| Real owned domain TLS | **BLOCKED** | DuckDNS is free DNS; real owned domain required for production |
| Production-ready claim | **BLOCKED** | Nonprod only; single-node SQLite |

---

## References

- Phase 4A plan: [102-phase4a-ops-hardening-alert-bridge-plan.md](../102-phase4a-ops-hardening-alert-bridge-plan.md)
- Phase 4A artifact (scaffold): [artifacts/2026-05-09-phase4a-ops-hardening-alert-bridge-plan.md](./2026-05-09-phase4a-ops-hardening-alert-bridge-plan.md)
- Phase 4A SendGrid evidence: [artifacts/2026-05-10-phase4a-sendgrid-bridge-evidence.md](./2026-05-10-phase4a-sendgrid-bridge-evidence.md)
- Phase 3H offsite monitoring: [artifacts/2026-05-09-gcp-phase3h-offsite-monitoring.md](./2026-05-09-gcp-phase3h-offsite-monitoring.md)
- Phase 3J DuckDNS TLS: [artifacts/2026-05-09-gcp-phase3j-duckdns-tls-attempt.md](./2026-05-09-gcp-phase3j-duckdns-tls-attempt.md)
- Production readiness roadmap: [67-production-readiness-roadmap.md](../67-production-readiness-roadmap.md)

---

**Non-claims**: NOT production-ready, NOT full production posture, NOT production alerting, NOT PostgreSQL, NOT HA. Helper scripts are read-only audit/drill tools. DuckDNS TLS SUCCESS. SendGrid rotate BLOCKED pending operator action.