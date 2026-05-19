# FerrumGate Guides

> **Status**: Information architecture index. Concepts, API, operator, and adapter guides expanded; landing scaffold created.
> **Owner**: Engineering
> **Parent**: [`docs/ROADMAP.md`](../ROADMAP.md)

---

## Guide index

| Guide | Status | Description |
|-------|--------|-------------|
| [Quickstart](quickstart.md) | API/curl flow validated locally | FerrumGate in 10 minutes — local demo only; full API/curl sequence confirmed; ferrumctl and MCP locally validated after bugfix |
| [Concepts](concepts.md) | Expanded | Core concepts: intent, proposal, capability, provenance, lineage, adapters, R0–R3, architecture overview |
| [API](api.md) | Expanded scaffold | Endpoint inventory, auth, errors, execution lifecycle example, rate limiting |
| [Operator Guide](operator.md) | Expanded | Configuration, health, backup/restore, token rotation, monitoring, incident response, local-vs-hosted caveats |
| [Policy Authoring](policy-authoring.md) | Templates added; validation/simulation pending | Policy bundles, 7 examples, common patterns; validation and simulation are planned |
| [MCP Integration](mcp-integration.md) | Locally validated | MCP server setup, client config, tools list, auth; local lifecycle and query_lineage validated after bugfix |
| [Hosted Deployment](hosted-deployment.md) | Scaffold | Docker Compose, systemd, PostgreSQL, future Helm |
| [Security Model](security-model.md) | Scaffold | Bearer auth, scoped tokens, RBAC design, tenant model |
| [SLO/SLA](slo-sla.md) | Scaffold | SLO targets, validation runbook, metrics |
| [Adapter Reference](adapter-reference.md) | Expanded | Per-adapter operations, rollback, limitations, examples, risk classes |
| [Troubleshooting](troubleshooting.md) | Scaffold | Common issues, diagnostics, recovery steps |

## Landing page

A Zola-based static site scaffold is available in `site/`:

- `site/config.toml` — Zola configuration
- `site/templates/` — HTML templates (base + index)
- `site/static/css/main.css` — Stylesheet
- `site/content/_index.md` — Root page content

The landing page includes a prominent status banner, Block A disclaimer, architecture explanation, and links to all guides. It is designed for local build only; no deployment or domain is configured.

## Non-claims

- **Partially validated**: Quickstart API/curl flow validated locally through lineage endpoint (2026-05-19). ferrumctl and MCP locally validated after bugfix (2026-05-19). Quickstart timing is validated for the API/curl path only. Fresh-user test has not been performed.
- **NOT production-ready**: These guides do not change the production-ready posture of FerrumGate.
- **NOT a marketing site**: These are repository docs for operators and integrators.
- **NOT complete**: Several guides reference planned features (simulation, templates, Helm, RBAC) that are not yet implemented.
- **NOT deployed**: The `site/` scaffold is local-only. No cloud, domain, or hosting is configured.

## Related docs

- [`docs/production-readiness-v2/07-product-docs-plan.md`](../production-readiness-v2/07-product-docs-plan.md) — Product docs roadmap and acceptance criteria
- [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../production-readiness-v2/00-scope-and-nonclaims.md) — Scope and non-claims
