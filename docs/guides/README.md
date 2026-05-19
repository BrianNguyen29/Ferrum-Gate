# FerrumGate Guides

> **Status**: Information architecture index. Guide scaffolds exist; content validation is pending.
> **Owner**: Engineering
> **Parent**: [`docs/ROADMAP.md`](../ROADMAP.md)

---

## Guide index

| Guide | Status | Description |
|-------|--------|-------------|
| [Quickstart](quickstart.md) | Scaffold | FerrumGate in 10 minutes — local demo only |
| [Concepts](concepts.md) | Scaffold | Core concepts: intent, proposal, capability, provenance, lineage |
| [Operator Guide](operator.md) | Scaffold | Configuration, health, backup/restore, token rotation |
| [Policy Authoring](policy-authoring.md) | Scaffold | Policy bundles, validation, simulation, templates |
| [MCP Integration](mcp-integration.md) | Scaffold | MCP server setup, client config, tools list, auth |
| [Hosted Deployment](hosted-deployment.md) | Scaffold | Docker Compose, systemd, PostgreSQL, future Helm |
| [Security Model](security-model.md) | Scaffold | Bearer auth, scoped tokens, RBAC design, tenant model |
| [SLO/SLA](slo-sla.md) | Scaffold | SLO targets, validation runbook, metrics |
| [Adapter Reference](adapter-reference.md) | Scaffold | Per-adapter operations, rollback, limitations |
| [Troubleshooting](troubleshooting.md) | Scaffold | Common issues, diagnostics, recovery steps |

## Non-claims

- **NOT validated**: Guides are scaffolds. Quickstart timing, command accuracy, and end-to-end flows are not yet validated.
- **NOT production-ready**: These guides do not change the production-ready posture of FerrumGate.
- **NOT a marketing site**: These are repository docs for operators and integrators.
- **NOT complete**: Several guides reference planned features (simulation, templates, Helm, RBAC) that are not yet implemented.

## Related docs

- [`docs/production-readiness-v2/07-product-docs-plan.md`](../production-readiness-v2/07-product-docs-plan.md) — Product docs roadmap and acceptance criteria
- [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../production-readiness-v2/00-scope-and-nonclaims.md) — Scope and non-claims
