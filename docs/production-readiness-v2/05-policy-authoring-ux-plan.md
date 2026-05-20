# 05 — Policy Authoring UX Plan

> **Status**: POL-1 local CLI complete; POL-2 simulate complete; POL-5 design complete; templates deferred.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Make policy authoring usable without reading Rust code or internal implementation details. Provide validate, simulate, template, diff, and rollback capabilities through ferrumctl.

## Current state

- Policy bundles have YAML/JSON model and CRUD API.
- `ferrumctl` has policy bundle CRUD: create, get, list, update, delete, set-active.
- Validation function exists but UX is insufficient.
- No simulate/dry-run capability.
- No template library.
- No policy diff/versioning.

## Gaps

| Gap | Priority | Why |
|-----|----------|-----|
| ~~No `ferrumctl policy validate`~~ | P0 | Done — local offline validation via `ferrumctl policy validate --file` |
| ~~No `ferrumctl policy simulate`~~ | P0 | Done — online simulation via `ferrumctl policy simulate --file --proposal [--intent]`; server required |
| No policy templates | P0/P1 | Authors must write from scratch |
| No policy authoring guide | P0 | No external-facing how-to doc |
| No policy diff | P1 | Design complete; see [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md) |
| No policy version history | P1 | Design complete; see [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md) |
| No policy rollback/revert | P1 | Design complete; see [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md) |

## Implementation tasks

1. **CLI commands**
   - [x] `ferrumctl policy validate --file my-policy.yaml`
   - [x] `ferrumctl policy simulate --file my-policy.yaml --proposal proposal.json [--intent intent.json]` (online; server required)
   - [x] `ferrumctl policy apply --file my-policy.yaml` (creates inactive by default; `--activate` opt-in; no server changes)
   - [x] `ferrumctl policy diff --bundle-id my-policy` (design complete; implementation NOT STARTED)
   - [x] `ferrumctl policy rollback --bundle-id my-policy` (design complete; implementation NOT STARTED)

2. **Simulation API**
   - [x] `POST /v1/policy-bundles/simulate`
   - [ ] `POST /v1/policy/simulate` (deferred; bundle-scoped simulate covers current need)
   - Returns: decision (Allow/Deny/RequireApproval/Quarantine/DraftOnly), matched rule, reason

3. **Policy templates**
   - [ ] read-only safe baseline
   - [ ] require approval for R3
   - [ ] deny external HTTP except allowlist
   - [ ] draft-only email
   - [ ] tenant-scoped policy (later)

4. **Policy authoring guide**
   - [ ] Write `docs/guides/policy-authoring.md` (scaffold exists)
   - [ ] Include schema reference, 5+ examples, common patterns

5. **Policy version history**
   - [x] Design version table, rollback semantics, diff strategy, migration/backfill plan, provenance event — see [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md)
   - [ ] Store previous versions in DB
   - [ ] Active bundle switch audit event
   - [ ] Rollback to previous active bundle

## Acceptance criteria

- [x] POL-1: Invalid policy returns a useful error with line/rule reference. (local CLI only; no server simulation)
- [x] POL-2: Simulation returns decision and matched rule without side effect. (`POST /v1/policy-bundles/simulate` + `ferrumctl policy simulate`; test-backed; POL-5 remains open)
- [ ] POL-3: Template produces a valid policy that passes validate.
- [x] POL-4: Policy switch is auditable (provenance event).
- [x] POL-5 design: Version history, diff, and rollback design documented and accepted. — [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md)
- [ ] POL-5 implementation: Rollback to previous policy works and restores prior active bundle.

## Evidence required

- `policy-ux-test-evidence.md`
- `policy-simulation-evidence.md`
- Demo recording or test output for each POL gate

## Non-claims

- **NOT a visual builder**: Web editor/rule builder is P2/out of scope.
- **NOT production-ready**: Policy UX improvements do not change the production-ready posture.
- **NOT validated externally**: Acceptance criteria require execution; this doc is the plan.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.7, §4 Phase 5
- [`05a-policy-version-history-design.md`](05a-policy-version-history-design.md) — POL-5 design artifact
- [`docs/guides/policy-authoring.md`](../../guides/policy-authoring.md) — User-facing guide scaffold.
