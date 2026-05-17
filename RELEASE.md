# FerrumGate Release Notes

> **Latest candidate**: `v0.1.0-rc.2` (prepared, not yet tagged)
>
> This is a release candidate for v1 single-node SQLite. **No production deployment claim is made.**
> Default package version: `0.1.0`. Repository: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials).
> Full production posture requires evaluation against `docs/implementation-path/27-production-evaluation-plan.md`
> and explicit operator signoff per `docs/implementation-path/31-release-paths-todo.md` Path 2 gates.

**Scope**: Single-node governance core with SQLite-backed persistence only
**Phase**: Phase 1 (SQLite write queue; Phase 2 transaction batching deferred/regressed)

---

## v0.1.0-rc.2 Delta

### What's New Since rc.1

- **MCP D1 governance beta preview** — `crates/ferrum-integrations-mcp` local coverage hardened (239 tests); D1 local drill runner available
- **Auth gate** — bearer-token auth enforced when `auth_mode = "bearer"`; dev config remains `auth_mode = "disabled"` for local development
- **Rate limiting** — configurable per-endpoint rate limiting integrated with the gateway
- **Local lifecycle/load smoke** — `bash scripts/run_pre_target_gate.sh --full` passes locally; `bins/ferrum-stress` available for bounded local load validation
- **D78-8 mapping** — delivery-to-milestone traceability updated
- **Architecture/status docs** — `docs/implementation-path/67-production-readiness-roadmap.md` and `122-completion-roadmap-and-hardening-tracker.md` added
- **T3 scaffolds** — Phase 3 PostgreSQL/MCP bridge scaffolds present (no functional Phase 3 claim)
- **Clippy cleanup** — resolved G1 clippy warnings with behavior-neutral cleanup in `ferrum-gateway/src/server.rs` and `ferrum-integrations-mcp/src/lib.rs`

### What Is Still NOT Supported / Blocked

- **NOT production-ready** — operator signoff and Path 2 gates remain required
- **Block A (real domain)**: BLOCKED — no real owned domain or DNS available yet
- **SendGrid API key rotation**: pending / operator-blocked
- **Live target-host MCP smoke/load**: still open; only local validation performed to date
- **PostgreSQL / multi-node / HA**: not implemented; `postgres://` DSN support is scaffold-only

### rc.2 G1 Evidence (Fresh Run)

| Check | Status | Evidence |
|-------|--------|----------|
| `cargo check --workspace` | PASS | Fresh G1 run 2026-05-17 |
| `cargo fmt --all --check` | PASS | Fresh G1 run 2026-05-17 |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Fresh G1 run 2026-05-17 |
| `cargo test --workspace` | PASS (~797 tests) | Fresh G1 run 2026-05-17 |
| `scripts/generate_rc_evidence.py` | PASS | "Overall: ALL PASS" |
| `scripts/validate_repo_layout.sh` | PASS | "Repository layout looks OK" |
| `scripts/check_contract_consistency.py` | PASS | "VALIDATION PASSED" |
| `bash scripts/run_pre_target_gate.sh --full` | PASS | "ALL LOCAL CHECKS PASSED" |
| `git diff --check` | PASS | no trailing whitespace conflicts |

> **Note**: `Cargo.toml` version remains `0.1.0`. No version bump for rc.2.

---

## v0.1.0-rc.1 Release Notes (Historical)

**Candidate name**: `v0.1.0-rc.1`
**Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials)
**Scope**: Single-node governance core with SQLite-backed persistence only
**Phase**: Phase 1 (SQLite write queue; Phase 2 transaction batching deferred/regressed)

---

## RC-Ready Declaration

FerrumGate v1 is **RC-ready** for single-node SQLite-backed deployment.

All P0/P1/P2 items verified complete as of 2026-04-28 (fresh P6 validation):
- **P0**: scope-mismatch deny implemented in PDP (`crates/ferrum-pdp/src/engine.rs:31-46`)
- **P1**: poisoned-context fixtures (6 tests), Phase F docs pack finalized, supported flows documented
- **P2**: clippy passes (`--all-targets`), ~797 workspace tests pass, RC evidence script present and passing

**Production posture is conditional.** Full production-ready is not claimed because operational
constraints remain (see Accepted Risks below). Operators must evaluate against the production
evaluation plan before any production deployment.

---

## What This Release Is NOT

- **NOT** production-ready — production deployment requires operator signoff (Path 2 per `31-release-paths-todo.md`)
- **NOT** multi-node/HA — PostgreSQL is not implemented; Phase 3 is the path to full production scale
- **NOT** Phase 2 — transaction batching was deferred/regressed; Phase 1 write queue is the production target

---

## Supported Scope

### In Scope (v1 Single-Node Support Contract)

- Single-node governance core with SQLite persistence
- REST API for: evaluate, mint capability, authorize, prepare, execute, verify, compensate
- Lineage/provenance endpoint
- Approval workflow with pagination
- Phase 1 write queue (bounded offline `ferrumctl backup` workflow)
- Health/readiness endpoints (shallow; functional probe required for readiness)

### Out of Scope (Post-v1 Backlog)

- PostgreSQL / multi-node / HA / read-replica
- Phase 2 transaction batching + direct UPDATE (deferred/regressed)
- Real adapter implementations (fs permissions/symlinks, git remote ops, http replay, maildraft full implementation)
- Automated backup scheduling / retention policy
- U1–U4 upgrade tracks (outside v1 single-node support contract)

---

## P6 Evidence Summary

| Check | Status | Evidence |
|-------|--------|----------|
| `cargo check --workspace` | PASS | Fresh P6 validation 2026-04-28 |
| `cargo fmt --all --check` | PASS | Fresh P6 validation 2026-04-28 |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Fresh P6 validation 2026-04-28 |
| `cargo test --workspace` | PASS (~797 tests) | Fresh feature-completeness validation 2026-04-28 |
| `scripts/validate_repo_layout.sh` | PASS | "Repository layout looks OK" |
| `scripts/check_contract_consistency.py` | PASS | "VALIDATION PASSED" |
| `scripts/generate_rc_evidence.py` | PASS | "Overall: ALL PASS" |

### Invariant Matrix

| Status | Count |
|--------|-------|
| VERIFIED | 12 (Invariants 1–12) |
| PARTIAL | 0 |
| INFERRED | 0 |

Full matrix: `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`

### Readiness Dimensions

| Dimension | Status |
|-----------|--------|
| P3 (readiness/observability) | PASS — `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep` all return 200 |
| P4 (ADR/DSN guardrails) | PASS — ADR-50 verified; `ferrumd` rejects unsupported DSN types at startup |
| P5 (backup workflow) | PASS — `ferrumctl backup create/verify/restore` implemented; verify runs `PRAGMA integrity_check` |

---

## Accepted Risks

The following risks are acknowledged and documented. Operators must formally accept these before
any production pilot (per `27-production-evaluation-plan.md` Operator Signoff Packet).

| Risk | Description | Reference |
|------|-------------|-----------|
| **SQLite single-node write ceiling** | ~300 writes/s sustained throughput limit; above this requires PostgreSQL (Phase 3) | `27-production-evaluation-plan.md` §1.2 |
| **No PostgreSQL/multi-node/HA** | PostgreSQL not implemented; `postgres://` DSNs rejected at startup; multi-node/HA not in scope | ADR-50; `30-production-roadmap.md` §3 |
| **Phase 2 deferred/regressed** | Transaction batching + direct UPDATE was partially implemented but deferred; Phase 1 write queue is production target | `30-production-roadmap.md` §2 |
| **Bounded backup workflow only** | `ferrumctl backup` is offline workflow only; no automated scheduling, no retention policy, no incremental backup | `27-production-evaluation-plan.md` §3.5 |
| **Compensate may be noop-backed** | `POST /v1/executions/{id}/compensate` may return 200 without performing external undo depending on adapter/rollback class | `27-production-evaluation-plan.md` §3.6 |
| **Health endpoints are shallow** | `/v1/healthz` and `/v1/readyz` confirm server is alive but do not validate store, migrations, or governance loop; functional probe required for readiness | `27-production-evaluation-plan.md` §4.2 |
| **TLS/reverse proxy required** | FerrumGate does not terminate TLS; must be deployed behind a TLS-terminating reverse proxy | `27-production-evaluation-plan.md` §2.1 |

---

## Pre-Tag Checklist (Completed)

> **Note**: G1 gates were re-verified immediately before creating and publishing `v0.1.0-rc.1`. The release is published as a GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.

The following gates were completed before cutting the `v0.1.0-rc.1` git tag. `Cargo.toml` version remains unchanged at `0.1.0`.

| # | Gate Criterion | Status |
|---|----------------|--------|
| G1.1 | `cargo check --workspace` passes | ☑ PASS |
| G1.2 | `cargo fmt --all --check` passes | ☑ PASS |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | ☑ PASS |
| G1.4 | `cargo test --workspace` passes (~797 tests) | ☑ PASS |
| G1.5 | `scripts/generate_rc_evidence.py` passes all five checks | ☑ PASS |
| G1.6 | `scripts/validate_repo_layout.sh` passes | ☑ PASS |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | ☑ PASS |

See `docs/implementation-path/31-release-paths-todo.md` §Path 1 for rollback/abort criteria.

---

## Cross-Reference Index

| Document | Purpose |
|----------|---------|
| `docs/implementation-path/25-EV-v1-single-node-rc-evidence.md` | Canonical RC evidence record |
| `docs/implementation-path/23-production-readiness-assessment.md` | RC-ready declaration with all dimensions verified |
| `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` | 12 VERIFIED / 0 PARTIAL / 0 INFERRED |
| `docs/implementation-path/27-production-evaluation-plan.md` | Production evaluation framework + Operator Signoff Packet |
| `docs/implementation-path/31-release-paths-todo.md` | Release paths: RC tag (Path 1), Production pilot (Path 2), Phase 3 PostgreSQL (Path 3) |
| `docs/implementation-path/44-v1-review-readiness-template.md` | Conservative review readiness template |
| `docs/implementation-path/50-p4-postgres-store-facade-adr.md` | ADR-50: PostgreSQL phased implementation plan |
| `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` | Support contract with accepted risks §4 |
| `docs/PRODUCTION_NOTES.md` | Production notes with stress test evidence |
| `docs/implementation-path/56-adapter-compensation-evidence-matrix.md` | Adapter compensation behavior evidence (post-RC docs-only) |
| `docs/implementation-path/57-workload-compensation-drill-plan.md` | Operator drill plan for compensation verification (post-RC docs-only) |
| `docs/implementation-path/58-workload-compensation-drill-evidence-template.md` | Operator-fillable drill evidence template (post-RC docs-only) |
| `docs/implementation-path/59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence packet for Path 2 pilot (post-RC docs-only) |
| `docs/implementation-path/60-bounded-hardening-examples.md` | Bounded hardening drill examples (post-RC docs/operator-aid) |
| `scripts/check_pilot_readiness.py` | Optional Path 2 readiness/metrics probe prefill helper (post-RC operator-aid) |
| `scripts/generate_evidence_skeleton.py` | Optional command-output-to-markdown evidence skeleton helper (post-RC operator-aid) |
| `scripts/run_d1_d6_drills.py` | Automated D1–D6 local evidence drill runner, bounded adapter-level tests (post-RC operator-aid) |
| `docs/implementation-path/61-path-2-execution-plan.md` | Ordered Path 2 execution checklist before Phase 3 decision (post-RC operator-aid) |
| `configs/examples/*` | Example backup scheduler and TLS reverse proxy configs for operator adaptation |

> **Note (2026-04-29)**: Docs 56–60 and helper scripts are post-RC operator aids. They do not alter the `v0.1.0-rc.1` release tag, do not claim production readiness, and do not complete any G2 gate on behalf of the operator.

---

## Next Steps

Three mutually exclusive paths are documented in `docs/implementation-path/31-release-paths-todo.md`:

1. **Path 1 — RC Release**: Cut v1 RC tag + publish release notes (this candidate)
2. **Path 2 — Conditional Production Pilot**: Limited production deployment with operator signoff
3. **Path 3 — Phase 3 PostgreSQL**: Begin PostgreSQL implementation per ADR-50

**No path claims full production-ready status.** Path 1 is RC candidate only (rc.1 published; rc.2 prepared, not yet tagged).
Path 2 is conditional pilot requiring operator signoff. Path 3 requires Phase P1–P4 completion.

---

*Document generated: 2026-04-28. Updated: v0.1.0-rc.2 prepared 2026-05-17 (not yet tagged). rc.1 remains published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.*
