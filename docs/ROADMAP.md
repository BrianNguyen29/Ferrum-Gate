# Roadmap

Current direction and near-term priorities for FerrumGate.

---

## Status overview

| Area | Direction | Status |
|------|-----------|--------|
| Core governance lifecycle | Stabilize intent → policy → capability → execution → verify → provenance | Stable |
| SQLite performance | Write queue + PRAGMA tuning validated; operator tuning guide available | Stable |
| PostgreSQL support | Runtime and CI live tests passing; HA topology remains operator-owned | Beta |
| MCP stdio server | Default, stable; tools validated locally | Stable |
| MCP HTTP/SSE transport | Streamable HTTP / SSE transport; not yet validated | Experimental |
| AWS S3 adapter | Live execution (put/delete/get/copy) with versioning-based rollback; MinIO-gated integration tests; gateway/MCP wired | Implemented (experimental) |
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

- **MCP Streamable HTTP / SSE transport.**

## Next (future priorities, not implemented)

These are planned future directions. They are **not** implemented and have no committed timeline.

- **MCP target-host smoke** — Automated smoke tests against a deployed MCP target host (not just local stdio).
- **MCP resumability** — Session resumability. Not implemented; no committed timeline.
- **Audit verification UX** — Portable `ferrumctl audit export` bundle and local direct-verify mode for operators with filesystem access. See ADR 009.
- **Quickstart split** — Separate "5-minute cheat sheet" (copy-paste commands) from "10-minute walkthrough" (full lifecycle explanation).
- **Audit fail-closed** — Optional mode where audit-store failure blocks the action. See ADR 007.
- **R3 approval timeout / second factor** — Auto-deny stale approvals and optional MFA for high-risk actions. See ADR 008.

## Later (future priorities, not implemented)

These require broader design decisions or additional evidence before they can be committed.

- **WORM export** — Write-once-read-many sink integration and portable `ferrumctl audit export` bundle for stronger tamper resistance. Depends on external anchoring design. See ADR 009.
- **Behavioral anomaly detection** — Lightweight statistical profiling of actor behavior to flag unusual agency patterns. See ADR 010.
- **Performance regression gate** — Automated CI gate that blocks changes regressing established baselines. See ADR 011.
- **Runtime PostgreSQL default-on / packaging** — Enable `postgres` by default or provide a separate binary with PostgreSQL bundled. Requires feature-gate, binary-size, and dependency tradeoff review.
- **GCS / Azure Blob adapters** — Object-store adapters. Require rollback/ compensation contracts and local validation.
- **Multi-tenancy** — Only if the project pivots to a SaaS offering; requires a dedicated ADR and security review.
- **Production MCP HTTP/SSE** — After target-host smoke, load, and reconnect evidence exists.

## Not implemented / out of scope

Single-tenant by design; no roadmap commitment:

- Multi-tenancy
- Managed service / SaaS offering
- Email sending (maildraft manages drafts only)
- Compliance certification (SOC 2, ISO 27001, etc.)

---

See [PRODUCTION_NOTES.md](./PRODUCTION_NOTES.md) for runtime configuration guidance and [CONTRIBUTING.md](../CONTRIBUTING.md) for how to propose changes.
