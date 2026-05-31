# FerrumGate Docs

> **Repository workspace**: `/home/uong_guyen/work/Ferrum-Gate`.

This is the **single docs directory** used as the foundation for the FerrumGate project.

The goals of this documentation set:
- Provide a complete, consistent, and clear description of the project
- Enable AI agents and other engineers to implement and operate it
- Avoid scattered lookups across many files and specs

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
- `diagrams/` — visual diagrams for architecture, flows, state machines, constraints

## Summary

FerrumGate is an **intent-scoped reversible execution plane** for MCP/tool-using agents.
Every action with a side effect must pass through:
- intent
- policy
- capability
- provenance
- rollback
