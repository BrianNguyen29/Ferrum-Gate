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

## Format

Each ADR follows this structure:
- **Context** — What is the issue that we're seeing that is motivating this decision or change?
- **Decision** — What is the change that we're proposing or have agreed to implement?
- **Consequences** — What becomes easier or more difficult to do because of this change?
