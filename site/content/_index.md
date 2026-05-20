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

- [Quickstart guide](https://github.com/ferrumgate/ferrum-gate/blob/main/docs/guides/quickstart.md) — 10-minute local demo
- [Concepts guide](https://github.com/ferrumgate/ferrum-gate/blob/main/docs/guides/concepts.md) — Intent, proposal, capability, provenance, lineage
- [API guide](https://github.com/ferrumgate/ferrum-gate/blob/main/docs/guides/api.md) — Endpoint reference and lifecycle
- [Operator guide](https://github.com/ferrumgate/ferrum-gate/blob/main/docs/guides/operator.md) — Config, health, backup, incident response
- [Adapter reference](https://github.com/ferrumgate/ferrum-gate/blob/main/docs/guides/adapter-reference.md) — Per-adapter operations and rollback

## Site scaffold note

This is a local-only Zola static-site scaffold. No domain, DNS, or hosting is configured. Build with `make site-build` (requires Zola binary). Serve locally with `make site-serve`.
