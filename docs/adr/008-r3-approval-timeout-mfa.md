# ADR 008 — R3 Approval Timeout and Second Factor

## Status
Accepted (TOTP implemented in PR #209; lockout implemented in PR #213; approval timeout auto-deny implemented; WebAuthn and backup codes remain deferred)

## Context

R3 (irreversible / high-risk) actions require explicit operator approval via `approval:resolve`. The current implementation:
- Uses single-factor authentication by default (scoped token only).
- Optional TOTP second-factor confirmation is implemented and opt-in via `approval_mfa_required`.
- Optional approval timeout / auto-deny is implemented and opt-in via `approval_timeout_enabled` and `approval_timeout_seconds`.
- WebAuthn and backup codes remain deferred.
- Provides no out-of-band notification to the approving operator.

This gap was identified in the threat model (B8 — unauthorized approval) and the OWASP LLM06 mapping: single-factor approval and the absence of timeout/escalation are noted limitations.

## Decision

Two independent but complementary controls were added. Approval timeout is implemented; WebAuthn and backup codes remain deferred.

### 1. Approval timeout with auto-deny (implemented)
- `approval_timeout_enabled: bool` (config default `false`) so deployments opt in explicitly and preserve backward compatibility.
- `approval_timeout_seconds: u64` (config default `3600`, validated min `60`, max `86400`).
- Pending approvals older than the timeout are automatically transitioned to `Expired` by a background reconciler.
- The reconciler runs on a configurable interval (`approval_reconciliation_interval_secs`, default `300`).
- Metrics: `ferrumgate_approval_timeouts_total`.
- For each expired approval, the reconciler attempts to append an `ApprovalTimedOut` provenance event. If provenance append fails, the approval is still expired; the failure is logged and observable, but the event is not queued or retried.

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
2. Pending approvals exceeding the timeout are transitioned to `Expired` by the reconciler. The current side effects are: persisting the `Expired` state, incrementing `ferrumgate_approval_timeouts_total`, and attempting to append an `ApprovalTimedOut` provenance event. Provenance append failures are logged and observable; the approval is still expired. In unconfigured or disabled deployments, timeout handling is effectively absent (the feature is opt-in).
3. Timeout transitions increment `ferrumgate_approval_timeouts_total` and attempt to append an `ApprovalTimedOut` provenance event; append failures are logged and observable, and the approval is still expired. CLI (`ferrumctl admin approvals`) reflects the `Expired` state.
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
