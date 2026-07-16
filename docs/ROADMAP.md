# Roadmap

Current direction and near-term priorities for FerrumGate.

---

## Maturity levels

FerrumGate uses a P0–P5 readiness scale to label every subsystem. **P5 is intentionally not achieved by any current subsystem**; it denotes managed, compliance-grade, turnkey production readiness that this open-source project does not claim.

| Level | Label | Meaning |
|-------|-------|---------|
| P0 | Not implemented | Roadmap item, deferred, or out of scope. No usable code. |
| P1 | Design / spike | Early design, ADR, or prototype. Not for use. |
| P2 | Experimental | Skeleton or partial implementation; functional gaps; not for production. |
| P3 | Beta | Feature-complete for standard use; CI-tested; may require operator tuning or have known caveats. |
| P4 | Stable | Core use cases validated; suitable for local evaluation and controlled pilot work. |
| P5 | Production-ready | Managed HA, compliance certification, and turnkey operation. **Not claimed.** |

### Subsystem readiness matrix

| Subsystem | Level | Status | Caveats / Notes |
|-----------|-------|--------|-----------------|
| Core governance lifecycle | P4 | Stable | Intent → policy → capability → execution → verify → provenance; CI-tested. |
| SQLite store | P4 | Stable | Write queue + PRAGMA tuning; operator tuning guide available. |
| PostgreSQL store | P3 | Beta | Local and CI live-tested; HA topology is operator-owned. |
| Auth (bearer / scoped / OIDC / agent) | P4 | Stable | CI-tested across modes. |
| MCP stdio server | P4 | Stable | Default; tool-contract + stdio lifecycle smoke run in CI (`make adapter-smoke` + non-blocking lifecycle step). |
| MCP HTTP / SSE transport | P2 | Experimental | Streamable HTTP / SSE; in-memory session/replay skeleton; auth-bound; not restart-resumable. |
| MCP resumability | P0 | Not implemented | Replay buffer exists; persistent resume checkpoint is not implemented. |
| Filesystem adapter | P4 | Stable | Sandbox + snapshot rollback. |
| Git adapter | P4 | Stable | Repository-root allowlist + rollback. |
| HTTP adapter | P4 | Stable | rustls client, SSRF guard, bounded timeout, no redirects. |
| SQLite adapter | P4 | Stable | File-backed mutation with database-root allowlist. |
| Mail draft adapter | P4 | Stable | Drafts only; does not send email. |
| S3 adapter | P2 | Experimental | Live put/delete/get/copy; versioning-based rollback; shape-only unit tests in CI (`make adapter-smoke`); live MinIO integration tests gated. |
| GCS adapter | P2 | Experimental | Shape-only put/delete/get; generation-based rollback modeled; shape-only unit tests in CI (`make adapter-smoke`); live SDK path is a declared seam. |
| Azure Blob adapter | P0 | Not implemented | Deferred until GCS semantics are stable. |
| WORM sink | P2 | Experimental | `worm-sink` feature-gated; operator provisions bucket and Object Lock; not a compliance claim. |
| Audit verification UX | P3 | Beta | `ferrumctl audit export`/`verify` and the portable hash-chain/Merkle-root verification bundle are P3; optional S3 Object Lock WORM sink is P2 experimental, best-effort, operator-provisioned, with no external anchoring, compliance, or immutability claim. |

> Adapter maturity levels and promotion criteria are defined in [`guides/adapter-maturity-lifecycle.md`](./guides/adapter-maturity-lifecycle.md). Promoting an adapter requires evidence, not just implementation; current labels remain conservative.

| Behavioral anomaly detection | P2 | Experimental | Phase 1 V1; in-memory advisory high-risk/R3 burst detection; opt-in; does not change PDP decisions. V2/V3 persistence/enforcement/ML are future options only — see ADR 010. |
| PolicyBundle PDP engine | P3 | Beta | Phase 1; `QuarantineHold` enforced; bundle identity propagation and obligations deferred. |
| MFA TOTP | P4 | Stable | Enrollment/verification + per-factor lockout; WebAuthn and backup codes deferred. |
| Approval timeout / auto-deny | P2 | Experimental | Config-gated (`approval_timeout_enabled`); opt-in; background reconciler transitions stale pending approvals to `Expired`; emits metrics and attempts to append an `ApprovalTimedOut` provenance event; append failure is logged/observable; required by production-like config checks. |
| HA reconciler | P2 | Experimental | Opt-in; startup + periodic scan reconciles stale in-flight executions via CAS to Canceled/Failed with provenance/metrics. Does not provide leader election, rollback execution, capability revocation, or turnkey HA. |
| HA leader election | P0 | Not implemented | Backlog; requires PostgreSQL HA and distributed consensus design. |
| Schema drift checker | P4 | Stable | Refuses startup when `_schema_version` is newer than binary-supported version. |
| Operator tooling (`ferrumctl`, `ferrum-tui`, `ferrum-stress`, `ferrum-migrate`) | P4 | Stable | CLI, dashboard, smoke tests, and SQLite→PostgreSQL migration. |
| Helm chart | P2 | Experimental | Local-safe scaffold with monitoring rules; SQLite defaults are single-replica and allowlists are empty. Operators must configure PostgreSQL, secrets, topology, TLS, and HA for production. |
| Perf regression gate | P2 | Experimental / advisory | ADR 011; `make perf-gate` compares `ferrum-stress` results against sample baselines; advisory/non-blocking in regular CI; non-authoritative until promotion prerequisites are met. |
| Coverage gate | P2 | Experimental / advisory | Config-driven thresholds (`coverage-thresholds.toml`) with 7 critical crates and 18 monitor-only workspace members; CI advisory only (`coverage-threshold-soft`); local hard gate available (`make coverage-threshold-hard`). |
| Release automation | P4 | Stable | CI release workflow, cargo-deny, release-profile smoke. Does not imply managed service. |
| External opencode verifier parity | P0 | Out of scope | Remains out-of-product unless tracked separately. |

## Next (separate-PR proposals)

These are **deferred to upcoming separate PRs**. They are not implemented and have no committed timeline, but acceptance criteria are defined and they are prioritized over open-ended backlog items.

- **MCP target-host smoke** — Automated smoke tests against a deployed MCP target host (not just local stdio).
  - Acceptance: CI workflow runs stdio + HTTP smoke against a target host; validates tool discovery and a health tool call.

## Later (future / proposed, blocked or needs design)

These require broader design decisions, additional evidence, or an ADR before they can be committed.

- **WORM hardening follow-ups** — External anchoring evidence, operator runbook, and live Object Lock validation remain future; the feature-gated sink exists and is not a compliance claim.
- **MCP resumability** — Session resumability. Not implemented; no committed timeline.
  - Acceptance: Resume checkpoint persisted to store; session ID rehydration restores tool context and pending capability state.
- **Production MCP HTTP/SSE** — Production-ready Streamable HTTP / SSE transport. Requires target-host smoke, load, and reconnect evidence first.
  - Acceptance: Load test evidence (≥100 concurrent sessions, 0% errors over 5 min); reconnect test evidence; ADR 005 updated to Accepted.
- **Azure Blob adapter** — Object-store adapter. Deferred until GCS adapter semantics are stable.
  - Acceptance: Adapter implements `AdapterPort` with put/delete/get; versioning-based rollback; local emulator integration tests.
- **HA follow-ups** — Leader election and multi-node coordination remain future; the opt-in stale in-flight reconciler exists.
- **Persistent nonce cache** — ✅ Implemented in ADR-015: `NonceCache` seam with `InMemoryNonceCache` (default) and `PostgresNonceCache` (multi-process).
- **HA leader election** — Distributed leader election for coordinated multi-node operations beyond per-task reconciliation leases. Not implemented; requires PostgreSQL HA design.
- **Runtime PostgreSQL default-on / packaging** — Enable `postgres` by default or provide a separate binary with PostgreSQL bundled. Requires feature-gate, binary-size, and dependency tradeoff review.
- **Multi-tenancy** — Only if the project pivots to a SaaS offering; requires a dedicated ADR and security review.
- **Admin audit actor identity** — Authenticated actor identity in admin audit endpoints. Cross-cutting auth context; requires ADR and separate PR.
- **TOTP disable/rotate and break-glass** — Disable or rotate current TOTP factor and break-glass semantics. ✅ Implemented in PR #212.
- **Per-factor lockout** — Fixed-threshold failed-attempt lockout per factor. Schema, config, and API changes implemented in PR #213. Per-agent lockout remains deferred.
- **Stress coverage / server.rs refactor** — Broad stress scenario coverage and `server.rs`/`execution.rs` refactors. Separate quality/refactor PR.

## Not implemented / out of scope

Single-tenant by design; no roadmap commitment:

- Multi-tenancy
- Managed service / SaaS offering
- Email sending (maildraft manages drafts only)
- Compliance certification (SOC 2, ISO 27001, etc.)

---

See [PRODUCTION_NOTES.md](./PRODUCTION_NOTES.md) for runtime configuration guidance and [CONTRIBUTING.md](../CONTRIBUTING.md) for how to propose changes.
