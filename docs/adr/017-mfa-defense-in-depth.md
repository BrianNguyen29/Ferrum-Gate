# ADR 017 — Per-Agent MFA Lockout (Defense in Depth)

## Status

Accepted (implemented in P1-4A)

## Context

The existing MFA implementation (ADR 008, PR #209) supports TOTP factors per
agent. ADR 013 added break-glass bypass for disabling/rotating active factors.
Factor-level lockout (PR #213) limits brute-force attempts against a single
factor, but an attacker with knowledge of multiple factor IDs for the same
agent could simply switch factors and continue guessing. This creates a
gap: the agent's overall MFA challenge surface is not protected by a single
rate-limiting boundary.

The control plane uses MFA in three places:
- Admin MFA verify/disable/rotate (`/v1/admin/agents/{agent_id}/mfa/*`).
- Approval resolve (`/v1/approvals/{approval_id}/resolve`).
- Quarantine hold resolve (`/v1/quarantines/{hold_id}/resolve`).

All three paths previously performed factor-level lockout only. The goal is to
add a single, agent-scoped lockout layer that spans these paths and future
factor types (backup codes, WebAuthn) without removing factor-level lockout.

## Decision

Implement per-agent MFA lockout in P1-4A. Defer backup-code and WebAuthn
implementations to future work, but design the lockout table to span factor
types.

### 1. New `mfa_agent_lockouts` table

A dedicated table stores agent-level challenge state:

```text
agent_id            TEXT PRIMARY KEY
failed_attempts     INTEGER NOT NULL DEFAULT 0
locked_until        TEXT
last_failed_at      TEXT
lockout_count       INTEGER NOT NULL DEFAULT 0
updated_at          TEXT NOT NULL
```

Timestamps are stored as RFC 3339 text to match the existing `mfa_credentials`
schema. A separate table is used because agent-level lockout state must outlive
individual factors and span factor types.

### 2. Reuse existing lockout configuration

`mfa_lockout_max_attempts` and `mfa_lockout_duration_secs` apply to both
factor-level and agent-level lockout. No new configuration fields are added for
P1-4A.

### 3. Extend `MfaCredentialRepo`

Three methods are added to the existing repository:

- `get_agent_lockout(agent_id) -> Option<MfaAgentLockoutRecord>`
- `record_agent_failed_attempt(agent_id, max_attempts, duration_secs) -> MfaAgentLockoutRecord`
- `reset_agent_lockout(agent_id) -> bool`

The repo is the right home because it already owns MFA credential lifecycle and
lockout state. A separate facade would add an unnecessary seam.

### 4. Atomic lockout semantics

- `record_agent_failed_attempt` increments `failed_attempts` and sets
  `last_failed_at`.
- If the agent is currently locked, the lock is extended and the counter keeps
  incrementing; `lockout_count` does not increase.
- If the lock has expired, the counter is reset to 1 before evaluating the new
  failure, so the agent is not immediately re-locked by stale attempts. A new
  lock only triggers after the threshold is crossed again from the reset state.
- If the new `failed_attempts` reaches `max_attempts`, `locked_until` is set to
  `now + duration_secs` and `lockout_count` is incremented.
- `reset_agent_lockout` clears `failed_attempts`, `locked_until`, and
  `last_failed_at`, preserving `lockout_count`.

### 5. Centralized gateway helper

A shared helper in `crates/ferrum-gateway/src/mfa.rs` enforces the same lockout
logic across admin, approval, and quarantine paths:

- Check agent-level lockout before factor secret decryption/verification.
- Check factor-level lockout.
- Verify the TOTP code.
- On failure: record a failed attempt on both the agent and the factor; return
  `MfaLocked` if either is now locked, otherwise `MfaInvalid`.
- On success: reset both agent and factor lockout counters and return the
  matched TOTP counter.

The caller remains responsible for `record_use(counter)` so the existing CAS
replay protection is preserved unchanged.

### 6. Path-specific behavior

- Admin MFA verify: checks agent lockout for the path `agent_id` before factor
  verification.
- Admin MFA disable/rotate: uses `verify_or_breakglass`. Break-glass bypasses
  TOTP verification entirely, so agent lockout does not block the break-glass
  path. Re-verification with a code uses the shared helper and is subject to
  agent lockout.
- Approval resolve and quarantine resolve: check agent lockout for the actor
  (`request.actor.actor_id`) before fetching the factor record, avoiding factor
  existence leakage while the agent is locked. Factor ownership and status
  checks still apply after the fetch.

### 7. Error response

Agent lockout returns the existing `ApiErrorCode::MfaLocked` with
`retry_after_seconds` in `details`. No new error code is added.

### 8. Out of scope (deferred)

- Backup codes: not implemented in P1-4A; deferred to P1-4B.
- WebAuthn/passkeys: not implemented in P1-4A; remains an ADR boundary only.
- UI/TUI changes.
- Standalone redeem endpoint.
- Broad auth rewrite.
- Removal of factor-level lockout.

## Consequences

- **Positive**: The agent's MFA challenge surface is now protected as a single
  boundary, preventing brute-force hopping between factors.
- **Positive**: Lockout configuration is reused; no new configuration knobs are
  needed for P1-4A.
- **Positive**: A shared gateway helper ensures consistent enforcement across
  admin, approval, and quarantine paths and reduces drift as future factor
  types are added.
- **Positive**: Existing TOTP replay protection (`record_use` CAS) is preserved.
- **Positive**: Break-glass behavior remains unchanged and auditable.
- **Negative**: The lockout table is a new schema entity requiring SQLite and
  PostgreSQL migrations.
- **Negative**: Slightly more complex lockout logic due to the expiry-reset
  semantics, but this is localized in the repo layer.
- **Non-goal**: This does not add backup codes or WebAuthn; the lockout table
  is designed to accommodate those when they are implemented.

## Migration impact

SQLite and PostgreSQL each receive a forward-only migration that adds the
`mfa_agent_lockouts` table. Existing `mfa_credentials` data and factor-level
lockout columns are untouched. The gateway returns `MfaLocked` for agent-level
lockout, which clients should already handle for factor-level lockout.

## Acceptance criteria

1. Agent-level lockout applies across factor IDs for the same agent.
2. Agent lockout is enforced on admin MFA verify, disable (re-verify path),
   rotate (re-verify path), approval resolve, and quarantine resolve.
3. Successful MFA verification resets both agent and factor lockout counters.
4. TOTP code replay is still rejected by the existing `record_use` CAS.
5. Break-glass remains audited and bypasses only where already allowed by the
   current design.
6. SQLite and PostgreSQL migrations and repo behavior are equivalent.
7. OpenAPI/docs describe the agent-level lockout behavior without documenting
   deferred backup-code or WebAuthn endpoints as implemented.
