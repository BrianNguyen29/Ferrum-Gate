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

| # | Template name | Purpose summary | Status |
|---|---------------|-----------------|--------|
| 1 | Read-only safe baseline | Allow reads/health checks; default deny | Added |
| 2 | Require approval for R3 | Escalate R3 to human approval | Added |
| 3 | Deny external HTTP except allowlist | Egress control via explicit target list | Added |
| 4 | Draft-only email | Compose drafts without sending | Added |
| 5 | Tenant-scoped policy | Restrict operations to one tenant | Added |
| 6 | Quarantine high taint | Hold high-taint actions for review | Added (pre-existing, enhanced with caveats) |
| 7 | Rollback-required baseline | Require rollback readiness for R3 | Added |

**Total**: 7 templates — exceeds the minimum of 5.

### Required templates from `05-policy-authoring-ux-plan.md`

- [x] read-only baseline (Template 1)
- [x] require-approval-R3 (Template 2)
- [x] deny-external-HTTP (Template 3)
- [x] draft-only-email (Template 4)
- [x] tenant-scoped (Template 5)

### Quality checklist

- [x] Each template has a **Purpose** paragraph.
- [x] Each template has a **When to use** paragraph.
- [x] Each template has a **Caveats** paragraph.
- [x] Each template includes a **YAML code block** with a valid bundle scaffold.
- [x] No template claims runtime validation or simulation support.

## Non-claims

- **NOT validated at runtime**: These are documentation scaffolds. No `ferrumctl policy validate` or `ferrumctl policy simulate` execution was performed.
- **NOT a guarantee of schema correctness**: The YAML examples follow the documented bundle schema but have not been checked by a live validator.
- **NOT production-ready**: Adding templates does not change the production-ready posture.
- **No policy simulator implemented**: CLI validation/simulation remains planned per `docs/ROADMAP.md` §4 Phase 5.

## Related docs

- `docs/guides/policy-authoring.md` — Updated guide
- `docs/production-readiness-v2/05-policy-authoring-ux-plan.md` — Source template list
- `docs/production-readiness-v2/07-product-docs-plan.md` — DOC-5 acceptance
- `docs/production-readiness-v2/10-evidence-checklist.md` — Phase 7 checklist
