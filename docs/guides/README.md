# FerrumGate Guides

> **Owner**: Engineering

---

## Guide index

| Guide | Description |
|-------|-------------|
| [Quickstart](quickstart.md) | FerrumGate in 10 minutes — local demo only; API/curl, ferrumctl, and MCP paths confirmed |
| [Concepts](concepts.md) | Core concepts: intent, proposal, capability, provenance, lineage, adapters, R0–R3, architecture overview |
| [API](api.md) | Endpoint inventory, auth, errors, execution lifecycle example, rate limiting |
| [Operator Guide](operator.md) | Configuration, health, backup/restore, token rotation, monitoring, incident response, local-vs-hosted caveats |
| [Policy Authoring](policy-authoring.md) | Policy bundles, 7 validated examples, common patterns; validate/simulate/apply/diff/rollback/versions implemented |
| [MCP Integration](mcp-integration.md) | MCP server setup, client config, tools list, auth; local lifecycle and query_lineage validated |
| [Hosted Deployment](hosted-deployment.md) | Docker Compose, systemd, PostgreSQL, planned Helm; see deployment feature matrix |
| [Security Model](security-model.md) | Bearer auth, scoped tokens, RBAC design, tenant model |
| [Service Metrics](slo-sla.md) | Observability baselines, validation runbook, metrics |
| [Adapter Reference](adapter-reference.md) | Per-adapter operations, rollback, limitations, examples, risk classes |
| [Troubleshooting](troubleshooting.md) | Common issues, diagnostics, recovery steps |
| [Demo Flows](demo-flows.md) | Six copy-paste demo flows: governed file write, git commit, SQLite mutation, approval-required R3, MCP agent, policy simulation |

## Landing page

A Zola-based static site scaffold is available in `site/`:

- `site/config.toml` — Zola configuration (`base_url` set to local-only `http://127.0.0.1:1111`; no real domain)
- `site/templates/` — HTML templates (base + index)
- `site/static/css/main.css` — Stylesheet
- `site/content/_index.md` — Root page content with summary and quick links

Build the landing page locally with `make site-build` (requires Zola `0.22.1`); configure publishing separately if you host the site.

## Guide scope

These guides are for operators and integrators using FerrumGate. The `site/` landing page can be built locally with `make site-build`.

## Related docs

- [`PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes
