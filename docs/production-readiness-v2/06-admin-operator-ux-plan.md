# 06 — Admin/Operator UX Plan

> **Status**: UX-1–UX-6 CLI complete for current scope; broader admin APIs/dashboard deferred.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Enable operators to run and observe the system without spelunking through docs or source code. Start with ferrumctl expansion, then add admin APIs, and consider web UI/TUI later.

## Current state

- `ferrumctl` has: health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, policy bundle CRUD, backup/restore, admin status/executions/approvals/tokens/audit/backup helpers.
- No admin dashboard.
- Token lifecycle CLI exists for current scoped-token scope; no full user-management UI.
- `ferrumctl` is now the current operator-plane CLI; broader web/TUI UX remains deferred.

## Gaps

| Gap | Why |
|-----|-----|
| ~~No system status CLI~~ | Done — `ferrumctl admin status` aggregates health/readiness/deep/functional/metrics |
| ~~No execution viewer CLI~~ | Done — `ferrumctl admin executions list/get/cancel` wired to existing endpoints (list uses intents API) |
| ~~No approval queue CLI~~ | Done — `ferrumctl admin approvals list/get/resolve` wired to existing endpoints |
| ~~No policy manager CLI~~ | Done — `ferrumctl policy validate/apply` provides validation and apply via existing endpoints |
| ~~No backup/restore CLI beyond basic~~ | Done — `ferrumctl admin backup create/verify/restore` delegates to existing offline helpers |
| ~~No token/actor management CLI~~ | Done — `ferrumctl admin tokens` supports list/create/revoke/rotate scoped tokens for current scope |

## Implementation tasks

1. **Extend ferrumctl first** (recommended order)
   - [x] `ferrumctl admin status` — health, ready/deep, backend, backup age, queue depth, version (local CLI aggregation of existing endpoints; no new `/v1/admin/status`)
   - [x] `ferrumctl admin executions` — list/get/cancel using existing endpoints (list uses intents API; actor/time filters not yet supported)
   - [x] `ferrumctl admin approvals` — pending approvals with inspect/approve/reject (local CLI using existing `/v1/approvals` endpoints; no new admin API)
   - [x] `ferrumctl admin tokens` — list/create/revoke/rotate scoped tokens
   - [x] `ferrumctl admin backup` — create/verify/restore using existing offline helpers (no new server endpoint; no scheduler/remote backup)
   - [ ] `ferrumctl admin config` — view current effective config (redact token)

2. **Add admin APIs where needed**
   - [ ] `GET /v1/admin/status`
   - [ ] `GET /v1/admin/executions` with filters
   - [ ] `POST /v1/admin/approvals/{id}/approve`
   - [ ] `POST /v1/admin/approvals/{id}/reject`
   - [x] `POST /v1/admin/tokens`
   - [x] `DELETE /v1/admin/tokens/{id}`

3. **Web UI/TUI (later)**
   - [ ] Simple web dashboard (P2)
   - [ ] TUI alternative (P2)

## Acceptance criteria

- [x] UX-1: Operator can view current health/status without curl. (local CLI aggregation of existing endpoints; no new admin API)
- [x] UX-2: Operator can approve/reject without curl. (local CLI using existing approval endpoints; no new admin API)
- [x] UX-3: Operator can inspect execution lineage from CLI. (local CLI using existing execution/intent endpoints; no new admin API)
- [x] UX-4: Operator can rotate/revoke token from CLI.
- [x] UX-5: Operator can validate/apply policy from CLI. (local CLI using existing policy bundle endpoints; no new admin API; POL-4 audit switch remains open)
- [x] UX-6: Operator can run/verify backup from CLI. (local CLI delegation to existing offline backup helpers; no scheduler/remote backup)

## Evidence

- [`2026-05-20-scoped-token-implementation-evidence.md`](../implementation-path/artifacts/2026-05-20-scoped-token-implementation-evidence.md) — admin token APIs and `ferrumctl admin tokens` CLI evidence.
- [`2026-05-21-sec6-audit-log-implementation-evidence.md`](../implementation-path/artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md) — `ferrumctl admin audit list` evidence.
- [`2026-05-27-phase4-security-operator-signoff.md`](../implementation-path/artifacts/2026-05-27-phase4-security-operator-signoff.md) — operator evidence review/signoff for current Phase 4 scope.

## Non-claims

- **NOT a web dashboard**: CLI-first; web UI is P2 deferred.
- **NOT production-ready**: Operator UX does not change the production-ready posture.
- **NOT full RBAC**: Token management in this phase is scoped to admin/operator basics.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.6, §4 Phase 6
- [`docs/guides/operator.md`](../../guides/operator.md) — User-facing operator guide scaffold.
