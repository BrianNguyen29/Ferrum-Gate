# ADR 007 — Audit Fail-Closed Mode

## Status
Accepted

## Context

The current audit log is append-only and best-effort: store errors during audit append do not fail the primary action. This is a deliberate trade-off to avoid blocking the critical path of governance execution on audit-store health. However, this creates a gap where an audit trail may be silently lost if the store is temporarily unavailable or misconfigured.

For operators with stronger non-repudiation requirements, FerrumGate needs a mode where audit failure is treated as a hard failure, preventing the action from proceeding when the audit trail cannot be recorded.

## Decision

Introduce an optional **audit fail-closed** configuration knob (`audit_fail_closed: bool`, default `false`).

- When `false` (default): current behavior — audit errors are logged but do not block the action.
- When `true`: any audit append failure causes the enclosing request to return `503 Service Unavailable` and the action is not committed. The capability, if already minted, is immediately revoked.

The mode applies to all audit-tracked actions: token lifecycle, policy bundle changes, approval resolution, execution cancel, and capability minting.

## Consequences

- **Positive**: Operators can opt into stronger non-repudiation guarantees when their threat model requires it.
- **Positive**: The default remains safe for availability-first deployments (no breaking change).
- **Negative**: Increases coupling between the audit store and the critical path; store latency directly affects request latency.
- **Negative**: Requires careful operational design (e.g., PostgreSQL with connection pooling, or local SQLite with WAL) to avoid false-positive 503s.
- **Non-goal**: This does not make the audit log tamper-proof or compliance-grade on its own; see ADR 009 for WORM/export considerations.

## Acceptance criteria

1. `audit_fail_closed` is parsed from config and env vars (`FERRUMD_AUDIT_FAIL_CLOSED`).
2. When enabled, audit store errors return `503` and abort the action before side effects are committed.
3. When enabled, a capability minted in the same transaction is revoked if audit append fails.
4. Metrics emitted: `ferrumgate_audit_fail_closed_rejections_total`.
5. Documentation updated: `docs/guides/security-model.md` and `docs/operations/runbook.md`.
6. Integration tests cover both `true` and `false` paths.

## Non-goals

- Cryptographic signing of audit entries (out of scope for this ADR; see ADR 009).
- Automatic audit-store retry with backoff (can be added later without changing the mode semantics).
- Changing the default from `false` to `true` (requires operator migration notice and a separate decision).
