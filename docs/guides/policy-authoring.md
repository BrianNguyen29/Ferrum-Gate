# Policy Authoring Guide

> **Status**: Templates added. 7 copy-pasteable policy examples. Validation and simulation remain planned.
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

> **Caveat**: These are documentation scaffolds. They illustrate common policy patterns using the bundle schema. Runtime schema validation and simulation are planned features; apply templates with care and test in a non-production environment first.

### 1. Read-only safe baseline

**Purpose**: Allow only read and health-check operations; deny everything else by default.

**When to use**: Initial deployment, demo environments, or as a conservative starting point before widening the permission surface.

**Caveats**: Does not allow any mutation (including `maildraft.create`). Add explicit allow rules before enabling write operations. The `default-deny` catch-all must remain last.

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

**Purpose**: Automatically escalate any R3 (high-risk, mutating, irreversible) action to a human approval gate.

**When to use**: Production workloads where R0–R2 operations are trusted but R3 operations (e.g., `fs.write`, `mail.send`, external HTTP mutations) must not execute without sign-off.

**Caveats**: Requires an active approval workflow and operator presence. R3 actions will block until approved or rejected. Do not use if the system must run unattended.

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

**Purpose**: Permit HTTP mutations only to an explicit internal allowlist; block all other external targets.

**When to use**: Environments with strict egress control, or when integrating with a single known internal API. Prevents accidental or malicious calls to untrusted endpoints.

**Caveats**: Update `target_in` whenever the internal API endpoint changes. Wildcards are not supported; each hostname/path must be listed explicitly. HTTPS is strongly recommended for all allowed targets.

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

**Purpose**: Allow composing and editing email drafts while prohibiting actual delivery.

**When to use**: Review or staging environments where you want to preview email content without risking accidental sends to real inboxes.

**Caveats**: `mail.send` is denied entirely. If you need to allow sending to a test inbox, add a separate `target_in` rule above the deny line. This template does not validate draft content.

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

### 5. Tenant-scoped policy

**Purpose**: Restrict all operations to a specific tenant identifier, preventing cross-tenant access.

**When to use**: Multi-tenant deployments where each policy bundle must be bound to exactly one tenant. Combine with per-tenant bundle activation.

**Caveats**: The `tenant_id` field must match the tenant claim in the capability or auth context. This scaffold assumes a single-tenant bundle; multi-tenant wildcard or regex matching is not shown here. Operator must ensure the correct bundle is activated for the correct tenant.

```yaml
name: tenant-scoped-policy
version: "1.0.0"
rules:
  - id: allow-tenant-actions
    decision: Allow
    condition:
      tenant_id: "tenant-alpha"
  - id: deny-other-tenants
    decision: Deny
    condition: {}
```

### 6. Quarantine high taint

**Purpose**: Automatically quarantine actions that exceed a taint-score threshold for additional review.

**When to use**: Workflows with content-scoring or data-classification pipelines where high-risk inputs should not proceed without inspection.

**Caveats**: `taint_score_gt` requires an upstream taint-scoring mechanism. The threshold (0.8) is arbitrary; calibrate it against your data. Quarantined actions remain in a pending state and require manual disposition.

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

### 7. Rollback-required baseline

**Purpose**: Require explicit rollback preparation for every R3 action, ensuring a compensating path exists before execution.

**When to use**: Critical production environments where every high-risk change must be reversible. Pairs with the rollback-by-default invariant documented in [`docs/implementation-path/06-guardrails-and-invariants.md`](../implementation-path/06-guardrails-and-invariants.md).

**Caveats**: This policy assumes the runtime supports a `rollback_prepared` condition check. If the runtime cannot verify rollback readiness, the action may be denied or require approval depending on fallback behavior. Not a substitute for tested backup/restore procedures.

```yaml
name: rollback-required-baseline
version: "1.0.0"
rules:
  - id: allow-r3-with-rollback
    decision: Allow
    condition:
      risk_class: "R3"
      rollback_prepared: true
  - id: require-approval-r3-no-rollback
    decision: RequireApproval
    condition:
      risk_class: "R3"
  - id: allow-lower-risk
    decision: Allow
    condition:
      risk_class_in: ["R0", "R1", "R2"]
```

## Common patterns

- **Allow read-only**: Use `action_in` with read operations; end with a default-deny catch-all.
- **Require approval for R3**: Match `risk_class: "R3"`. Keep an allow rule for R0–R2 above it.
- **Deny out-of-scope fs**: Match `action: "fs.write"` with `target_not_in` safe paths.
- **Quarantine high taint**: Match `taint_score_gt` threshold. Calibrate the threshold to your data.
- **Allow draft-only email**: Allow `maildraft.*`, deny `mail.send`.
- **Tenant-scoped access**: Match `tenant_id` exactly. Activate the correct bundle per tenant.
- **Rollback-required baseline**: Check `rollback_prepared: true` for R3; fall back to `RequireApproval` if not prepared.

## Status caveat

> **production-ready = NO**. Policy validation and simulation are planned features. The CLI commands above are the target interface; some may not be fully implemented yet. See [`docs/ROADMAP.md`](../../ROADMAP.md) §4 Phase 5.

## Related docs

- [`concepts.md`](./concepts.md) — Policy decision, risk class, taint scoring.
- [`operator.md`](./operator.md) — How to set active policy bundles.
- [`docs/ROADMAP.md`](../../ROADMAP.md) — Policy UX gaps and planned features.
