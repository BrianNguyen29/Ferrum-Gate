# ADR 000 — Adapter Port / Rollback Adapter Seam

## Status
Accepted

## Context

FerrumGate needs to execute side effects against multiple external systems (filesystem, SQLite, Git, HTTP, S3, mail). Each system has different rollback semantics:
- **Filesystem**: revertible via backup/compensate
- **SQLite**: transactional but requires explicit rollback savepoints
- **Git**: revertible via reflog/reset
- **HTTP**: generally not revertible (compensate only)
- **S3**: versioned buckets allow version-level delete for rollback
- **Mail**: draft-only, no send = reversible

We needed a unified seam so the gateway can treat all adapters uniformly while preserving per-adapter rollback capabilities.

## Decision

Introduce `RollbackAdapter` as a trait with four phases:
1. **Prepare** — validate target, capture before-state
2. **Execute** — perform the side effect
3. **Verify** — confirm the side effect completed as expected
4. **Rollback/Compensate** — recover or compensate if the intent needs reversal

Each adapter implements all four phases. The gateway only calls through the uniform trait; adapter-specific logic (e.g., S3 version ID tracking) lives inside the adapter.

## Consequences

- **Positive**: Gateway remains agnostic to adapter internals; new adapters plug in uniformly.
- **Positive**: Rollback is first-class; every adapter must declare whether it supports rollback and how.
- **Negative**: Adapters must implement four methods even when some are no-ops (e.g., HTTP prepare is minimal).
- **Negative**: Cross-adapter transactions are not supported; each adapter rolls back independently.
