# ADR-016: HA Reconciler for Stale In-Flight Executions

## Status

Accepted

## Context

After a gateway crash or restart, executions that were in flight may remain in a
non-terminal state indefinitely. Operators need a bounded, opt-in mechanism that
identifies stale in-flight executions and transitions them to either a safe
terminal state or an explicit operator-review state. The reconciler must not
execute rollback contracts, revoke capabilities, or introduce a leader-
election/lease table. When a side effect may already have started (execution
`Running` and rollback contract `Prepared`), the reconciler must not guess the
outcome; instead it must surface the ambiguous pair for operator review.

## Decision

Add an opt-in HA reconciler with the following design:

- **Trigger**: enabled by `ha_reconciler_enabled` (default `false`). When
disabled the gateway has no behavior change.
- **Schedule**: one startup scan before traffic is accepted, plus a periodic
interval loop (`ha_reconciler_interval_secs`, default 60, range 5..=3600).
- **Staleness**: an execution is stale when `finished_at IS NULL` and
`started_at < now - ha_reconciler_stale_threshold_secs` (default 1800, range
60..=86400).
- **Disposition**: CAS-only transitions via the existing
lifecycle-outbox paired transition path.
  - `Proposed`, `Authorized`, `Prepared`, `AwaitingApproval` → `Canceled`
  - `Running` with a `Prepared` rollback contract → `RecoveryRequired` for both
    the execution and the rollback contract, with an `ErrorRaised` provenance
    obligation (see ADR-019). This preserves evidence and defers terminalization
    to operator review.
  - Other `Running` or `AwaitingVerification` states without a paired `Prepared`
    contract → `Failed` (no side effect was prepared, so failure is safe).
- **Batching**: each pass processes at most `ha_reconciler_batch_size`
executions (default 100, range 1..=10000), ordered by `started_at ASC`.
- **Idempotency**: because each transition uses CAS with the expected current
state, a second pass or concurrent race cannot double-transition an execution
that has already moved to a terminal or recovery state.
- **Provenance**: on a successful transition, emit `ProvenanceEventKind::ErrorRaised`
with metadata `{reconciler:"ha", previous_state, new_state, stale_for_secs}`.
No new event kind is introduced for the Canceled/Failed path; the RecoveryRequired
path carries an `ErrorRaised` obligation in the lifecycle outbox.
- **Metrics**: expose counters `ha_reconciler_canceled_total`,
`ha_reconciler_recovery_required_total`, and `ha_reconciler_errors_total` on `/v1/metrics`.

## Consequences

- Operators can enable crash-recovery cleanup without adding a lease table or
leader election.
- The reconciler does not execute rollback contracts, revoke capabilities, or
modify the transition matrix; it only moves stale in-flight executions to
terminal states or to `RecoveryRequired` for operator review.
- Stale side-effect pairs are recoverable as operator-review records rather than
being silently failed.
- CAS-only updates keep the implementation simple and idempotent across
restarts and concurrent instances.
- Provenance and metrics provide observability for reconciled executions.
