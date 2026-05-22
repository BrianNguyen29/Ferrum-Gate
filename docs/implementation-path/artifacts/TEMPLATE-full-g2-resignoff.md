# TEMPLATE — Full G2 Re-Signoff

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output and operator signoff.
> See [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) §Claim 2 for prerequisites.
> See [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) for the original G2 signoff form.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `production-target-hostname` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **Scope of signoff** | Full G2.1–G2.8 re-signoff (not conditional) |

---

## Prerequisites Checklist

All items below must be checked with evidence artifacts before this re-signoff is valid.

| # | Prerequisite | Evidence Artifact | Status |
|---|-------------|-------------------|--------|
| P.1 | Block A closed — real domain + DNS + HTTPS | `YYYY-MM-DD-block-a-closure-evidence.md` | ☐ |
| P.2 | Workload model refreshed with target metrics | `YYYY-MM-DD-workload-model-refresh-evidence.md` | ☐ |
| P.3 | SLO default-config failure/decision evidence compiled | `docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md` | ✅ DECISION EVIDENCE — default config intentionally fails; certification requires explicit high-throughput profile |
| P.4 | MCP target-host live workload (sustained) | `docs/implementation-path/artifacts/2026-05-22-mcp-target-live-workload-evidence.md` | ✅ ENGINEERING EVIDENCE — 10/10 iterations passed; baseline smoke PASS; bounded repeated MCP lifecycle smoke; NOT exhaustive adapter matrix; NOT production traffic; operator signoff NOT obtained |
| P.5 | PostgreSQL production deployment evidence | `YYYY-MM-DD-pg-production-deployment-signoff.md` | ☐ |
| P.6 | Backup/restore drill on production PG | `YYYY-MM-DD-pg-restore-drill-evidence.md` | ☐ |
| P.7 | Security audit pass (scoped tokens, RBAC, audit log) | `YYYY-MM-DD-security-audit-evidence.md` (planning reference: `docs/implementation-path/artifacts/2026-05-22-security-audit-evidence.md`) | ☐ |

**Overall prerequisites**: `PASS / FAIL` *(requires all P.1–P.7 checked)*

---

## G2 Item-by-Item Re-Signoff

### G2.1 — Workload Model Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `docs/implementation-path/artifacts/2026-05-22-workload-model-refresh-evidence.md` |
| Original signed assumption | ≤300 writes/s sustained, ≤300 writes/s peak, ≤1M writes/day (BrianNguyen, 2026-05-09) |
| Assumed vs observed throughput | Assumed ≤300 writes/s; observed target-host canonical run total 2,380 requests with zero errors; **assumption never approached** |
| Latency p50/p95/p99 per endpoint | Canonical target p50 ~190 ms, p95 ~380 ms, p99 ~394 ms; local steady-state p99 ~2 ms |
| Capacity ceiling observed | 2,380 requests, 0 errors, under max-valid rate-limit config (1000/10000); NOT a 300 writes/s validation |
| Recommended safe limits | Sustained ≤10 req/s, burst ≤50 req/s (engineering recommendation only; pending operator review) |
| Pass/Fail | ☐ — **Requires operator review and re-signoff** |

### G2.2 — Auth / TLS / Security Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `YYYY-MM-DD-security-audit-evidence.md` |
| Auth mode in production | `bearer` |
| TLS termination | *(e.g., Caddy/nginx/L4)* |
| Scoped token enforcement | `enabled / disabled` |
| Secret leak scan result | `PASS / FAIL` |
| Pass/Fail | ☐ |

### G2.3 — Backup Schedule Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `YYYY-MM-DD-pg-scheduled-backup-evidence.md` |
| Backup frequency | *(e.g., every 15 min)* |
| Retention policy | *(e.g., 4 days local, 30 days offsite)* |
| Offsite target | *(e.g., GCS/S3/rsync)* |
| Last successful backup timestamp | `YYYY-MM-DD HH:MM:SS UTC` |
| Pass/Fail | ☐ |

### G2.4 — Restore Drill Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `YYYY-MM-DD-pg-restore-drill-evidence.md` |
| Drill date | `YYYY-MM-DD` |
| Restore method | `pg_restore` / `psql` / other |
| Row count validation | `PASS / FAIL` |
| Hash validation | `PASS / FAIL` |
| `/v1/readyz/deep` after restore | `200 / other` |
| RTO measured | `N seconds` |
| Pass/Fail | ☐ |

### G2.5 — RPO / RTO Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `YYYY-MM-DD-rpo-rto-measurement-evidence.md` |
| RPO target | *(e.g., 15 minutes)* |
| RPO measured | *(fill)* |
| RTO target | *(e.g., 5 minutes)* |
| RTO measured | *(fill)* |
| Pass/Fail | ☐ |

### G2.6 — Production Evaluation Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md` (decision evidence: default fails by design; pass requires explicit profile) |
| SLO canonical run result | `PASS under explicit profile / FAIL under default` |
| SLO sustained window result | `PASS / FAIL` |
| Error rate | `N%` |
| 429 rate | `N%` |
| p99 latency | `N ms` |
| Pass/Fail | ☐ |

### G2.7 — Accepted-Risk Review Refreshed

| Field | Value |
|-------|-------|
| Block A status | `CLOSED` *(required for full G2)* |
| Remaining open blockers | *(list or "none")* |
| Known limitations acknowledged | *(list)* |
| Operator risk acceptance | `YES / NO` |
| Pass/Fail | ☐ |

### G2.8 — Compensate / Noop Risk Refreshed

| Field | Value |
|-------|-------|
| Evidence artifact | `docs/implementation-path/artifacts/2026-05-22-compensate-path-evidence.md` (local/conditional evidence only) |
| Compensate path tested | `YES / NO` |
| Noop path tested | `YES / NO` |
| Rollback contract coverage | `N%` |
| Pass/Fail | ☐ |

**Overall G2 status**: `PASS / FAIL` *(requires G2.1–G2.8 all checked)*

---

## Non-Claims

- **NOT a conditional signoff**: This template is for **full** G2 closure, not conditional pilot scope. Conditional re-signoff (2026-05-21) does not satisfy this template.
- **NOT valid without Block A closure**: If Block A is not closed (real domain + DNS + HTTPS), this signoff cannot be completed.
- **NOT self-executing**: This template records signoff only after all G2 items have real evidence.
- **NOT retroactive**: Re-signoff applies only to the specific version, environment, and evidence artifacts listed.
- **NOT a substitute for operator judgment**: The operator must independently evaluate each G2 item.

---

## Signoff

### Planning/Template-Readiness Signoff (BrianNguyen)

> **Signed by**: BrianNguyen (session authorization)
> **Date**: 2026-05-22
> **Scope**: This template is reviewed and accepted as a valid full G2 re-signoff form.
> **Nature**: Planning/decision document signoff only. This does **not** constitute evidence that any G2 item is satisfied or that full G2 is complete. Does **not** substitute for missing evidence.
> **Authority**: User explicitly authorized delegated signoff for planning and template readiness.

| Template Section | Status |
|-----------------|--------|
| Prerequisites checklist | ✅ Template ready |
| G2.1–G2.8 item tables | ✅ Template ready |
| Non-claims section | ✅ Template ready |
| Final operator signoff block | ✅ Template ready (intentionally blank below) |

### Final Operator Signoff (Intentionally Blank — Requires Real Evidence)

> **Operator name**: ________________________
> **Date**: ________________________
> **Signature / Ack**: ________________________
>
> **I confirm that**:
> - Block A is closed (real domain, DNS, HTTPS verified).
> - I have reviewed evidence for G2.1 through G2.8 and find each adequate.
> - I accept responsibility for the production posture of this deployment.
> - I understand that this is a full G2 signoff, not conditional pilot scope.

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | | |
| Operator (required) | | | |

---

## Related Docs

- [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) — Original G2 signoff form
- [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md)
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md)
- [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md)
- [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md)
- [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) — Conditional re-signoff precedent
