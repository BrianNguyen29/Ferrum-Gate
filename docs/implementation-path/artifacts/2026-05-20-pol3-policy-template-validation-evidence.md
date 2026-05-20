# POL-3 Evidence — Policy Templates Validated

> **Artifact ID**: 2026-05-20-pol3-policy-template-validation-evidence
> **Date**: 2026-05-20
> **Owner**: Engineering
> **Scope**: POL-3 acceptance criterion in `docs/production-readiness-v2/05-policy-authoring-ux-plan.md`

---

## Claim

Each policy template in `docs/guides/policy-authoring.md` produces a valid policy bundle that passes `ferrumctl policy validate`.

## Evidence

### Validation method

Local offline validation using the built `ferrumctl` binary:

```bash
ferrumctl policy validate --file <template>.yaml
```

All validation executed locally against the current debug build. No server, target host, or external calls were used.

### Templates validated

| # | Template name | Bundle ID | Validation result |
|---|---------------|-----------|-------------------|
| 1 | Scope-safe baseline | `scope-safe-baseline` | {"valid":true} |
| 2 | Require approval for R3 | `r3-approval-required` | {"valid":true} |
| 3 | Draft-only external communication | `draft-only-external` | {"valid":true} |
| 4 | Deny mutations baseline | `deny-mutations` | {"valid":true} |
| 5 | Scope-restricted baseline | `scope-restricted-baseline` | {"valid":true} |
| 6 | High-taint quarantine | `high-taint-quarantine` | {"valid":true} |
| 7 | External API call approval | `external-api-approval` | {"valid":true} |

**Total**: 7/7 templates passed validation.

### Command log

```
$ ferrumctl policy validate --file 01-scope-safe-baseline.yaml
{"valid":true}
$ ferrumctl policy validate --file 02-r3-approval-required.yaml
{"valid":true}
$ ferrumctl policy validate --file 03-draft-only-external.yaml
{"valid":true}
$ ferrumctl policy validate --file 04-deny-mutations.yaml
{"valid":true}
$ ferrumctl policy validate --file 05-scope-restricted-baseline.yaml
{"valid":true}
$ ferrumctl policy validate --file 06-high-taint-quarantine.yaml
{"valid":true}
$ ferrumctl policy validate --file 07-external-api-approval.yaml
{"valid":true}
```

## Limitations

- **Schema scope**: Templates use the implemented matcher set (`scope_mismatch`, `taint_at_least`, `action_is_mutation`, `rollback_class_equals`, `action_type_equals`). Aspirational matchers from earlier doc drafts (`action_in`, `risk_class`, `target_not_in`, `tenant_id`, `rollback_prepared`) are not yet implemented and have been removed from the guide.
- **No simulation run**: Validation confirms YAML schema correctness only. Simulation (`ferrumctl policy simulate`) requires a running server and was not executed for these templates.
- **No apply test**: Templates were not uploaded to a live store.
- **Default Allow**: The runtime evaluation returns `Allow` when no rule matches. Templates that rely on implicit default-deny behavior should be paired with an explicit catch-all deny rule where possible (the current matcher set does not support a universal catch-all).

## Non-claims

- **NOT production-ready**: Template validation does not change the production-ready posture.
- **NOT externally validated**: All checks ran locally against the debug build.
- **NOT a schema guarantee**: The bundle schema may evolve; re-validation is recommended after upgrades.

## Related docs

- `docs/guides/policy-authoring.md` — Updated guide with validated templates
- `docs/production-readiness-v2/05-policy-authoring-ux-plan.md` — POL-3 acceptance source
- `docs/production-readiness-v2/10-evidence-checklist.md` — Phase 5 checklist
- `docs/implementation-path/artifacts/2026-05-19-doc5-policy-templates-evidence.md` — Prior template inventory (schema updated since)
