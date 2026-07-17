# ADR-019: Explicit Ambiguous-Side-Effect Recovery State

## Status

Accepted

## Context

FerrumGate v1 executes side effects through adapter-specific rollback contracts. After a side-effect call returns, the gateway runs verify checks and may detect an ambiguous outcome: the adapter reported a recoverable error, the verification checks were inconclusive, or the gateway crashed while the execution was `Running` and the rollback contract was `Prepared`. In these cases the gateway cannot safely auto-commit, auto-compensate, or auto-rollback without operator input, because an incorrect terminalization could worsen consequences or hide evidence.

Prior recovery designs considered allowing automatic R2 compensation (including HTTP replay or compensating SQL statements) for HTTP and SQLite mutation adapters. That approach was rejected because external HTTP effects and out-of-process database mutations are not safely undoable by the gateway; replay is not true undo, and generated compensation SQL may not restore the original state.

## Decision

Introduce a non-terminal `RecoveryRequired` state for both executions and rollback contracts, and apply the following rules:

- **R2 rejection for HTTP/SQLite mutations**: `HttpMutation` and `SqlMutation` actions are classified as `R3IrreversibleHighConsequence`. They require explicit approval or draft mode, `auto_commit=false`, and manual verification/commit through the R3 boundary. R2 compensation/replay is not permitted for these adapters.
- **Owner-only recovery**: An execution or contract in `RecoveryRequired` must be reviewed by the operator who owns the action lineage. The gateway does not auto-commit, auto-compensate, or auto-terminalize ambiguous side effects.
- **Transition paths**: From `RecoveryRequired`, the operator may transition to `Committed`, `Compensated`, `RolledBack`, or `Failed` after external verification. No other automatic transitions are allowed.
- **Provenance requirement**: All transitions into `RecoveryRequired` must carry an `ErrorRaised` provenance obligation/event, and the lifecycle outbox record must be reconciled before the gateway considers the state resolved.
- **HA reconciler integration**: For stale in-flight `Running + Prepared` pairs (where the execution was started but never verified after a crash), the HA reconciler transitions both the execution and the rollback contract to `RecoveryRequired` with `ErrorRaised` instead of failing them. This preserves evidence and awaits operator review.
- **No mixed-version downgrade**: Once a store contains persisted `RecoveryRequired` rows, operators must not downgrade to a gateway version that does not understand the state. Downgrade without migrating those rows to a known terminal state would break lifecycle queries and reconciliation.
- **API visibility**: The public API exposes `RecoveryRequired` in `ExecutionRecord.state`, `RollbackContract.state`, and the `202` responses of `execute`, `verify`, and `compensate` endpoints, with `recovery_required=true` in the response body.

## Consequences

- Operators can see and review ambiguous side effects instead of the gateway silently choosing a terminal path.
- HTTP and SQLite mutation adapters are explicitly R3-only, matching the reality that the gateway cannot safely undo external effects.
- The HA reconciler no longer fails stale side-effect executions; it defers them to operator review.
- Downgrade safety becomes a deployment constraint: any version that writes `RecoveryRequired` rows must be matched by a version that reads them.
- Client contracts must treat `RecoveryRequired` as a non-terminal review state and must not assume success or failure until the operator resolves it.
