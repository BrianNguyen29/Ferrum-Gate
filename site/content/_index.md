+++
title = "FerrumGate"
template = "index.html"
+++

FerrumGate is an **intent-scoped execution gateway** — an audit and rollback layer for agent and operator actions. Before any side effect occurs, declare intent, get policy evaluation, obtain a time-bound capability, and leave a complete provenance trail.

## Status

> **RC-ready / conditional. NOT production-ready.**
> Suitable for local evaluation and single-node pilot only.

- Core gateway (intent → capability → execution → provenance): implemented and locally validated
- Adapter slices (fs, git, http, sqlite, maildraft): implemented; local tests pass
- Policy bundles and evaluation: implemented; 7 templates available
- MCP server integration: implemented and locally validated
- Backup/restore (ferrumctl): implemented; local and target-host drills passed
- Token rotation: target-host validated

## Blockers

- **DOC-1 fresh-user test**: NOT performed
- **Block A (real owned domain/DNS)**: WAIVED/CONDITIONAL for pilot
- **HA / multi-node**: not implemented

## Quick links

- [Quickstart guide](@/guides/quickstart.md) — 10-minute local demo
- [Concepts guide](@/guides/concepts.md) — Intent, proposal, capability, provenance, lineage
- [API guide](@/guides/api.md) — Endpoint reference and lifecycle
- [Operator guide](@/guides/operator.md) — Config, health, backup, incident response
- [Adapter reference](@/guides/adapter-reference.md) — Per-adapter operations and rollback
- [Policy authoring](@/guides/policy-authoring.md) — Policy bundles, templates, patterns
- [MCP integration](@/guides/mcp-integration.md) — Server setup, client config, tools
- [Hosted deployment](@/guides/hosted-deployment.md) — Docker Compose, systemd, PostgreSQL
- [Security model](@/guides/security-model.md) — Bearer auth, scoped tokens, RBAC
- [SLO / SLA](@/guides/slo-sla.md) — Draft targets, validation runbook, metrics
- [Troubleshooting](@/guides/troubleshooting.md) — Common issues and fixes

## Site scaffold note

This is a local-only Zola static-site scaffold. No domain, DNS, or hosting is configured. Build with `make site-build` (requires Zola binary). Serve locally with `make site-serve`.
