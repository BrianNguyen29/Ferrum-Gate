# ADR-016: HA Reconciler for Stale In-Flight Executions

## Status

Accepted

## Context

After a gateway crash or restart, executions that were in flight may remain in a
non-terminal state indefinitely. Operators need an automatic, bounded recovery
mechanism that transitions these stale executions to terminal states without
executing rollback contracts, revoking capabilities, or introducing a leader-
election/lease table. The reconciler must be opt-in and must not change the
existing execution transition matrix.

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
  `ExecutionRepo::compare_and_set_state`.
  - `Proposed`, `Authorized`, `Prepared`, `AwaitingApproval` → `Canceled`
  - `Running`, `AwaitingVerification` → `Failed`
- **Batching**: each pass processes at most `ha_reconciler_batch_size`
  executions (default 100, range 1..=10000), ordered by `started_at ASC`.
- **Idempotency**: because each transition uses CAS with the expected current
  state, a second pass or concurrent race cannot double-transition an execution
  that has already moved to a terminal state.
- **Provenance**: on a successful transition, emit `ProvenanceEventKind::ErrorRaised`
  with metadata `{reconciler:"ha", previous_state, new_state, stale_for_secs}`.
  No new event kind is introduced.
- **Metrics**: expose counters `ha_reconciler_canceled_total`,
  `ha_reconciler_failed_total`, and `ha_reconciler_errors_total` on `/v1/metrics`.

## Consequences

- Operators can enable crash-recovery cleanup without adding a lease table or
  leader election.
- The reconciler does not execute rollback contracts, revoke capabilities, or
  modify the transition matrix; it only moves stale in-flight executions to
  terminal states.
- CAS-only updates keep the implementation simple and idempotent across
  restarts and concurrent instances.
- Provenance and metrics provide observability for reconciled executions.
