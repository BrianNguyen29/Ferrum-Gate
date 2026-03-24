# 15 — Deployment and operations

## Development
- single process
- sqlite local
- memory ledger chấp nhận được

## Staging / production-like
- persistent store
- provenance bật
- rollback bật
- strict manifest pinning nên bật
- logs không lộ secrets

## Operations checklist
- policy bundle đúng environment
- rollback không bị tắt
- sanitize/DLP bật
- TTL hợp lý
- lineage query usable

## Pending approvals

R3 (IrreversibleHighConsequence) executions require explicit approval before the capability is consumed. While a capability is awaiting approval it is NOT consumed — the execution remains in AwaitingApproval state.

**Discover pending approvals:**
```
GET /v1/approvals[?limit=N][&cursor=CURSOR][&proposal_id=UUID][&execution_id=UUID]
```
Returns a response envelope:

- `items`: pending approvals, most recent first
- `next_cursor`: cursor for the next page, or `null` when there is no next page

Cursor pagination is preferred for operator workflows because it stays stable while the pending set changes.

`offset` pagination is deprecated and retained only as a temporary compatibility path while operators and clients move to cursor-based paging.

- `limit` defaults to 50, maximum 100
- `cursor` selects the next page
- `proposal_id` narrows the list to a single proposal
- `execution_id` narrows the list to approvals linked to a specific execution

Deprecated offset mode is retained temporarily for compatibility:

```
GET /v1/approvals[?limit=N][&offset=M][&proposal_id=UUID][&execution_id=UUID]
```

When using offset mode, the endpoint still returns the same envelope shape, with `next_cursor: null`. New integrations should use `cursor`, and existing offset-based consumers should plan to migrate before offset support is removed.

Filter by proposal_id: when `proposal_id` is provided, returns only pending approvals for that specific proposal.

Filter by execution_id: when `execution_id` is provided, returns only pending approvals linked to this execution.

Combined filters: when both `proposal_id` and `execution_id` are provided, both filters apply (AND semantics).

**Act on a pending approval:**
```
POST /v1/approvals/{approval_id}/resolve
{"actor": {...}, "approve": true, "reason": "..."}
```
Granting (approve=true) consumes the capability and advances the execution to Prepared. Denying (approve=false) leaves the execution in AwaitingApproval and does NOT consume the capability.

Pending approvals expire after 15 minutes (expires_at). Expired approvals must be re-created by re-authorizing the execution.
