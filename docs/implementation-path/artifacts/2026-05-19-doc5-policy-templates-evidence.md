# DOC-5 Evidence — Policy Templates Added

> **Artifact ID**: 2026-05-19-doc5-policy-templates-evidence
> **Date**: 2026-05-19
> **Owner**: Engineering
> **Scope**: DOC-5 acceptance criterion in `docs/production-readiness-v2/07-product-docs-plan.md`

---

## Claim

`docs/guides/policy-authoring.md` contains at least 5 policy templates/examples with clear purpose, when-to-use guidance, YAML code blocks, and caveats.

## Evidence

### File changed

- `docs/guides/policy-authoring.md` — expanded Templates section (2026-05-19)

### Template inventory

> **2026-05-20 update**: The original DOC-5 template scaffolds were rewritten to match the implemented
> `ferrumctl policy validate` schema. The table below reflects the current guide inventory. See
> `2026-05-20-pol3-policy-template-validation-evidence.md` for the validation run.

| # | Template name | Purpose summary | Status |
|---|---------------|-----------------|--------|
| 1 | Scope-safe baseline | Deny scope mismatch and quarantine high-taint mutations | Added; validated 2026-05-20 |
| 2 | Require approval for R3 | Escalate R3 to human approval | Added |
| 3 | Draft-only external communication | Allow draft-only external communication | Added; validated 2026-05-20 |
| 4 | Deny mutations baseline | Deny mutating actions outright | Added; validated 2026-05-20 |
| 5 | Scope-restricted baseline | Require declared resource scope for non-R0 actions | Added; validated 2026-05-20 |
| 6 | High-taint quarantine | Hold high-taint mutations for review | Added; validated 2026-05-20 |
| 7 | External API call approval | Require approval for external API calls | Added; validated 2026-05-20 |

**Total**: 7 templates — exceeds the minimum of 5.

### Required templates from `05-policy-authoring-ux-plan.md`

- [x] read-only/scope-safe baseline (Template 1)
- [x] require-approval-R3 (Template 2)
- [x] external communication/API controls (Templates 3 and 7; URL-level allowlist matcher not implemented)
- [x] draft-only external communication (Template 3)
- [x] scope-restricted baseline as tenant-scoping substitute (Template 5; true tenant matcher deferred to Phase 4)

### Quality checklist

- [x] Each template has a **Purpose** paragraph.
- [x] Each template has a **When to use** paragraph.
- [x] Each template has a **Caveats** paragraph.
- [x] Each template includes a **YAML code block** with a valid bundle scaffold.
- [x] No template claims runtime validation or simulation support. (Superseded by POL-3 validation on 2026-05-20.)

## Non-claims (as of 2026-05-19)

- **NOT validated at runtime**: These are documentation scaffolds. No `ferrumctl policy validate` or `ferrumctl policy simulate` execution was performed. (Superseded by `2026-05-20-pol3-policy-template-validation-evidence.md`.)
- **NOT a guarantee of schema correctness**: The YAML examples follow the documented bundle schema but have not been checked by a live validator. (Superseded by `2026-05-20-pol3-policy-template-validation-evidence.md`.)
- **NOT production-ready**: Adding templates does not change the production-ready posture.
- **No policy simulator implemented**: CLI validation/simulation remains planned per `docs/ROADMAP.md` §4 Phase 5. (Superseded; validate/simulate/apply/diff/rollback are now implemented.)

## Related docs

- `docs/guides/policy-authoring.md` — Updated guide (schema corrected and templates validated 2026-05-20)
- `docs/production-readiness-v2/05-policy-authoring-ux-plan.md` — Source template list
- `docs/production-readiness-v2/07-product-docs-plan.md` — DOC-5 acceptance
- `docs/production-readiness-v2/10-evidence-checklist.md` — Phase 7 checklist
- `docs/implementation-path/artifacts/2026-05-20-pol3-policy-template-validation-evidence.md` — POL-3 validation evidence
