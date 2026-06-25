# ADR 001 — Capability TTL + Single-Use Model

## Status
Accepted

## Context

Capabilities in FerrumGate represent time-bounded authority to perform an action. Without TTL and single-use constraints, a capability could be replayed indefinitely or held open for arbitrary durations, violating the intent-scoped execution model.

## Decision

Capabilities enforce two hard constraints:
- **TTL maximum**: 300 seconds from mint time. No capability can be minted with a longer TTL.
- **Single-use**: Each capability is consumed on first successful use. A second attempt with the same capability ID returns `CapabilityAlreadyUsed`.

The `CapabilityService` tracks minted capabilities in memory (with optional persistence) and rejects any use that violates either constraint.

## Consequences

- **Positive**: Limits blast radius of a leaked or stolen capability to 5 minutes and one use.
- **Positive**: Encourages short-lived, intent-scoped workflows rather than long-lived sessions.
- **Negative**: Requires clients to re-mint capabilities frequently for multi-step workflows.
- **Negative**: Clock skew between client and server can cause TTL rejections; we mitigate with a 30-second clock skew allowance.
