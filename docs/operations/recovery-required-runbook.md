# RecoveryRequired Operator Runbook

> **Parent**: [`operations/runbook.md`](./runbook.md) · [`guides/operator.md` § Recovery-required review](../guides/operator.md)
>
> **References**: [ADR-019 — Explicit Ambiguous-Side-Effect Recovery State](../adr/019-ambiguous-side-effect-recovery.md) · [PRODUCTION_NOTES § Recovery-Required Downgrade Safety](../PRODUCTION_NOTES.md)

## What this state means

`RecoveryRequired` is a **non-terminal, owner-review state** for executions and rollback contracts. The gateway detected an ambiguous side effect — a recoverable adapter error, an indeterminate verification outcome, or a stale `Running + Prepared` pair found by the HA reconciler after a crash — and refuses to auto-commit, auto-compensate, or auto-fail. An operator who owns the action lineage must verify the real outcome and choose a terminal path.

- Every transition **into** `RecoveryRequired` carries an `ErrorRaised` provenance obligation; the paired lifecycle outbox record must be reconciled before the state counts as resolved.
- Terminal states are: `Committed`, `Compensated`, `RolledBack`, `Failed` (plus `Denied`, `Quarantined`, `Canceled`).
- HTTP and SQLite mutation adapters are R2-rejected permanently: the gateway never replays or compensates them; they are R3-only (`auto_commit=false`, manual verify and commit).

## 1. List and detect

Alert anchor: `ferrumgate_lifecycle_outbox_operator_review > 0` on `/v1/metrics`.

```bash
# Lifecycle outbox records awaiting operator review (recovery transitions land here)
ferrumctl admin lifecycle-outbox list --status needs_operator_review --limit 50

# Executions: no server-side exec-state filter yet — list and filter locally
ferrumctl admin executions list --limit 200 --format json \
  | jq '.items[] | select(.exec_state == "RecoveryRequired")'

# Detail views
ferrumctl admin lifecycle-outbox get <outbox-id>
ferrumctl admin executions get <execution-id>
```

Review `last_error`, `attempt_count`, `previous_*_state`, `next_*_state`, and `provenance_obligations` on the outbox record before acting.

## 2. Verify outside the gateway

The gateway deliberately does not decide the outcome. Establish the true side-effect state out-of-band before any transition:

1. **Target system first**: check the downstream service, filesystem path, git repo, or SQLite database directly (its own state, logs, or audit trail).
2. **Gateway evidence second**: confirm the `ErrorRaised` provenance event and the lineage chain for the execution:

   ```bash
   curl -fsS -H "Authorization: Bearer $TOKEN" \
     http://127.0.0.1:18080/v1/provenance/lineage/<execution-id>
   ```

3. **Record the evidence** with your decision. The choice of terminal state must be reproducible for incident review.

Do not treat a gateway response (`202 recovery_required=true`) as success or failure — it only means the outcome is ambiguous.

## 3. Transition to a terminal state

Transitions are made through the control API; `ferrumctl` exposes listing/inspection only (no execution transition subcommands today).

| External verdict | Call | Outcome |
|---|---|---|
| Effect is safely undoable by the contract's compensation plan | `POST /v1/executions/{id}/compensate` | `200` → `Compensated` (terminal); `202` → compensation incomplete, pair stays in `RecoveryRequired`, keep reviewing |
| Effect applied and should be kept | `POST /v1/executions/{id}/commit` | R3 explicit-commit boundary: requires the rollback contract in `Verified` and a `SideEffectVerified` provenance event; returns `409` while the pair is still in `RecoveryRequired` |
| Effect unrecoverable | Not exposed via the API yet | Keep the pair in `RecoveryRequired` and escalate |

Runtime notes (v1):

- `compensate` is currently the only endpoint that accepts a `(RecoveryRequired, RecoveryRequired)` pair. On success both the execution and the contract become `Compensated`; on an incomplete recovery they remain in `RecoveryRequired` (the gateway will not silently fall back to `Failed`).
- `commit` cannot be called directly on a recovery pair (`409 Conflict`), and the `verify` endpoint currently accepts only contracts in `ExecutedAwaitingVerify` — re-entering verification from `RecoveryRequired` is not yet exposed. Until that path exists, do **not** edit the store by hand; keep the record in recovery and escalate.
- `RecoveryRequired → Failed` is permitted by the store transition matrix but has no public endpoint today.

Close the outbox record after the underlying data is correct or the state is externally verified:

```bash
# Only after the underlying issue is fixed (e.g., restored provenance parent):
ferrumctl admin lifecycle-outbox retry <outbox-id> \
  --actor-id "<operator-id>" --reason "<fix applied>"

# Only when the state is verified externally and automatic repair must not run again:
ferrumctl admin lifecycle-outbox resolve <outbox-id> \
  --actor-id "<operator-id>" --reason "<external evidence>"
```

Both commands require a non-empty reason and emit an audit trail with the operator actor.

## 4. Provenance closure check

After resolution, confirm the lineage chain is intact and the outbox has drained:

```bash
curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/metrics \
  | grep ferrumgate_lifecycle_outbox
curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18080/v1/readyz/deep
```

Expected: `ferrumgate_lifecycle_outbox_operator_review` returns to `0`, deep readiness no longer reports lifecycle outbox degradation, and the execution's lineage still runs `PolicyEvaluated → CapabilityMinted → … → SideEffectVerified → Terminal`.

## 5. Downgrade ban (do not violate)

Once a store holds `RecoveryRequired` rows, **do not downgrade** to a gateway version that does not understand that state. Older binaries may fail to read or reconcile those rows, leaving ambiguous side effects in an unreadable state.

Before any downgrade, either:

- Resolve every `RecoveryRequired` execution to a terminal state (`Committed`, `Compensated`, `RolledBack`, or `Failed`), **or**
- Migrate the rows to a state the target version understands.

This applies to both SQLite and PostgreSQL stores, and mixed-version clusters sharing a store must share the recovery state machine.
