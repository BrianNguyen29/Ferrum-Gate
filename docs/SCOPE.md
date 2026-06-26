# Project Scope

This document defines what FerrumGate is, what it is not, and where the boundaries lie between the project and operator responsibilities.

## In scope

- **Single-tenant, self-hosted governance gateway** for AI agents and tool-using systems.
- **Intent-scoped execution** with policy evaluation, capability minting, rollback prepare/verify/compensate, and provenance chain.
- **Local pilot and controlled evaluation** with SQLite or PostgreSQL.
- **MCP stdio server** as the default, stable integration surface.
- **Bounded adapters** for filesystem, HTTP, Git, SQLite, mail draft, and S3 (experimental) side effects.
- **Operator tooling**: `ferrumctl`, `ferrum-tui`, `ferrum-stress`, `ferrum-migrate`.

## Operator-owned

- **Deployment topology**: VMs, containers, Kubernetes, networking.
- **TLS termination**: Reverse proxy certificates, rotation, and trust chains.
- **Secrets management**: Bearer tokens, OIDC client credentials, PostgreSQL credentials.
- **Database HA**: PostgreSQL replication, failover, and backup retention.
- **Production acceptance testing**: Performance, security, and compliance validation in your environment.
- **Monitoring and alerting**: Prometheus, AlertManager, log aggregation.

## Out of scope / not implemented

| Item | Status | Note |
|------|--------|------|
| Multi-tenancy | Not implemented | Single-tenant by design |
| Managed SaaS / hosted service | Not implemented | No roadmap commitment |
| Email sending | Not implemented | Mail draft adapter manages drafts only |
| Compliance certification (SOC 2, ISO 27001, etc.) | Out of scope | Open-source project; operator must certify their own deployment |
| MCP Streamable HTTP / SSE transport | Experimental | Not production-ready; requires `--features http` |
| MCP resumability | Not implemented | Future priority; no committed timeline |
| Turnkey HA product | Not implemented | Operator must design HA topology |

## Honest assessment

FerrumGate is not a turnkey HA product or a compliance-certified platform. It is a governance engine that operators integrate into their own infrastructure. All production readiness decisions—topology, TLS, secrets, backups, database HA, and acceptance testing—remain operator responsibilities.

## Related docs

- [`ROADMAP.md`](./ROADMAP.md) — Feature direction and timeline
- [`PRODUCTION_NOTES.md`](./PRODUCTION_NOTES.md) — Runtime configuration and stress baselines
- [`guides/operator.md`](./guides/operator.md) — Day-to-day operations
- [`operations/runbook.md`](./operations/runbook.md) — Incident response
