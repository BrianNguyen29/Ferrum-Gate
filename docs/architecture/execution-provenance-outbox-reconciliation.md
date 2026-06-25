# Execution/Provenance Outbox and Reconciliation

Status: **partially implemented**. The store-level `LifecycleOutboxRepo` table, basic
operator review commands (`ferrumctl admin lifecycle-outbox`), startup reconciler, and
periodic background reconciler worker exist. Crash-injection tests and readiness
degradation for pending drift remain **deferred**.


## Problem

Execution state and provenance are written through separate repository calls in the
gateway lifecycle. Event+edge append is already transactional inside the provenance
repository, but a crash can still leave drift between:

- execution/rollback state transitions,
- terminal provenance events,
- parent provenance edges,
- audit rows.

The runtime must detect and repair this drift without allowing side effects to
advance silently.

## Execution State Transition Matrix (Store Seam)

> **Status:** Implemented in `crates/ferrum-store/src/transitions.rs`.

The store enforces a strict execution state transition matrix at the `is_valid_execution_transition`
seam. Self-transitions are allowed only for idempotent non-terminal states:
Authorized, Prepared, Running, AwaitingVerification.

| From               | To (valid)                                                  |
|--------------------|-------------------------------------------------------------|
| Proposed           | Authorized, Running, Canceled                               |
| Authorized         | Running, Canceled, Authorized (self)                        |
| Prepared           | Running, Canceled, Prepared (self)                          |
| Running            | Committed, Failed, Compensated, Running (self)              |
| AwaitingVerification | Committed, Failed, Compensated, AwaitingVerification (self) |
| AwaitingApproval   | Canceled                                                    |
| Terminal           | none                                                        |

Terminal states: Committed, Compensated, RolledBack, Denied, Quarantined, Failed, Canceled.

This matrix is behavior-preserving for all current handler sites (authorize, prepare,
execute, verify, commit, compensate, cancel). Invalid transitions are rejected at the
store seam before the lifecycle outbox write is accepted. Handler-level guards (e.g.,
`compare_and_set_state`, `execution_is_cancelable_pre_side_effect`) remain in place
as a second line of defense.

## Store Contract

> **Status:** Implemented. SQLite and PostgreSQL `lifecycle_outbox` tables, repo trait,
> and fencing/lease logic are in production. The startup reconciler runs before HTTP binding;
> a periodic background reconciler is available via config and disabled by default.

Add a store-level `LifecycleOutboxRepo` with the following operations:

- `enqueue_lifecycle_transition(record)`: inserted in the same database transaction
  as the execution and rollback-contract state update.
- `mark_provenance_written(outbox_id, event_id)`: records the terminal provenance
  event written for the transition.
- `mark_reconciled(outbox_id, result)`: closes the outbox item after validation or
  repair.
- `claim_pending_reconciliation(limit, lease_owner, lease_ttl)`: atomically claims
  pending/provenance-written records for one reconciler worker. PostgreSQL uses
  row locks with `FOR UPDATE SKIP LOCKED`; SQLite claims inside a write
  transaction. A record with an unexpired lease must not be returned to another
  owner.
- Every successful claim increments `reconciliation_lease_generation`. Background
  mutations use `(outbox_id, lease_owner, generation)` as a fencing token. A
  worker holding an older generation cannot mark provenance written, reconcile,
  or move a record to operator review.
- `renew_reconciliation_lease(lease, ttl)` extends only the currently fenced
  generation. The default lease TTL is 120 seconds, while one record is bounded
  by a 30-second timeout and renewed every 10 seconds.
- `list_pending_reconciliation(limit)`: returns records whose state write exists
  but provenance/audit verification has not completed. This is observational
  only; mutating reconcilers must use `claim_pending_reconciliation`.

Every record stores `execution_id`, optional `rollback_contract_id`, previous and
new execution state, previous and new rollback state, intended provenance kind,
idempotency key, created timestamp, attempt count, and last error.
The store also owns reconciliation lease metadata (`lease_owner`, `lease_expires`)
outside the public proto record so multi-node workers cannot process the same
outbox item concurrently. PostgreSQL stores expiry as `TIMESTAMPTZ`; SQLite stores
canonical RFC 3339 text.

## Reconciliation Rules

> **Status:** Implemented. `reconcile_lifecycle_outbox` runs at startup before HTTP binding.
> A periodic background reconciler is enabled via `lifecycle_reconciliation_enabled` (default false),
> with configurable interval and batch limit. It coordinates with the HTTP server graceful shutdown
> using a `tokio::sync::Notify` signal. Readiness degradation for pending drift remains deferred.

The reconciler is fail-closed:

- If an execution is terminal but the matching terminal provenance event is absent,
  emit the missing event with `metadata.reconciled=true`.
- If an event exists without the required parent edge, append only the missing edge
  when the parent event is unambiguous.
- If parent causality is ambiguous, mark the outbox item `needs_operator_review` and
  do not advance execution state.
- `needs_operator_review` records are not automatically claimed again. An operator
  must resolve them or reset them for retry after correcting the underlying data.
- A record-level error does not abort the batch. The store records `last_error`,
  increments `attempt_count`, releases the lease, and retries later. After three
  failed attempts, the record moves to `needs_operator_review`.
- If execution state and rollback-contract state disagree, prefer the stricter
  unrecovered state (`Failed`/`RecoveryIncomplete`) and emit `ErrorRaised`.

## Gateway Contract

> **Status:** Partially implemented. Handlers use atomic state+outbox writes for
> prepare, execute, verify, commit, compensate, and cancel. Readiness degradation
> and metrics integration for pending drift are deferred.

Gateway lifecycle handlers should not directly treat a state write as complete.
They should:

1. persist state transition and outbox item atomically;
2. emit provenance event+edges transactionally;
3. mark outbox provenance written;
4. return success only after the required provenance write succeeds.

On restart, a reconciler claims pending records before HTTP startup. A periodic
background reconciler continues claiming bounded batches after startup. Readiness
is degraded while pending drift or expired leases exist. Metrics expose active
leases, expired leases, record failures, fencing conflicts, repairs, operator
reviews, and the duration of the most recent batch.

## Migration Plan

> **Status:** Partially implemented. Steps 1-4 are complete. Crash-injection tests are deferred.

1. ✅ Add SQLite and PostgreSQL `lifecycle_outbox` tables with a unique idempotency key.
2. ✅ Add `LifecycleOutboxRepo` to `StoreFacade`.
3. ✅ Convert prepare/execute/verify/commit/compensate/cancel handlers to use atomic state+outbox writes.
4. ✅ Add bounded periodic background reconciler worker with graceful shutdown.
5. Add crash-injection tests:
   - state committed, provenance missing;
   - provenance event present, edge missing;
   - terminal compensated/rolledback with recovered=false;
   - repeated reconciler run idempotency.

## Deferred

### Periodic background reconciler
Implemented via `lifecycle_reconciliation_enabled` (default `false`),
`lifecycle_reconciliation_interval_secs` (default `60`), and
`lifecycle_reconciliation_batch_limit` (default `1000`). The worker spawns after
startup pre-reconcile and runs bounded `reconcile_lifecycle_outbox` batches on a
`tokio::time::interval`. It shuts down gracefully via `tokio::sync::Notify` when
the HTTP server receives a shutdown signal, with a 5-second timeout for the current
batch to complete.

### Readiness degradation for pending drift
Deep readiness (`/v1/readyz/deep`) does not yet inspect `lifecycle_outbox` pending
or expired-lease counts. This is deferred until the periodic reconciler is validated
in production.

### Crash-injection tests
Automated crash-injection tests that kill the process mid-transition and assert
repair on restart are not yet implemented. They are the final validation gate for
the outbox+reconciliation pipeline.
