# ADR 002 — Lineage Chain Invariant

## Status
Accepted

## Context

Provenance and auditability require that every side effect be traceable back to a policy evaluation, a minted capability, and an approved intent. Without an enforced chain, side effects could be executed without proper authorization.

## Decision

Before any side effect is committed, the system must verify the following minimum lineage chain exists:

```
PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal
```

The terminal state is one of:
- `SideEffectCommitted`
- `SideEffectCompensated`
- `SideEffectRolledBack`

Adapters must emit provenance events at each transition. The store enforces that an execution record cannot be created without a preceding capability mint and policy evaluation.

## Consequences

- **Positive**: Every mutation is auditable end-to-end.
- **Positive**: Compromised or missing steps are caught at execution time, not post-hoc.
- **Negative**: Adds latency (provenance emission is synchronous with the critical path).
- **Negative**: Requires all adapters to emit events in a consistent format; new adapters must implement provenance before they can be enabled.
