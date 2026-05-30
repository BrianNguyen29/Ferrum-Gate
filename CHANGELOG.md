# Changelog

All notable changes for FerrumGate v1 release candidates are documented in [RELEASE.md](./RELEASE.md).

**Repository**: `https://github.com/ferrumgate/ferrum-gate` (upstream/original — private, accessible with authorized GitHub credentials) | **Default version**: `0.1.0` | **Status**: RC candidate v0.1.0-rc.2 prepared (not yet tagged)

## v0.1.0-rc.2 (RC Refresh)

**Status**: RC-ready / conditional single-node SQLite — **NOT production-ready**

See [RELEASE.md](./RELEASE.md) for full release notes, accepted risks, evidence summary, and pre-tag checklist.

### Engineering Delta Since rc.1

- **MCP D1 governance beta preview** — `crates/ferrum-integrations-mcp` local coverage hardened (239 tests)
- **Auth gate** — bearer-token auth enforced in production config mode; dev config remains auth-disabled for local development
- **Rate limiting** — configurable rate-limit middleware integrated with gateway
- **Local lifecycle/load smoke** — pre-target gate (`run_pre_target_gate.sh --full`) passes; local stress runner (`bins/ferrum-stress`) available
- **D78-8 mapping** — delivery-to-milestone mapping updated for traceability
- **Architecture/status docs** — production-readiness roadmap and completion tracker added to `docs/PRODUCTION_NOTES.md`
- **T3 scaffolds** — Phase 3 PostgreSQL/MCP bridge scaffolds landed (no functional Phase 3 claim)
- **Minimal clippy cleanup** — resolved G1 clippy warnings with behavior-neutral cleanup in `ferrum-gateway/src/server.rs` and `ferrum-integrations-mcp/src/lib.rs`

### Remaining Blockers (Non-Production Declaration)

- **Block A (real domain)**: WAIVED/CONDITIONAL — no real owned domain or DNS available yet; temporary domain accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure
- **SendGrid API key rotation**: CLOSED — verified on VM, synthetic alert delivered to primary+secondary inboxes, old key revoked
- **Live target-host MCP smoke/load**: local bounded evidence available; target-host validation remains deferred — not production-ready
- **Production deployment**: requires explicit operator signoff per `31-release-paths-todo.md` Path 2 gates

---

## v0.1.0-rc.1 (RC Release Candidate)

**Status**: RC-ready / conditional single-node SQLite

See [RELEASE.md](./RELEASE.md) for full release notes, accepted risks, evidence summary, and pre-tag checklist.

### Unreleased — Post-RC Operator Tooling (2026-04-29)

Added operator evidence/templates and bounded helper scripts (no release tag change):

- Internal evidence docs (56–60) — removed from public tree; see `docs/releases/v0.1.0-rc1.md` for evidence summary
- `scripts/check_pilot_readiness.py` — Optional readiness/metrics probe prefill helper
- `scripts/generate_evidence_skeleton.py` — Optional command-output-to-markdown evidence skeleton helper
- `scripts/run_d1_d6_drills.py` — Automated D1–D6 local evidence drill runner (bounded adapter-level tests, local/test-drill only, operator review required)
- `docs/guides/operator.md` — Operator procedures for Path 2 execution
- `configs/examples/*` — Operator-owned examples for backup scheduling and nginx TLS reverse proxy

> **Note**: These are post-RC operator aids. No G2 gate is completed by these documents/scripts. No production deployment claim is made. RC tag `v0.1.0-rc.1` remains at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.

### Summary of Changes

This RC candidate resolves all P0/P1/P2 items identified during Phase F evidence gate:

- **P0**: Scope-mismatch deny implemented in PDP (`crates/ferrum-pdp/src/engine.rs:31-46`)
- **P1**: Poisoned-context regression fixtures (6 tests), Phase F docs pack finalized, supported flows documented
- **P2**: clippy clean (`cargo clippy --workspace --all-targets -- -D warnings` passes), RC evidence script present and passing, ~797 workspace tests pass

### Evidence Base

| Dimension | Status |
|-----------|--------|
| P6 Validation | PASS — fresh validation 2026-04-28 |
| Invariant Matrix | 12 VERIFIED / 0 PARTIAL / 0 INFERRED |
| RC Evidence | `docs/releases/v0.1.0-rc1.md` |
| Production Readiness | `docs/PRODUCTION_NOTES.md` |
| Release Paths | `docs/PRODUCTION_NOTES.md` |

### Post-v1 Backlog

- PostgreSQL / multi-node / HA (Phase 3 path per ADR-50)
- Real adapter implementations (fs permissions, symlinks, git remote ops, http replay, maildraft)
- U1–U4 upgrade tracks (outside v1 single-node support contract)

---

*RC candidate v0.1.0-rc.1 published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.*
