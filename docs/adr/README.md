# Architecture Decision Records (ADRs)

This directory records significant architecture decisions made in the FerrumGate project.

| ADR | Title | Status |
|-----|-------|--------|
| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |
| [001](001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |
| [002](002-lineage-chain.md) | Lineage Chain Invariant | Accepted |
| [003](003-store-split.md) | SQLite/PostgreSQL Store Split | Accepted |
| [004](004-s3-feature-gate.md) | S3 Feature Gate and Live Mode Semantics | Accepted |
| [005](005-mcp-transport-maturity.md) | MCP Transport Maturity Boundary | Accepted |
| [006](006-archive-ledger-deferred-runtime.md) | Archive ferrum-ledger and Deferred Runtime Items | Accepted |
| [007](007-audit-fail-closed.md) | Audit Fail-Closed Mode | Accepted |
| [008](008-r3-approval-timeout-mfa.md) | R3 Approval Timeout and Second Factor | Proposed (split into separate PRs: approval timeout; MFA TOTP) |
| [009](009-worm-export-audit-bundle.md) | WORM Export and Portable Audit Bundle | Proposed (audit verification UX deferred to separate PR; WORM sink backlog) |
| [010](010-behavioral-anomaly-detection.md) | Behavioral Anomaly Detection | Accepted (Phase 1 V1 implemented) |
| [011](011-performance-regression-gate.md) | Performance Regression Gate | Accepted (advisory / non-blocking in regular CI) |
| [012](012-policy-bundle-rule-semantics.md) | PolicyBundle PDP Rule Semantics | Proposed |
| [013](013-mfa-breakglass.md) | MFA Disable/Rotate Break-Glass | Accepted |
| [014](014-quarantine-holds.md) | Quarantine Holds for Policy-Flagged Proposals | Accepted |
| [015](015-shared-nonce-cache.md) | Shared Nonce Cache for Agent Auth Replay Protection | Accepted |
| [016](016-ha-reconciler.md) | HA Reconciler for Stale In-Flight Executions | Accepted |
| [017](017-mfa-defense-in-depth.md) | Per-Agent MFA Lockout (Defense in Depth) | Accepted |

## Format

Each ADR follows this structure:
- **Context** — What is the issue that we're seeing that is motivating this decision or change?
- **Decision** — What is the change that we're proposing or have agreed to implement?
- **Consequences** — What becomes easier or more difficult to do because of this change?
