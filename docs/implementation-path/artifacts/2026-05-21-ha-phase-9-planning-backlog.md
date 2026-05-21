# HA ADR Phase 9 — Concise Planning Backlog

> **Date**: 2026-05-21  
> **Status**: PLANNING — ADR approved as planning decision 2026-05-21; no implementation
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md) §Phase 9 / [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md)  
> **Non-claims**: HA/multi-node = NO; not production-ready; PostgreSQL production deployment = NO

---

## Preconditions before starting

Per [`docs/ROADMAP.md`](../../ROADMAP.md) §Phase 9, do **not** start HA implementation until:

1. PostgreSQL production foundation stable (PG-1 through PG-4).
2. Security/tenant model decided (single-tenant T1 approved; scoped tokens/RBAC implemented).
3. SLO metrics available and validated.
4. Backup/restore evidence exists.

Current status (2026-05-21):
- ✅ PG-1 local Docker baseline complete; PG-2.3a complete; PG-2.3b deferred.
- ✅ Scoped tokens / RBAC / SEC-6 implemented.
- ✅ SLO baseline ratified; canonical SLO Run #3 max-valid PASS.
- ✅ Backup/restore drill local evidence exists.
- ☐ PG target/staging production-like deployment NOT yet done.
- ✅ HA ADR approved as planning decision 2026-05-21.

## Staged plan (from ROADMAP.md)

| Stage | Item | Status |
|-------|------|--------|
| HA-1 | HA ADR approved as planning decision | ✅ APPROVED — operator delegate signoff recorded 2026-05-21; no implementation claim |
| HA-2 | Manual failover drill pass | ☐ NOT STARTED |
| HA-3 | Read replica behavior documented | ☐ NOT STARTED |
| HA-4 | Automated failover drill pass | ☐ DEFERRED |
| HA-5 | RPO/RTO measured for HA scenario | ☐ NOT STARTED |

## Recommended claim path

```
production-grade single-node PostgreSQL
→ manual failover support
→ read replica support
→ automated HA
```

Do not promise HA earlier than this sequence.

## Concise backlog items

| # | Item | Priority | Owner | Acceptance |
|---|------|----------|-------|------------|
| H.1 | Draft HA ADR covering managed-PG vs self-hosted, failover strategy, replica strategy, split-brain prevention, leader/writer model, read routing, migration handling, RPO/RTO target | P1 | Engineering + Operator | ADR reviewed and approved before any HA code |
| H.2 | Document manual failover runbook (primary down → standby promoted → ferrumd reconnect) | P2 | Engineering + Operator | Runbook exists; no live drill required for planning artifact |
| H.3 | Design read-replica routing (read-only endpoints → replica, writes → primary) | P2 | Engineering | Design doc approved; readiness probe shows replica lag |
| H.4 | Implement automated failover (only after H.1–H.3 and operator cluster available) | P3 | Engineering + Operator | Primary failure drill pass; no split-brain; RPO/RTO measured |

## Cross-references

- [`docs/ROADMAP.md`](../../ROADMAP.md) §Phase 9 — full HA staged plan and acceptance gates
- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — existing HA roadmap scaffold
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) — PG foundation prerequisites
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) §Phase 9 — evidence checklist

---

*Backlog artifact — HA ADR Phase 9 planning (2026-05-21).*
