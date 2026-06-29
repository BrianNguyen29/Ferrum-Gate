# ADR 008 — R3 Approval Timeout and Second Factor

## Status
Accepted (TOTP implemented in PR #209; approval timeout auto-deny, WebAuthn, backup codes, key rotation, and lockout deferred)

## Context

R3 (irreversible / high-risk) actions require explicit operator approval via `approval:resolve`. The current implementation:
- Uses single-factor authentication (scoped token only).
- Has no timeout on pending approvals; an R3 action can remain in `pending` indefinitely.
- Provides no out-of-band notification to the approving operator.

This gap was identified in the threat model (B8 — unauthorized approval) and the OWASP LLM06 mapping: single-factor approval and the absence of timeout/escalation are noted limitations.

## Decision

Propose two independent but complementary controls, each targeting a **separate follow-up PR** to keep review scope bounded:

### 1. Approval timeout with auto-deny (separate PR)
- Introduce `approval_timeout_seconds: u64` (config default `3600`, max `86400`).
- Pending approvals older than the timeout are automatically rejected with status `timed_out`.
- A background task (or cron-like reconciliation) evaluates pending approvals on a configurable interval (`approval_reconciliation_interval_seconds`, default `300`).
- Metrics: `ferrumgate_approval_timeouts_total`.

### 2. Second-factor confirmation / MFA TOTP (implemented in PR #209)
- Introduce an optional `approval_mfa_required: bool` (config default `false`).
- When enabled, the `POST /v1/approvals/{id}/resolve` endpoint requires a second factor in addition to the scoped token.
- The second factor is **pluggable by design**: a TOTP code, a WebAuthn assertion, or an out-of-band cryptographic acknowledge (e.g., signed JWT from a separate identity provider).
- Phase 1 (completed): document the interface and provide module-level helpers. `approval_mfa_required` is parsed and wired; when enabled the endpoint returns `403 MfaRequired` if the `mfa_factor.code` is missing in the resolve request, or `MfaInvalid` if the TOTP code is wrong. (The earlier no-op trait seam was removed in post-MFA-hardening cleanup; TOTP is now implemented directly via module helpers.)
- Phase 2 (implemented in PR #209): TOTP verification is the first concrete adapter. Admin routes (`/v1/admin/agents/{agent_id}/mfa/*`) support enrollment, verification, disable, rotate, and list. Secrets are AES-256-GCM encrypted at rest.
- Phase 3 (future): operator-provided WebAuthn or IdP integration.

Both controls are opt-in to preserve backward compatibility.

## Consequences

- **Positive**: Reduces the window of exposure for stale pending approvals.
- **Positive**: Moves toward defense-in-depth for high-risk actions.
- **Negative**: Adds operational complexity (MFA enrollment, secret distribution, clock sync for TOTP).
- **Negative**: Background reconciliation requires a runtime task or scheduler; SQLite single-process deployments must handle this without an external cron.
- **Non-goal**: This does not replace the need for RBAC and scoped tokens; it is an additional layer.

## Acceptance criteria

1. Approval timeout config is parsed, validated (min `60`, max `86400`), and applied.
2. Pending approvals exceeding the timeout are transitioned to `timed_out` with an audit entry.
3. Timeout rejections are reflected in the lifecycle outbox and CLI (`ferrumctl admin approvals`).
4. TOTP verification interface is defined and implemented. ✅ Phase 1 & 2
5. TOTP is implemented directly via module helpers. (The earlier no-op trait seam was removed in post-MFA-hardening cleanup.)
6. When `approval_mfa_required=true`, approval resolve returns `403` with `mfa_required` detail if the second factor is missing or invalid, and `MfaInvalid` if the code is wrong. ✅ TOTP implemented
7. Documentation updated: `docs/guides/security-model.md`, `docs/operations/runbook.md`, and `docs/security/threat-model-stride.md`.
8. Integration tests cover timeout and MFA rejection paths (invalid, valid, replay). ✅ Real TOTP tests

## Non-goals

- SMS or email-based MFA (TOTP is the first concrete adapter; others can follow).
- Changing the default approval behavior from synchronous, single-factor (no breaking change without operator opt-in).
- Real-time push notifications to operators (out of scope; can be handled by external alerting).

## Related decisions

- See [ADR 013 — MFA Disable/Rotate Break-Glass](013-mfa-breakglass.md) for TOTP re-verification and break-glass bypass semantics on MFA disable/rotate.
