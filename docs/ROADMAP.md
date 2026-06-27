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
- Store-backed `CapabilityService` for production capability mint/get/revoke/use paths, with in-memory service retained for tests/dev
- Schema-drift checker that refuses startup when the database schema version is newer than the binary-supported version

## Beta

Functional but may require operator tuning or have known caveats:

- **PostgreSQL runtime** — local and CI live-tested. Production HA/multi-node topology is not managed by the repo.

## Experimental

Skeleton or partial implementation; not ready for production use:

- **MCP Streamable HTTP / SSE transport.**

## Next (separate-PR proposals)

These are **deferred to upcoming separate PRs**. They are not implemented and have no committed timeline, but acceptance criteria are defined and they are prioritized over open-ended backlog items.

- **Approval timeout / auto-deny** — Auto-deny stale approvals after a configurable timeout. See ADR 008 (separate PR from MFA).
  - Acceptance: `approval_timeout_seconds` parsed/validated; pending approvals transition to `timed_out`; reflected in lifecycle outbox and CLI.
- **MFA TOTP second factor** — TOTP verification for high-risk approval resolution. See ADR 008 (separate PR from approval timeout).
  - Acceptance: `MfaVerifier` trait with TOTP implementation behind `mfa-totp` feature; `approval_mfa_required` config; resolve endpoint returns `403` with `mfa_required` when factor missing/invalid.
- **Audit verification UX** — Portable `ferrumctl audit export` bundle and local direct-verify mode for operators with filesystem access. See ADR 009.
  - Acceptance: `ferrumctl audit export` produces `.jsonl` + `manifest.json`; `ferrumctl audit verify` checks hash chain and Merkle root.
- **MCP target-host smoke** — Automated smoke tests against a deployed MCP target host (not just local stdio).
  - Acceptance: CI workflow runs stdio + HTTP smoke against a target host; validates tool discovery and a health tool call.

## Later (future / proposed, blocked or needs design)

These require broader design decisions, additional evidence, or an ADR before they can be committed.

- **WORM export** — Write-once-read-many sink integration and portable `ferrumctl audit export` bundle for stronger tamper resistance. Depends on external anchoring design. See ADR 009.
  - Acceptance: `AuditSink` trait with `WormSink` behind feature gate; MinIO Object Lock integration test; background export with retry logic.
- **Behavioral anomaly detection** — Lightweight statistical profiling of actor behavior to flag unusual agency patterns. See ADR 010.
  - Acceptance: `BehavioralProfiler` trait with `ThresholdDetector`; anomaly events written to audit log; Prometheus metric `ferrumgate_behavioral_anomaly_detected_total`.
- **Performance regression gate** — Automated CI gate that blocks changes regressing established baselines. See ADR 011.
  - Acceptance: `make perf-gate` runs short `ferrum-stress` scenarios and compares against baselines; advisory in CI until baselines are authoritative.
- **PolicyBundle PDP engine** — Policy decision point with bundle-scoped rule evaluation. **Blocked** on rule semantics ADR (needed before engine contract can be finalized).
  - Acceptance: Rule semantics ADR accepted; PDP engine compiles bundle rules to a decision graph; integration tests for permit/deny/obligate cases.
- **MCP resumability** — Session resumability. Not implemented; no committed timeline.
  - Acceptance: Resume checkpoint persisted to store; session ID rehydration restores tool context and pending capability state.
- **Production MCP HTTP/SSE** — Production-ready Streamable HTTP / SSE transport. Requires target-host smoke, load, and reconnect evidence first.
  - Acceptance: Load test evidence (≥100 concurrent sessions, 0% errors over 5 min); reconnect test evidence; ADR 005 updated to Accepted.
- **GCS / Azure Blob adapters** — Object-store adapters. Require rollback/compensation contracts and local validation.
  - Acceptance: Adapter implements `AdapterPort` with put/delete/get/copy; versioning-based rollback; local emulator integration tests.
- **HA reconciler** — Background task to reconcile capability and execution state across restarted or failed-over instances.
  - Acceptance: Reconciler scans stale `in_flight` executions and transitions them to `failed` or `compensated` with audit entries; works with PostgreSQL and SQLite.
- **Persistent nonce cache** — Agent auth replay protection uses a bounded in-memory cache. Multi-process or multi-node deployments require a shared persistent cache layer. Not implemented.
- **HA leader election** — Distributed leader election for coordinated multi-node operations beyond per-task reconciliation leases. Not implemented; requires PostgreSQL HA design.
- **Runtime PostgreSQL default-on / packaging** — Enable `postgres` by default or provide a separate binary with PostgreSQL bundled. Requires feature-gate, binary-size, and dependency tradeoff review.
- **Multi-tenancy** — Only if the project pivots to a SaaS offering; requires a dedicated ADR and security review.

## Not implemented / out of scope

Single-tenant by design; no roadmap commitment:

- Multi-tenancy
- Managed service / SaaS offering
- Email sending (maildraft manages drafts only)
- Compliance certification (SOC 2, ISO 27001, etc.)

---

See [PRODUCTION_NOTES.md](./PRODUCTION_NOTES.md) for runtime configuration guidance and [CONTRIBUTING.md](../CONTRIBUTING.md) for how to propose changes.
