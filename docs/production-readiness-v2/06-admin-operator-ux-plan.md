# 06 — Admin/Operator UX Plan

> **Status**: Planning artifact. Not implemented.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Enable operators to run and observe the system without spelunking through docs or source code. Start with ferrumctl expansion, then add admin APIs, and consider web UI/TUI later.

## Current state

- `ferrumctl` has: health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, policy bundle CRUD, backup/restore.
- No admin dashboard.
- No token/user/policy operator UI.
- `ferrumctl` is not yet an operator-plane tool.

## Gaps

| Gap | Why |
|-----|-----|
| No system status CLI | Operator cannot see health/backend/backup age at a glance |
| No execution viewer CLI | Operator must curl or read DB to see execution state |
| No approval queue CLI | Operator cannot approve/reject without raw API calls |
| No policy manager CLI | Operator cannot validate/simulate/apply from CLI |
| No backup/restore CLI beyond basic | No drill mode, no backup list, no verification |
| No token/actor management CLI | No scoped token creation, revocation, rotation |

## Implementation tasks

1. **Extend ferrumctl first** (recommended order)
   - [ ] `ferrumctl admin status` — health, ready/deep, backend, backup age, queue depth, version
   - [ ] `ferrumctl admin executions` — list with filters (state, actor, time, risk tier)
   - [ ] `ferrumctl admin approvals` — pending approvals with inspect/approve/reject
   - [ ] `ferrumctl admin tokens` — list/create/revoke/rotate scoped tokens
   - [ ] `ferrumctl admin backup` — run/verify/list/restore-drill
   - [ ] `ferrumctl admin config` — view current effective config (redact token)

2. **Add admin APIs where needed**
   - [ ] `GET /v1/admin/status`
   - [ ] `GET /v1/admin/executions` with filters
   - [ ] `POST /v1/admin/approvals/{id}/approve`
   - [ ] `POST /v1/admin/approvals/{id}/reject`
   - [ ] `POST /v1/admin/tokens`
   - [ ] `DELETE /v1/admin/tokens/{id}`

3. **Web UI/TUI (later)**
   - [ ] Simple web dashboard (P2)
   - [ ] TUI alternative (P2)

## Acceptance criteria

- [ ] UX-1: Operator can view current health/status without curl.
- [ ] UX-2: Operator can approve/reject without curl.
- [ ] UX-3: Operator can inspect execution lineage from CLI.
- [ ] UX-4: Operator can rotate/revoke token from CLI.
- [ ] UX-5: Operator can validate/apply policy from CLI.
- [ ] UX-6: Operator can run/verify backup from CLI.

## Evidence required

- `operator-ux-test-evidence.md`
- Demo recording or test output for each UX gate

## Non-claims

- **NOT a web dashboard**: CLI-first; web UI is P2 deferred.
- **NOT production-ready**: Operator UX does not change the production-ready posture.
- **NOT full RBAC**: Token management in this phase is scoped to admin/operator basics.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.6, §4 Phase 6
- [`docs/guides/operator.md`](../../guides/operator.md) — User-facing operator guide scaffold.
