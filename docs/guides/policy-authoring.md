# Policy Authoring Guide

> **Status**: Scaffold. Policy bundles exist; simulation and templates are planned.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Policy schema

Policy bundles are YAML or JSON documents containing a list of rules.

```yaml
name: safe-baseline
version: "1.0.0"
rules:
  - id: allow-read-only
    decision: Allow
    condition:
      action_in: ["fs.read", "git.log", "sqlite.query"]

  - id: require-approval-for-r3
    decision: RequireApproval
    condition:
      risk_class: "R3"

  - id: deny-external-http
    decision: Deny
    condition:
      action: "http.mutation"
      target_not_in: ["https://api.internal.example.com"]

  - id: quarantine-high-taint
    decision: Quarantine
    condition:
      taint_score_gt: 0.8
```

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

## CLI commands (planned)

```bash
# Validate a policy file
ferrumctl policy validate --file my-policy.yaml

# Simulate against a sample intent
ferrumctl policy simulate --file my-policy.yaml --intent sample-intent.json

# Apply as inactive
ferrumctl policy apply --file my-policy.yaml --inactive

# Diff against active
ferrumctl policy diff --bundle-id my-policy

# Rollback to previous
ferrumctl policy rollback --bundle-id my-policy
```

## Templates

### 1. Read-only safe baseline

```yaml
name: read-only-safe
version: "1.0.0"
rules:
  - id: allow-reads
    decision: Allow
    condition:
      action_in: ["fs.read", "git.log", "sqlite.query", "health.check"]
  - id: default-deny
    decision: Deny
    condition: {}
```

### 2. Require approval for R3

```yaml
name: r3-approval-required
version: "1.0.0"
rules:
  - id: r3-approval
    decision: RequireApproval
    condition:
      risk_class: "R3"
  - id: allow-r1-r2
    decision: Allow
    condition:
      risk_class_in: ["R0", "R1", "R2"]
```

### 3. Deny external HTTP except allowlist

```yaml
name: http-allowlist
version: "1.0.0"
rules:
  - id: allow-internal
    decision: Allow
    condition:
      action: "http.mutation"
      target_in: ["https://api.internal.example.com"]
  - id: deny-external
    decision: Deny
    condition:
      action: "http.mutation"
```

### 4. Draft-only email

```yaml
name: draft-only-email
version: "1.0.0"
rules:
  - id: allow-maildraft
    decision: Allow
    condition:
      action_in: ["maildraft.create", "maildraft.update", "maildraft.delete"]
  - id: deny-send
    decision: Deny
    condition:
      action: "mail.send"
```

### 5. Quarantine high taint

```yaml
name: taint-quarantine
version: "1.0.0"
rules:
  - id: quarantine-high
    decision: Quarantine
    condition:
      taint_score_gt: 0.8
  - id: allow-low
    decision: Allow
    condition:
      taint_score_lte: 0.8
```

## Common patterns

- **Allow read-only**: Use `action_in` with read operations.
- **Require approval for R3**: Match `risk_class: "R3"`.
- **Deny out-of-scope fs**: Match `action: "fs.write"` with `target_not_in` safe paths.
- **Quarantine high taint**: Match `taint_score_gt` threshold.
- **Allow draft-only email**: Allow `maildraft.*`, deny `mail.send`.

## Status caveat

> **production-ready = NO**. Policy validation and simulation are planned features. The CLI commands above are the target interface; some may not be fully implemented yet. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 5.

## Related docs

- [`concepts.md`](./concepts.md) — Policy decision, risk class, taint scoring.
- [`operator.md`](./operator.md) — How to set active policy bundles.
- [`docs/ROADMAP.md`](../../ROADMAP.md) — Policy UX gaps and planned features.
