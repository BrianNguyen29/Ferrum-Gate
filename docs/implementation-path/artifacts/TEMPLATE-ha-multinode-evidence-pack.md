# TEMPLATE — HA / Multi-Node Evidence Pack

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output and operator signoff.
> See [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) for the HA staged plan.
> See [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) §Claim 5 for prerequisites.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `production-ha-cluster-name` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **PostgreSQL version / HA tool** | `e.g., Patroni 3.0 / managed PG` |
| **Node count** | `N` |

---

## Prerequisites Checklist

All items below must be checked before this evidence pack is valid.

| # | Prerequisite | Evidence Artifact | Status |
|---|-------------|-------------------|--------|
| P.1 | HA ADR approved as planning decision | `ha-adr.md` signoff | ☐ |
| P.2 | PostgreSQL production foundation stable | `YYYY-MM-DD-pg-production-deployment-signoff.md` | ☐ |
| P.3 | Security/tenant model decided | `04-security-tenant-model-adr.md` signoff | ☐ |
| P.4 | SLO metrics available | `YYYY-MM-DD-slo-default-config-pass-evidence.md` | ☐ |
| P.5 | Backup/restore evidence exists | `YYYY-MM-DD-pg-restore-drill-evidence.md` | ☐ |
| P.6 | Manual failover runbook drafted | `manual-failover-runbook.md` | ☐ |
| P.7 | Read replica design drafted | `read-replica-design.md` | ☐ |

**Overall prerequisites**: `PASS / FAIL` *(requires all P.1–P.7 checked)*

---

## HA-2 — Manual Failover Drill

### Primary Down Detection

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| Primary failure injected | `pg_ctl stop` / VM shutdown / network partition | Primary unreachable | | ☐ |
| Detection time | Monitoring alert or health probe | `<= N seconds` | | ☐ |
| Alert fired | Alertmanager / PagerDuty / email | Alert received | | ☐ |

### Standby Promotion

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| Standby promoted to primary | Manual command per runbook | Promotion success | | ☐ |
| Promotion time | Stopwatch | `<= N seconds` | | ☐ |
| Write capability restored | `INSERT/UPDATE` test | Success | | ☐ |

### ferrumd Reconnect / Reroute

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| ferrumd detects primary change | Pool reconnect or restart | Recovery | | ☐ |
| `/v1/readyz/deep` after reroute | Health probe | HTTP 200, `store: healthy` | | ☐ |
| Recovery time | Stopwatch | `<= N seconds` | | ☐ |
| No split-brain observed | Query both old and new primary | Only one primary accepts writes | | ☐ |

### RPO / RTO Measurement

| Metric | Target | Measured | Pass/Fail |
|--------|--------|----------|-----------|
| RPO (data loss window) | `<= N seconds` | | ☐ |
| RTO (recovery time) | `<= N seconds` | | ☐ |
| Failover duration (promotion + reconnect) | `<= N seconds` | | ☐ |

**HA-2 overall**: `PASS / FAIL`

---

## HA-3 — Read Replica Behavior

### Read Routing

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| Read-only endpoints use replica | Query routing log / EXPLAIN | Replica used | | ☐ |
| Writes go to primary | Insert test + routing log | Primary used | | ☐ |
| Replica lag metric exposed | `/v1/metrics` or PG query | Lag in seconds present | | ☐ |

### Stale Read Handling

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| Stale read documented | Runbook acknowledges lag | Documented | | ☐ |
| Lag threshold alert | Alert rule | Fires when lag > threshold | | ☐ |

**HA-3 overall**: `PASS / FAIL`

---

## HA-4 — Automated Failover (Deferred Until Prerequisites Met)

> **Note**: HA-4 is explicitly deferred until HA-2 and HA-3 are complete and stable. Do not fill this section until automated failover is implemented.

| Check | Method | Expected | Actual | Pass/Fail |
|-------|--------|----------|--------|-----------|
| Failover occurs automatically | Primary failure injection | No manual step required | | ☐ |
| No split-brain | Concurrent write tests | Only one primary at any time | | ☐ |
| Writes resume | Insert test after failover | Success | | ☐ |
| Data consistency verified | Row count + hash comparison | Match | | ☐ |
| Incident log generated | Audit log / provenance | Failover event recorded | | ☐ |

**HA-4 overall**: `PASS / FAIL` *(deferred)*

---

## Known Limitations at Time of Evidence

**Placeholder**: List any HA-specific limitations.

- [ ] *(example)* Automated failover not yet implemented — manual only.
- [ ] *(example)* Split-brain prevention relies on operator procedure, not automatic fencing.
- [ ] *(example)* Read replica lag threshold not tuned for production traffic.
- [ ] *(add as applicable)*

---

## Non-Claims

- **NOT a production-ready claim by itself**: HA evidence is a prerequisite for HA-capable production, not sufficient alone.
- **NOT validated for all topologies**: This template assumes a specific HA strategy (managed PG / Patroni / manual). Other topologies may require additional checks.
- **NOT self-executing**: This template records evidence only after real failover drills are performed.
- **NOT retroactive**: Evidence applies only to the specific cluster configuration, versions, and date listed.
- **Does not close Block A**: HA evidence is independent of domain/DNS closure.
- **Does not replace PG production signoff**: HA evidence pack is separate from `TEMPLATE-pg-production-deployment-signoff.md`.
- **Manual failover ≠ true HA**: HA-2 pass does not constitute an HA claim. True HA requires HA-4 automated failover.

---

## Signoff

### Planning/Template-Readiness Signoff (BrianNguyen)

> **Signed by**: BrianNguyen (session authorization)
> **Date**: 2026-05-22
> **Scope**: This template is reviewed and accepted as a valid HA/multi-node evidence pack form.
> **Nature**: Planning/decision document signoff only. This does **not** constitute evidence of HA implementation, multi-node deployment, automated failover, or production readiness. Does **not** substitute for missing evidence.
> **Authority**: User explicitly authorized delegated signoff for planning and template readiness.

| Template Section | Status |
|-----------------|--------|
| Prerequisites checklist | ✅ Template ready |
| HA-2 manual failover drill table | ✅ Template ready |
| HA-3 read replica behavior table | ✅ Template ready |
| HA-4 automated failover table (deferred) | ✅ Template ready |
| Non-claims section | ✅ Template ready |
| Final operator signoff block | ✅ Template ready (intentionally blank below) |

### Final Operator Signoff (Intentionally Blank — Requires Real Evidence)

> **Operator name**: ________________________
> **Date**: ________________________
> **Signature / Ack**: ________________________
>
> **I confirm that**:
> - Manual failover drill passed with measured RPO/RTO.
> - Read replica behavior is documented and tested.
> - I understand the limitations listed above.
> - I accept responsibility for ongoing HA operations and failover procedures.

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | | |
| Operator (required) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md)
- [`docs/production-readiness-v2/ha-adr.md`](../../production-readiness-v2/ha-adr.md)
- [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md)
- [`docs/production-readiness-v2/read-replica-design.md`](../../production-readiness-v2/read-replica-design.md)
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md)
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md)
- [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md)
- [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md)
- [`docs/implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md)
- [`docs/implementation-path/artifacts/HA-multi-node-evidence-runbook.md`](./HA-multi-node-evidence-runbook.md) — Operator execution guide for failover drill, RPO/RTO measurement, read replica validation, and rollback criteria
