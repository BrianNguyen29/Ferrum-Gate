# ADR 013 — MFA Disable/Rotate Break-Glass

## Status

Accepted (implemented in PR #212)

## Context

The MFA TOTP enrollment and verification flow (ADR 008, PR #209) allows operators to add a second factor to agent identities. Once an active factor exists, disabling or rotating it is a high-risk action that could be used to bypass MFA protections. The original disable/rotate handlers accepted empty request bodies and did not require re-verification of the active factor, creating a bypass risk: any caller with the `admin:mfa` route scope could remove or rotate the factor without proving possession of the current secret.

Additionally, operators need an auditable emergency bypass for scenarios where the current TOTP secret is lost or the device is unavailable.

## Decision

Require TOTP re-verification for disable and rotate when an active factor exists, with an auditable break-glass bypass.

### 1. Re-verification path

- `POST /v1/admin/agents/{agent_id}/mfa/disable` and `POST .../rotate` accept a JSON body with optional `code` and optional `reason`.
- When an active factor exists and `code` is present:
  - Decrypt the active factor secret using the configured `mfa_secret_key`.
  - Verify the TOTP code against the secret (±1 step slew).
  - Atomically record the matched counter via CAS (`record_use` in store) to prevent replay.
  - Only then proceed to revoke (disable) or revoke+create (rotate).
- Disable with no active factor remains `404 NotFound`.
- Rotate with no active factor proceeds like enrollment (returns `201 Created`) with audit mode `no_active_factor`; no re-verification is needed.

### 2. Break-glass bypass path

- When an active factor exists and `code` is absent:
  - Require the authenticated actor to have scope `admin:mfa:breakglass` (or the wildcard `*`).
  - Require a non-empty `reason` in the request body.
  - On success, audit with mode `break_glass` and include the `reason`.
  - On missing scope, return `403 MfaRequired`.
  - On missing reason, return `400 ValidationError`.

### 3. AuthActor scope enrichment

- `AuthActor` (PR #211) is extended with `scopes: Vec<String>` and `has_scope(&self, scope: &str) -> bool`.
- Scopes are populated in the middleware for:
  - `Scoped` tokens: `token.scopes`
  - `Agent` auth: `agent.allowed_scopes`
  - `OIDC` auth: `role.default_scopes()` derived from the mapped role

### 4. Audit metadata enrichment

- Disable audit details include:
  - `mode`: `"reverify"` or `"break_glass"`
  - `reverified_factor_id`: the active factor ID (reverify path only)
  - `reason`: break-glass reason (break-glass path)
- Rotate audit details include:
  - `mode`: `"reverify"`, `"break_glass"`, or `"no_active_factor"`
  - `previous_factor_id`: the rotated-out factor ID (when present)
  - `reverified_factor_id`: the active factor ID (reverify path only)
  - `reason`: break-glass reason (break-glass path)

## Consequences

- **Positive**: Active MFA factors can no longer be removed or rotated without proof of possession or explicit break-glass authorization.
- **Positive**: Break-glass is auditable and requires a non-empty operator reason.
- **Positive**: CAS `record_use` prevents TOTP code replay across verify → disable/rotate sequences.
- **Negative**: Slightly more complex handler logic; shared `verify_or_breakglass` helper is extracted to avoid duplication.
- **Non-goal**: This does not add per-factor lockout, failed-attempt counters, or WebAuthn support.

## Migration impact

Disable and rotate endpoints now require a JSON body with a `code` or `reason` when an active factor exists; an empty request body returns `403 MfaRequired` instead of succeeding silently. Clients that previously called these endpoints without a body must be updated to provide re-verification or break-glass parameters.

## Acceptance criteria

1. Disable/rotate with an active factor and a valid `code` succeeds and consumes the TOTP counter via CAS.
2. Disable/rotate with an active factor and no `code` requires `admin:mfa:breakglass` scope and a non-empty `reason`.
3. Disable with no active factor returns `404`.
4. Rotate with no active factor returns `201` and audit mode `no_active_factor`.
5. Replay of the same TOTP code after `record_use` returns `403 MfaInvalid`.
6. Wildcard `*` scope satisfies break-glass scope check.
7. Empty request body (no JSON) is treated as no code / no reason and returns `MfaRequired` when an active factor exists.
8. OpenAPI schemas and docs are updated.

## Non-goals

- Per-factor lockout or failed-attempt counters (requires schema and config changes; deferred).
- WebAuthn or backup codes (future MFA adapter work).
- Broad auth redesign or changes to `required_scope_for_path`.
