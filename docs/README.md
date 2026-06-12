# FerrumGate Docs

This is the documentation hub for FerrumGate, an intent-scoped reversible execution gateway for AI agents and tool-using systems.

The goals of this documentation set:
- Provide a complete, consistent, and clear description of the project
- Enable AI agents and other engineers to implement and operate it
- Avoid scattered lookups across many files and specs

## Product READMEs

- [`../README.md`](../README.md) — English product overview and onboarding path
- [`../README.vi.md`](../README.vi.md) — Vietnamese product overview and onboarding path

## How to use

If you have time to read only a few documents, read them in this order:

1. `guides/README.md`
2. `guides/concepts.md`
3. `guides/quickstart.md`
4. `guides/operator.md`
5. `guides/security-model.md`
6. `PRODUCTION_NOTES.md`

## Subdirectories

- `guides/` — usage, operations, and deployment guides
- `architecture/` — architecture documents
- `security/` — security model
- `api/` — API documentation
- `mcp/` — MCP documentation
- `operations/` — operations guides
- `operator/` — operator guides
- `diagrams/` — Mermaid source diagrams for architecture, execution lifecycle, lineage chain, and deployment topology

## Summary

FerrumGate is an **intent-scoped reversible execution plane** for MCP/tool-using agents.
Every action with a side effect must pass through:
- intent
- policy
- capability
- provenance
- rollback
