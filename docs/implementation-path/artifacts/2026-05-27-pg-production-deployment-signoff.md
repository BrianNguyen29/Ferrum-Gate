# PostgreSQL Production Deployment Signoff — 2026-05-27

> **Artifact ID**: 2026-05-27-pg-production-deployment-signoff
> **Date**: 2026-05-27
> **Owner**: Engineering + Operator
> **Scope**: Tier 1.5 Batch 1 — Consolidated signoff for PG-P.1 to PG-P.6
> **Constraint**: Target VM deployment. No production-ready claim.

---

## 1. Executive Summary

All 6 PostgreSQL production deployment gates (PG-P.1 to PG-P.6) have been successfully completed on the ferrumgate-nonprod VM. This artifact consolidates evidence from individual gate artifacts.

---

## 2. Gate Completion Status

| Gate | Description | Status | Evidence Artifact |
|------|-------------|--------|-------------------|
| PG-P.1 | Target PostgreSQL provisioned and reachable | ✅ COMPLETE | [2026-05-27-pg-target-deployment-evidence.md](./2026-05-27-pg-target-deployment-evidence.md) |
| PG-P.2 | ferrumd starts with production PG DSN | ✅ COMPLETE | [2026-05-27-pg-target-deployment-evidence.md](./2026-05-27-pg-target-deployment-evidence.md) |
| PG-P.3 | TLS/SSL encrypted DSN validated | ✅ COMPLETE | [2026-05-27-pg-tls-dsn-evidence.md](./2026-05-27-pg-tls-dsn-evidence.md) |
| PG-P.4 | PgBouncer connection pooling operational | ✅ COMPLETE | [2026-05-27-pg-pgbouncer-evidence.md](./2026-05-27-pg-pgbouncer-evidence.md) |
| PG-P.5 | Backup/restore drill passes | ✅ COMPLETE | [2026-05-27-pg-restore-drill-evidence.md](./2026-05-27-pg-restore-drill-evidence.md) |
| PG-P.6 | Alert rules deployed to Prometheus | ✅ COMPLETE | [2026-05-27-pg-alert-deployment-evidence.md](./2026-05-27-pg-alert-deployment-evidence.md) |

---

## 3. Deployment Summary

### Infrastructure Stack

| Component | Version/Details |
|-----------|-----------------|
| PostgreSQL | 16.14 (Ubuntu 16.14-1.pgdg24.04+1) |
| PgBouncer | 1.25.2 (transaction mode) |
| TLS | Self-signed CA, TLSv1.3, TLS_AES_256_GCM_SHA384 |
| Backup | pg_dump -Fc, every 15 min, 4-day retention |
| Offsite | GCS bucket via gsutil rsync |
| Monitoring | Prometheus + 5 PG-specific alert rules |

### ferrumd Configuration

- **Store backend**: PostgreSQL via PgBouncer
- **DSN**: `postgres://ferrumgate_app@127.0.0.1:6432/ferrumgate?sslmode=disable`
- **Pool settings**: max=10, min_idle=2, acquire_timeout=5s
- **Health**: All components healthy (store, write_queue, pool)

### Data Migration

- **Source**: SQLite /var/lib/ferrumgate/ferrumgate.db (6.9 MB)
- **Target**: PostgreSQL ferrumgate database
- **Rows migrated**: 4,511 total (intents: 4459, proposals: 13, capabilities: 13, provenance_events: 26)
- **Verification**: Row counts match exactly

---

## 4. Non-Claims (Preserved)

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** — Target VM deployment only |
| **full G2** | **NOT COMPLETE** — G2.1–G2.8 remain conditional pilot only |
| **Block A** | **WAIVED/CONDITIONAL** — Real domain still required for Tier 2 |
| **HA/multi-node** | **NO** — Single-node PostgreSQL |
| **Automated failover** | **NO** — Manual intervention required |
| **Sustained SLO window** | **NO** — Bounded validation only |
| **Real domain** | **NO** — Tier 1.5 is domainless |

---

## 5. Tier 1.5 Progress

With PG-P.1 to PG-P.6 complete, the PostgreSQL production deployment component of Tier 1.5 is **COMPLETE**.

Remaining Tier 1.5 components:
- ☐ HA multi-node topology (Batch 2)
- ☐ Automated failover (Batch 3)
- ☐ Operator acknowledgment

---

## 6. Related Artifacts

- [2026-05-27-pg-target-deployment-evidence.md](./2026-05-27-pg-target-deployment-evidence.md)
- [2026-05-27-pg-tls-dsn-evidence.md](./2026-05-27-pg-tls-dsn-evidence.md)
- [2026-05-27-pg-pgbouncer-evidence.md](./2026-05-27-pg-pgbouncer-evidence.md)
- [2026-05-27-pg-restore-drill-evidence.md](./2026-05-27-pg-restore-drill-evidence.md)
- [2026-05-27-pg-alert-deployment-evidence.md](./2026-05-27-pg-alert-deployment-evidence.md)
- [docs/production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md](../../production-readiness-v2/00b-tier-1.5-domainless-infrastructure.md)
- [docs/production-readiness-v2/13-tier-1.5-completion-status.md](../../production-readiness-v2/13-tier-1.5-completion-status.md)

---

*Artifact created: 2026-05-27. PostgreSQL production deployment consolidated signoff. No production-ready claim.*
