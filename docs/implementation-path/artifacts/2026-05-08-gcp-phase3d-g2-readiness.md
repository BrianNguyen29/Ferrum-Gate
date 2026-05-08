# GCP Phase 3D G2 Readiness Evidence Artifact

**Date**: 2026-05-08

**Scope**: GCP non-prod FerrumGate VM — Phase 3D G2 readiness evidence collection

**Status**: **NON-PROD rehearsal evidence only**.

## Non-Claims

This Phase 3D run does **not** claim:
- production-ready status
- G2 completion
- pilot authorization
- operator signoff
- PostgreSQL/multi-node/HA readiness
- nip.io as production-suitable TLS domain

Phase 3D evidence is from GCP non-prod rehearsal only and does not substitute for canonical operator review via docs 54/58/59/63/65.

---

## Target Environment

| Field | Value |
|-------|-------|
| Project | `fairy-b13f4` |
| Region | `asia-southeast1` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Static IP | `34.158.51.8` |
| TLS Domain | `34-158-51-8.nip.io` |
| HTTPS URL | `https://34-158-51-8.nip.io` |
| App Port | `19080` (localhost, behind Caddy) |
| TLS Terminator | Caddy v2.11.2 |
| Store | SQLite `/var/lib/ferrumgate/data/ferrumgate.db` |
| Backup Dir | `/var/lib/ferrumgate/backups` |

---

## Restore Drill Evidence

**Date**: 2026-05-08

### Input

| Parameter | Value |
|-----------|-------|
| Latest backup file | `ferrumgate_20260508_154446.db` |
| Backup source | `/var/lib/ferrumgate/backups/ferrumgate_20260508_154446.db` |

### Procedure

1. Identified latest backup: `ferrumgate_20260508_154446.db`
2. Initiated restore to temporary copy: `ferrumgate_restore_drill_20260508_165658.db`
3. Ran `PRAGMA integrity_check` on restored database
4. Counted tables in restored schema
5. Removed restore copy after verification

### Observed Results

| Check | Expected | Observed |
|-------|----------|----------|
| Restore copy created | Yes | Yes (`ferrumgate_restore_drill_20260508_165658.db`) |
| `PRAGMA integrity_check` | ok | `ok` |
| Table count | > 0 | 14 |
| Restore copy removed | Yes | Yes |

### Output

```
LATEST_BACKUP=ferrumgate_20260508_154446.db
RESTORE_COPY=ferrumgate_restore_drill_20260508_165658.db
INTEGRITY=ok
TABLE_COUNT=14
RESTORE_COPY_REMOVED=yes
```

### Caveats

- Drill performed on GCP non-prod SQLite store, not production store
- Production restore drill should be performed in production-adjacent environment
- RPO = time since last backup; any writes after last backup are lost on restore

---

## Metrics Snapshot

Collected from `GET /v1/metrics` on `https://34-158-51-8.nip.io/v1/metrics`

### Store Health

| Metric | Value |
|--------|-------|
| `ferrumgate_store_health_up` | 1 |
| `ferrumgate_write_queue_depth` | 0 |

Interpretation: Store is healthy (`up=1`), write queue is empty (`depth=0`).

### Request Counts (lifetime)

| Endpoint | Request Count |
|----------|---------------|
| `/v1/healthz` | 7 |
| `/v1/readyz` | 4 |
| `/v1/readyz/deep` | 3 |
| `/v1/metrics` | 5 |

### Error Counts

| Metric | Value |
|--------|-------|
| `readyz/deep` 503 count | 0 |

No 503 errors observed on `/v1/readyz/deep` during this session.

---

## TLS/Auth Probe Results

| Probe | Expected | Observed |
|-------|----------|----------|
| `GET /v1/approvals` without bearer token | 401 | 401 |
| `GET /v1/approvals` with VM-local bearer token | 200 | 200 |

Token handling: Full token retrieved on-VM via `sudo cat /etc/ferrumgate/ferrumgate_initial_token`. Only used for immediate probe. Never printed to logs or committed.

---

## Phase 3C Read-Only Smoke (Fail-Closed)

Script: `scripts/gcp/phase3c_live_rehearsal.sh`

### Results

| Check | Expected | Observed |
|-------|----------|----------|
| `https://34-158-51-8.nip.io/v1/healthz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/readyz/deep` | 200 | 200 |
| `https://34-158-51-8.nip.io/v1/metrics` | 200 | 200 |
| `caddy.service` | active | active |
| `ferrumgate.service` | active | active |
| `ferrumgate-backup.timer` | enabled | enabled |
| Firewall: SSH 22 | From `118.69.4.63/32` | Yes |
| Firewall: app 19080 | From `118.69.4.63/32` | Yes |
| Firewall: HTTP 80 | From `0.0.0.0/0` | Yes |
| Firewall: HTTPS 443 | From `0.0.0.0/0` | Yes |
| `GET /v1/approvals` no token | 401 | 401 |
| `GET /v1/approvals` with token | 200 | 200 |

**Result**: `PASSED: All checks succeeded.`

Script exit code: 0

---

## Light Workload Smoke Test

Sequential single-request smoke test (5 rounds per endpoint):

| Endpoint | Success Rate | Notes |
|----------|--------------|-------|
| `/v1/healthz` | 5/5 | HTTP 200 all rounds |
| `/v1/readyz` | 5/5 | HTTP 200 all rounds |
| `/v1/readyz/deep` | 5/5 | HTTP 200 all rounds |
| `/v1/metrics` | 5/5 | HTTP 200 all rounds |

All endpoints responded correctly under sequential single-request load.

---

## Phase 3D G2 Gate Readiness Summary

| Gate | Name | Status | Basis |
|------|------|--------|-------|
| G2.1 | Target workload model | operator-required | No workload model provided |
| G2.2 | Bearer auth + TLS + firewall | ready | TLS via nip.io+Caddy works; auth 401/200 confirmed |
| G2.3 | Backup schedule evidence | partial | Timer enabled; manual backup confirmed; production schedule pending |
| G2.4 | Restore drill | ready | INTEGRITY=ok; 14 tables; copy removed |
| G2.5 | RPO/RTO acceptance | operator-required | No formal RPO/RTO acceptance provided |
| G2.6 | Production evaluation framework | partial | Repo-side tests pass; operator framework pending |
| G2.7 | Accepted-risk review | partial | Weak spots resolved; operator signoff pending |
| G2.8 | Compensate noop risk | partial | Compensate flow verified; operator acceptance pending |

**Conservative conclusion**: G2 is NOT complete. The GCP non-prod target is ready for operator review only. All G2 gates remain open pending operator signoff via canonical doc 54.

---

## References

- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C artifact: [2026-05-08-gcp-phase3c-live-rehearsal.md](./2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D checklist: [98-phase3d-g2-readiness-checklist.md](../98-phase3d-g2-readiness-checklist.md)
- Operator review packet: [97-phase3ab-operator-review-packet.md](../97-phase3ab-operator-review-packet.md)
