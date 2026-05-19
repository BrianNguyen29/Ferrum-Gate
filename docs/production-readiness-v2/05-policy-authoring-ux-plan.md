# 05 — Policy Authoring UX Plan

> **Status**: Planning artifact. Not implemented.
> **Owner**: Engineering
> **Last updated**: 2026-05-18
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
| No `ferrumctl policy validate` | P0 | Authors cannot check policy before applying |
| No `ferrumctl policy simulate` | P0 | Authors cannot preview decisions against sample intents |
| No policy templates | P0/P1 | Authors must write from scratch |
| No policy authoring guide | P0 | No external-facing how-to doc |
| No policy diff | P1 | Cannot compare versions |
| No policy version history | P1 | Cannot audit or rollback |
| No policy rollback/revert | P1 | Cannot safely undo a bad change |

## Implementation tasks

1. **CLI commands**
   - [ ] `ferrumctl policy validate --file my-policy.yaml`
   - [ ] `ferrumctl policy simulate --file my-policy.yaml --intent sample-intent.json`
   - [ ] `ferrumctl policy apply --file my-policy.yaml --inactive`
   - [ ] `ferrumctl policy diff --bundle-id my-policy`
   - [ ] `ferrumctl policy rollback --bundle-id my-policy`

2. **Simulation API**
   - [ ] `POST /v1/policy-bundles/simulate`
   - [ ] `POST /v1/policy/simulate`
   - Returns: decision (Allow/Deny/RequireApproval/Quarantine/DraftOnly), matched rule, risk class

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
   - [ ] Store previous versions in DB
   - [ ] Active bundle switch audit event
   - [ ] Rollback to previous active bundle

## Acceptance criteria

- [ ] POL-1: Invalid policy returns a useful error with line/rule reference.
- [ ] POL-2: Simulation returns decision and matched rule without side effect.
- [ ] POL-3: Template produces a valid policy that passes validate.
- [ ] POL-4: Policy switch is auditable (provenance event).
- [ ] POL-5: Rollback to previous policy works and restores prior active bundle.

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
- [`docs/guides/policy-authoring.md`](../../guides/policy-authoring.md) — User-facing guide scaffold.
