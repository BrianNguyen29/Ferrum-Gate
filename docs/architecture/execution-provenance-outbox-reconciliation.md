# Execution/Provenance Outbox and Reconciliation

Status: proposed implementation contract.

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

## Store Contract

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

1. Add SQLite and PostgreSQL `lifecycle_outbox` tables with a unique idempotency key.
2. Add `LifecycleOutboxRepo` to `StoreFacade`.
3. Convert prepare/execute/verify/commit/compensate/cancel handlers one domain at a
   time to use atomic state+outbox writes.
4. Add `ferrumd reconcile lifecycle --dry-run` for operator inspection.
5. Add crash-injection tests:
   - state committed, provenance missing;
   - provenance event present, edge missing;
   - terminal compensated/rolledback with recovered=false;
   - repeated reconciler run idempotency.
