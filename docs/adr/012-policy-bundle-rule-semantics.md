# ADR 012 — PolicyBundle PDP Rule Semantics

## Status
Proposed (blocked until accepted; `PolicyBundlePdpEngine` implementation cannot proceed without these semantics)

## Context

`StaticPdpEngine` currently encodes a hard-coded, heuristic-based policy evaluation in Rust (`crates/ferrum-pdp/src/engine.rs`). It evaluates proposals through a fixed ordering of imperative checks (scope mismatch, critical risk, taint quarantine, R3 approval, draft-only, outcome clauses). This works for bootstrap deployments but is not configurable by operators and cannot evolve without code changes.

The `PolicyBundle` YAML format (`contracts/policy-bundle.example.yaml`) and `ferrumctl policy` CLI already exist, but there is no runtime engine that compiles bundle rules to a decision graph. The roadmap lists **PolicyBundle PDP engine** as blocked on a rule-semantics ADR.

Before implementing the engine, we must resolve:
1. How rule priority and ordering interact with hard-coded invariants.
2. How conflicting rules (e.g., `Allow` vs `Deny` both matching) are resolved.
3. Whether obligations (side-effect metadata attached to a decision) are part of the rule language.
4. Whether bundle versions are pinned at capability mint time or looked up at execution time.
5. How to migrate from `StaticPdpEngine` heuristics without breaking existing deployments.

## Decision

### 1. Decision algebra (total order)

Define a total ordering over `Decision` values that determines precedence when multiple rules match:

```
Deny > Quarantine > RequireApproval > AllowDraftOnly > Allow
```

- **Deny** is terminal: no lower-precedence decision can override it.
- **Quarantine** is terminal for the execution path (proposals are held), but operators may later resolve to `Allow` or `Deny` via out-of-band disposition.
- **RequireApproval** blocks until approval is resolved; if approved, the effective decision becomes `Allow` (or `AllowDraftOnly` if the rule set also contains a draft-only match at lower priority, though in practice approval resolves to the least restrictive of the matched post-approval rules).
- **AllowDraftOnly** restricts the action to draft/staging adapters.
- **Allow** is the least restrictive; it is the default when no rule matches.

This ordering is consistent with `StaticPdpEngine` behavior: `Deny` (scope mismatch) is evaluated before `RequireApproval` (R3) and `AllowDraftOnly` (draft-only intent).

### 2. Rule ordering and priority

Rules are evaluated in **strict descending priority** (highest numeric `priority` first). Within the same priority, evaluation order is **undefined** (engines may short-circuit or batch-evaluate matchers). Bundle authors must use distinct priorities to guarantee ordering.

A single rule matches when **all** its matchers are true (AND semantics across matchers). There is no OR-within-a-rule; bundle authors add multiple rules with the same decision if they need OR logic.

### 3. First-match-wins with fallback

The engine evaluates rules in priority order and stops at the **first matching rule**. The decision from that rule is the provisional result. The engine then continues scanning **only** to collect `obligations` (see §4) from any lower-priority rules that also match. The first-match decision cannot be overridden by lower-priority rules, but obligations from lower-priority rules are accumulated.

If no rule matches, the result is `Allow` with an empty obligation set. This preserves backward compatibility with the current default-allow scaffold.

### 4. Obligations (open decision)

**Proposed**: Each rule may declare an optional `obligations` list (key-value pairs) that are accumulated into the `EvaluateProposalResponse` when the rule matches. Obligations do not change the decision, but they attach metadata to the capability (e.g., `audit_retention_days=90`, `notify_channel=security`). The capability service stores obligations alongside the capability; the execution adapter may read them.

**Open question**: Should obligations be collected only from matching rules, or from all rules regardless of match? The default proposal is **only from matching rules** to avoid leaking metadata from rules that do not apply.

**Blocked on**: Adding `obligations: Vec<Obligation>` to the proto `EvaluateProposalResponse` and capability record. This schema change is out of scope for this ADR and will be handled in the engine implementation PR.

### 5. Bundle version pinning at capability mint / execution

**Capability mint time**: When the PDP evaluates a proposal and mints a capability, the engine records the **exact bundle version** (`bundle_id + version`) in the capability metadata. This pins the policy semantics that authorized the action.

**Execution time**: The executor may optionally re-evaluate the pinned bundle version against the current proposal context (e.g., if the capability is exercised long after mint). If the re-evaluation under the current active bundle would produce a stricter decision, the executor may emit a warning or audit event, but it **must not** unilaterally deny an already-minted capability unless the capability has been explicitly revoked. This prevents retroactive policy changes from breaking in-flight workflows.

**Migration implication**: `StaticPdpEngine` does not use bundles; its implicit bundle ID is `static-default` with version `0.0.0`. Capabilities minted under the static engine will carry this sentinel version and will not be re-evaluated against bundle rules.

### 6. Conflict resolution

Conflicts arise when two rules at the **same priority** both match and yield different decisions. Because same-priority order is undefined, the engine resolves same-priority conflicts by applying the **total order** from §1: the more restrictive decision wins. This is deterministic and audit-logged.

Bundle authors should avoid same-priority conflicts; the engine may emit a warning (or, in strict mode, reject the bundle) when same-priority conflicts are detected during bundle validation.

### 7. Invariant rules (hard-coded guardrails)

Certain checks (e.g., scope mismatch for non-R0 mutations when no resource scope is declared) are treated as **invariant rules** with priority `u16::MAX` (or an equivalent sentinel). They are conceptually prepended to the bundle and cannot be overridden by operator-authored rules. This preserves the critical invariants from `StaticPdpEngine`:

| Invariant | Priority | Decision | Matcher(s) |
|-----------|----------|----------|------------|
| Scope mismatch | `MAX` | `Deny` | `scope_mismatch` |
| Critical risk + no approval | `MAX-1` | `RequireApproval` | `risk_tier_equals` + `approval_mode_none` |

Operators may add additional rules at lower priority, but they cannot suppress these invariants. If an operator needs to bypass an invariant, they must change the intent or proposal context (e.g., declare a resource scope, set an approval mode), not override the rule.

### 8. Migration path from StaticPdpEngine

1. **Phase 1 (backfill)**: Translate `StaticPdpEngine` heuristics into a canonical `static-default` bundle YAML that produces the same decisions. This bundle is shipped as a built-in fallback.
2. **Phase 2 (dual-run)**: When `PolicyBundlePdpEngine` is configured, it loads the active bundle. If no bundle is active, it falls back to the `static-default` bundle. This allows operators to opt-in without breaking existing deployments.
3. **Phase 3 (deprecation)**: Once bundle-based policies are validated in production, `StaticPdpEngine` is deprecated and eventually removed. The `static-default` bundle remains as a built-in for bootstrap deployments.

The acceptance criteria for the engine implementation PR include an integration test that asserts `PolicyBundlePdpEngine` with the `static-default` bundle produces the same decisions as `StaticPdpEngine` for all existing test cases.

### 9. Matcher extensibility (open decision)

New matchers may be added to the engine without ADR review if they:
- Operate only on the existing evaluation context (intent, proposal, trust context).
- Do not introduce new decision values or change precedence.
- Are documented in `docs/guides/policy-authoring.md`.

Matchers that require new evaluation context fields (e.g., time-of-day, principal attributes) require a separate schema/ADR review.

## Consequences

- **Positive**: Bundle-authored policy becomes deterministic, auditable, and versioned.
- **Positive**: Operators can inspect and simulate policies before activating them.
- **Positive**: Version pinning prevents surprise policy changes from invalidating in-flight capabilities.
- **Negative**: The total-order precedence model is simpler than some industry PDPs (e.g., XACML ` deny-biased` / `permit-biased` combining algorithms). If FerrumGate later needs policy sets with complex combining algorithms, this model may need extension.
- **Negative**: Same-priority conflict detection requires a validation pass over the bundle; this adds complexity to `ferrumctl policy validate`.
- **Negative**: Obligations require a proto/schema change that is not yet implemented.

## Acceptance criteria

1. This ADR is accepted and merged.
2. `PolicyBundlePdpEngine` is implemented with the semantics above:
   - Total-order decision precedence.
   - Descending-priority rule evaluation with first-match-wins.
   - Invariant rules prepended at `MAX`/`MAX-1` priority.
   - Same-priority conflict resolution by stricter decision.
3. Bundle version is recorded in capability metadata at mint time.
4. `static-default` bundle YAML is created and validated to match `StaticPdpEngine` output for all existing tests.
5. Integration tests cover: permit, deny, quarantine, require-approval, draft-only, same-priority conflict, and empty-bundle default-allow.
6. `docs/guides/policy-authoring.md` is updated to document precedence, same-priority behavior, and version pinning.
7. `docs/ROADMAP.md` is updated to move PolicyBundle PDP engine from "Later" to "Next" (or "Implemented" once accepted and implemented).

## Non-goals

- Implementing obligations in this PR (schema change is deferred).
- Changing the proto `Decision` enum (no new values needed).
- Adding time-of-day, geolocation, or principal-attribute matchers.
- Removing `StaticPdpEngine` before the bundle engine is validated.
- XACML or Rego compatibility layers.
- Dynamic rule mutation at runtime (rules are loaded from bundle versions; runtime mutation is out of scope).
