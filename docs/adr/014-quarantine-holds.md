# ADR-014: Quarantine Holds for Policy-Flagged Proposals

## Status

Accepted

## Context

The FerrumGate policy decision point (PDP) can evaluate a proposal and decide
`Allow`, `Deny`, `RequireApproval`, or `Quarantine`. The first three decisions
have clear, implemented outcomes:

* `Allow` — the caller may mint a capability and execute the action.
* `Deny` — the action is rejected and the intent is terminal.
* `RequireApproval` — an `ApprovalRequest` is created and must be resolved by an
  operator before a capability can be minted.

`Quarantine` was defined as a decision value but had no runtime behavior: a
quarantined proposal was returned to the caller as `Quarantine`, yet nothing
prevented the caller from immediately minting a capability and executing the
action. This left a gap in intent-scoped execution and broke the invariant that
a policy decision must be enforced before side effects occur.

We need a durable, auditable hold mechanism that:

1. Blocks capability mint and execution for quarantined proposals.
2. Allows an operator (or automated process) to review and resolve the hold to
   `Allowed` or `Denied`.
3. Automatically expires holds that are not resolved within a configurable
   timeout, preventing stale holds from blocking the system indefinitely.
4. Emits provenance events for hold creation, resolution, and timeout so the
   full lifecycle is lineage-traceable.

## Decision

Introduce a first-class `QuarantineHold` entity and wire it through the gateway,
store, CLI, and observability layers.

### Entity lifecycle

* **Creation.** When `evaluate_proposal` returns `Decision::Quarantine`, the
  gateway inserts a `QuarantineHold` in state `Pending` and emits a
  `QuarantineHoldCreated` provenance event.
* **Blocking.** `mint_capability` checks the quarantine hold for the proposal.
  Only a hold in state `Allowed` permits minting; `Pending`, `Denied`, and
  `Expired` all block the mint with `403 Forbidden`.
* **Resolution.** `POST /v1/quarantines/{hold_id}/resolve` accepts an actor,
  `allow` flag, optional reason, and optional MFA factor. It atomically updates
  the hold to `Allowed` or `Denied` once and emits a `QuarantineResolved`
  provenance event linked to the creation event.
* **Timeout.** A background `quarantine_timeout_reconciler` periodically expires
  `Pending` holds older than `quarantine_timeout_seconds`, transitions them to
  `Expired`, and emits `QuarantineTimedOut` provenance events.

### Configuration

Three new server configuration fields are added, mirroring the existing approval
timeout settings:

* `quarantine_timeout_enabled` — default `false`.
* `quarantine_timeout_seconds` — default `86400`, range `60..=604800`.
* `quarantine_reconciliation_interval_secs` — default `300`, range `5..=86400`.

They are exposed as CLI flags, TOML config file fields, and environment
variables under the `FERRUMD_` prefix.

### Lineage

The provenance lineage parent spec is extended so:

* `QuarantineHoldCreated` -> parent `PolicyEvaluated`.
* `QuarantineResolved` -> parent `QuarantineHoldCreated`.
* `QuarantineTimedOut` -> parent `QuarantineHoldCreated`.

This preserves the minimum lineage chain invariant and makes every hold
lifecycle transition auditable.

### CLI and API

* `ferrumctl admin quarantines list|get|resolve` commands are added, analogous
  to `admin approvals`.
* The OpenAPI spec gains `/v1/quarantines`, `/v1/quarantines/{hold_id}`, and
  `/v1/quarantines/{hold_id}/resolve` endpoints plus `QuarantineHold`,
  `QuarantineListEnvelope`, and `QuarantineResolveRequest` schemas.

## Consequences

* Quarantined proposals are now genuinely enforced: no capability can be minted
  until the hold is explicitly allowed.
* Operators gain a review-and-release workflow matching the existing approvals
  pattern, with consistent CLI and HTTP surfaces.
* Automatic timeout prevents indefinitely stale holds while still recording the
  timeout in provenance.
* Provenance lineage now covers the full quarantine lifecycle, supporting
  forensic tracing and compliance reporting.
* The implementation intentionally mirrors approvals to keep the cognitive
  footprint low and reuse existing patterns for metrics, reconcilers, and MFA.
