# Policy Authoring Guide

> **Status**: Templates validated. 7 copy-pasteable policy examples. `validate`, `simulate`, `apply`, `diff`, `rollback`, and `versions` are implemented.
> **Parent**: [`guides/README.md`](./README.md)

---

## Policy schema

Policy bundles are YAML documents containing a list of rules. Each rule has a decision, a priority, and one or more matchers.

```yaml
version: "1.0.0"
bundle_id: safe-baseline
rules:
  - id: deny-scope-mismatch
    description: "Deny when requested resources are outside intent scope"
    decision: Deny
    priority: 100
    matchers:
      - type: scope_mismatch

  - id: quarantine-high-taint
    description: "Quarantine mutating actions with high taint"
    decision: Quarantine
    priority: 90
    matchers:
      - type: taint_at_least
        value: 70
      - type: action_is_mutation

  - id: require-approval-r3
    description: "Require approval for irreversible actions"
    decision: RequireApproval
    priority: 80
    matchers:
      - type: rollback_class_equals
        value: "R3IrreversibleHighConsequence"
```

### Version field

Policy bundles use [SemVer](https://semver.org/) for the `version` field (e.g., `1.0.0`, `0.2.1`). To bump versions safely, use:

```bash
# Bump minor version and write to a new file
ferrumctl author bundle bump my-policy.yaml --bump-type minor --output my-policy.v2.yaml

# Bump patch version (default), write to stdout as JSON
ferrumctl author bundle bump my-policy.yaml --output - --json
```

This increments the version and writes the updated bundle to the specified output path. See [`contracts/policy-bundle.example.yaml`](../../contracts/policy-bundle.example.yaml) for a canonical reference.

### Matcher reference

| Matcher | Fields | Meaning |
|---------|--------|---------|
| `scope_mismatch` | none | True when intent has no resource scope and the action is not R0 |
| `taint_at_least` | `value: <u8>` | True when the computed taint score is >= the value |
| `action_is_mutation` | none | True when the action is not R0 native reversible |
| `rollback_class_equals` | `value: <string>` | True when the proposal's rollback class matches the value exactly |
| `action_type_equals` | `value: <string>` | True when the inferred effect type matches the value exactly |

Rollback class values: `R0NativeReversible`, `R1SnapshotRecoverable`, `R2Compensatable`, `R3IrreversibleHighConsequence`.

Effect type values: `ReadOnlyAnalysis`, `DraftCreation`, `FileMutation`, `GitMutation`, `DatabaseMutation`, `ExternalApiCall`, `ExternalCommunication`, `Scheduling`, `AdministrativeChange`.

### Decision reference

| Decision | YAML alias | Effect |
|----------|------------|--------|
| `Allow` | — | Proposal proceeds to capability minting |
| `Deny` | — | Proposal blocked immediately |
| `Quarantine` | — | Proposal held for manual disposition |
| `RequireApproval` | `requireapproval` or `require_approval` | Human operator must approve before proceeding |
| `AllowDraftOnly` | `allowdraftonly` or `allow_draft_only` | Action limited to draft/staging; no live side effect |

## Policy authoring flow

```
Create policy from template
→ validate locally
→ simulate against sample intents
→ apply as inactive bundle
→ run dry-run evaluation
→ set active
→ monitor decisions
→ rollback if needed
```

## CLI commands

```bash
# Validate a policy file (local; no server required)
ferrumctl policy validate --file my-policy.yaml

# Simulate against a sample proposal (bundle simulation; requires running server)
ferrumctl policy simulate --file my-policy.yaml --proposal proposal.json

# Simulate with intent file and JSON output
ferrumctl policy simulate --file my-policy.yaml --proposal proposal.json --intent intent.json --json

# Runtime simulation: evaluate a live intent against the active runtime policy
# Distinction: "simulate" tests a policy bundle file; "runtime-simulate" tests the currently active policy in the runtime
ferrumctl policy runtime-simulate --proposal proposal.json

# Runtime simulation with intent file and JSON output
ferrumctl policy runtime-simulate --proposal proposal.json --intent intent.json --json

# Apply as inactive (requires running server)
ferrumctl policy apply --file my-policy.yaml

# Activate on apply
ferrumctl policy apply --file my-policy.yaml --activate

# List versions (requires running server)
ferrumctl policy versions --bundle-id my-policy

# Diff between versions (requires running server)
ferrumctl policy diff --bundle-id my-policy --from 1 --to 2

# Rollback to a specific version (requires running server)
# Required: --target-version. Optional: --actor, --json
ferrumctl policy rollback --bundle-id my-policy --target-version 2

# Example with actor and JSON output
ferrumctl policy rollback --bundle-id my-policy --target-version 2 --actor admin --json
```

## Templates

> **Caveat**: These templates use the implemented matcher set. They have been validated offline with `ferrumctl policy validate`. Simulation and apply require a running server. The runtime returns `Allow` when no rule matches, so conservative templates should place restrictive rules at higher priority.

### 1. Scope-safe baseline

**Purpose**: Deny scope-mismatched requests and quarantine high-taint mutations.

**When to use**: Initial deployment, demo environments, or as a conservative starting point before widening the permission surface.

**Caveats**: Does not block all mutations — only those with high taint or scope mismatch. Add explicit deny rules for additional restrictions. The runtime defaults to `Allow` when no rule matches.

```yaml
version: "1.0.0"
bundle_id: scope-safe-baseline
rules:
  - id: deny-scope-mismatch
    description: "Deny when requested resources are outside intent scope"
    decision: Deny
    priority: 100
    matchers:
      - type: scope_mismatch
  - id: quarantine-high-taint-mutation
    description: "Quarantine mutating actions with high taint"
    decision: Quarantine
    priority: 90
    matchers:
      - type: taint_at_least
        value: 70
      - type: action_is_mutation
```

### 2. Require approval for R3

**Purpose**: Automatically escalate any R3 (irreversible, high-consequence) action to a human approval gate.

**When to use**: Production workloads where lower-risk operations are trusted but R3 actions must not execute without sign-off.

**Caveats**: Requires an active approval workflow and operator presence. R3 actions will block until approved or rejected. Do not use if the system must run unattended.

```yaml
version: "1.0.0"
bundle_id: r3-approval-required
rules:
  - id: r3-approval
    description: "Require approval for R3 irreversible actions"
    decision: RequireApproval
    priority: 100
    matchers:
      - type: rollback_class_equals
        value: "R3IrreversibleHighConsequence"
```

### 3. Draft-only external communication

**Purpose**: Allow composing and editing drafts while prohibiting actual external delivery.

**When to use**: Review or staging environments where you want to preview content without risking accidental sends to real inboxes or external systems.

**Caveats**: `ExternalCommunication` is inferred from the proposal effect type. If the runtime cannot infer the effect type, the rule may not match. Combine with other restrictive rules for defense in depth.

```yaml
version: "1.0.0"
bundle_id: draft-only-external
rules:
  - id: draft-only-external-comm
    description: "Force draft-only for external communication"
    decision: AllowDraftOnly
    priority: 100
    matchers:
      - type: action_type_equals
        value: "ExternalCommunication"
```

### 4. Deny mutations baseline

**Purpose**: Deny all mutating actions outright.

**When to use**: Highly conservative environments, read-only analysis pipelines, or audit modes where no state change is permitted.

**Caveats**: R0 native-reversible actions are not considered mutations by this matcher. If you need to deny those too, add a separate `scope_mismatch` or explicit rule. The runtime defaults to `Allow` for non-matching proposals.

```yaml
version: "1.0.0"
bundle_id: deny-mutations
rules:
  - id: deny-all-mutations
    description: "Deny all mutating actions"
    decision: Deny
    priority: 100
    matchers:
      - type: action_is_mutation
```

### 5. Scope-restricted baseline

**Purpose**: Restrict operations by requiring a declared resource scope. Actions without a scope that are not R0 are denied.

**When to use**: Environments where every intent must declare its resource scope. Prevents open-ended operations.

**Caveats**: R0 native-reversible actions bypass this check. Operators must ensure intents declare scopes correctly. This is the closest available equivalent to tenant scoping; true multi-tenant filtering requires Phase 4 tenant model work.

```yaml
version: "1.0.0"
bundle_id: scope-restricted-baseline
rules:
  - id: deny-no-scope
    description: "Deny non-R0 actions without a declared resource scope"
    decision: Deny
    priority: 100
    matchers:
      - type: scope_mismatch
```

### 6. High-taint quarantine

**Purpose**: Automatically quarantine actions that exceed a taint-score threshold for additional review.

**When to use**: Workflows with content-scoring or data-classification pipelines where high-risk inputs should not proceed without inspection.

**Caveats**: `taint_at_least` requires an upstream taint-scoring mechanism. The threshold (80) is arbitrary; calibrate it against your data. Quarantined actions remain in a pending state and require manual disposition. This template also requires the action to be a mutation; remove `action_is_mutation` if you want to quarantine reads too.

```yaml
version: "1.0.0"
bundle_id: high-taint-quarantine
rules:
  - id: quarantine-high-taint
    description: "Quarantine mutating actions with high taint score"
    decision: Quarantine
    priority: 100
    matchers:
      - type: taint_at_least
        value: 80
      - type: action_is_mutation
```

### 7. External API call approval

**Purpose**: Require explicit approval for any external API call.

**When to use**: Environments with strict egress control, or when integrating with external services where every call must be operator-approved.

**Caveats**: The effect type is inferred from the proposal; if inference fails, the rule may not match. There is no URL-level allowlist matcher yet; this rule applies to all `ExternalApiCall` actions.

```yaml
version: "1.0.0"
bundle_id: external-api-approval
rules:
  - id: require-approval-external-api
    description: "Require approval for external API calls"
    decision: RequireApproval
    priority: 100
    matchers:
      - type: action_type_equals
        value: "ExternalApiCall"
```

## Common patterns

- **Deny scope mismatch**: Use `scope_mismatch` matcher at high priority. Catches missing resource scopes on non-R0 actions.
- **Require approval for R3**: Match `rollback_class_equals` with `R3IrreversibleHighConsequence`. Keep priority high.
- **Quarantine high taint**: Combine `taint_at_least` with `action_is_mutation`. Calibrate the threshold to your data.
- **Draft-only external**: Match `action_type_equals` with `ExternalCommunication`. Decision: `AllowDraftOnly`.
- **Deny mutations**: Match `action_is_mutation` with decision `Deny`. R0 actions bypass this matcher.
- **Scope-restricted access**: Use `scope_mismatch` to enforce declared scopes. R0 actions bypass.
- **External API approval**: Match `action_type_equals` with `ExternalApiCall`. Decision: `RequireApproval`.

## Status caveat

> **production-ready = NO**. Policy validation is implemented locally. Simulation, apply, diff, rollback, and versions require a running server. The matcher set is limited to the types listed above; advanced conditionals (`action_in`, `target_not_in`, `tenant_id`, `rollback_prepared`) are not yet implemented.

## Related docs

- [`concepts.md`](./concepts.md) — Policy decision, risk class, taint scoring.
- [`operator.md`](./operator.md) — How to set active policy bundles.
- [`docs/api/policy-simulation.md`](../api/policy-simulation.md) — Policy simulation API reference.
