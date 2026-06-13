# Roadmap

Current direction and near-term priorities for FerrumGate.

---

## Status overview

| Area | Direction | Status |
|------|-----------|--------|
| Core governance lifecycle | Stabilize intent → policy → capability → execution → verify → provenance | Stable |
| SQLite performance | Write queue + PRAGMA tuning validated; operator tuning guide available | Stable |
| PostgreSQL support | Runtime and CI live tests passing; HA topology remains operator-owned | Beta |
| MCP integration | stdio tools validated; HTTP/SSE deferred | Experimental |
| Operator experience | ferrumctl, ferrum-tui, Helm chart, monitoring rules, backup/restore drills | Implemented |
| Multi-tenancy | Not on current roadmap | Not implemented |
| Compliance certification | Out of scope for open-source project | Not implemented |

## Stable

Core model implemented, CI-tested, and suitable for local evaluation and controlled pilot work:

- Intent lifecycle, policy evaluation, capability minting, rollback prepare/verify/compensate
- SQLite write queue, provenance chain
- Bearer / scoped / OIDC / agent auth modes

## Implemented

Feature-complete for standard use; local and CI-validated:

- Filesystem, HTTP, Git, SQLite, mail draft adapters
- `ferrumctl` CLI; `ferrum-stress` smoke tests; `ferrum-tui` dashboard
- Prometheus metrics; rate limiting; Helm chart

## Beta

Functional but may require operator tuning or have known caveats:

- **PostgreSQL runtime** — local and CI live-tested. Production HA/multi-node topology is not managed by the repo.

## Experimental

Skeleton or partial implementation; not ready for production use:

- **MCP Streamable HTTP / SSE transport and resumability.**

## Not implemented / out of scope

Single-tenant by design; no roadmap commitment:

- Multi-tenancy
- Managed service / SaaS offering
- Email sending (maildraft manages drafts only)
- Compliance certification (SOC 2, ISO 27001, etc.)

---

See [PRODUCTION_NOTES.md](./PRODUCTION_NOTES.md) for runtime configuration guidance and [CONTRIBUTING.md](../CONTRIBUTING.md) for how to propose changes.
