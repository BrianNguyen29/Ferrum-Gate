# FerrumGate v0.1.0-rc.1 Release Notes

> **Status**: RC-ready / conditional single-node SQLite
>
> This is a release candidate for v1 single-node SQLite. No production deployment claim is made.
> Default package version: `0.1.0`. Repository: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials).
> Full production posture requires evaluation against `docs/implementation-path/27-production-evaluation-plan.md`
> and explicit operator signoff per `docs/implementation-path/31-release-paths-todo.md` Path 2 gates.

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

---

## Next Steps

Three mutually exclusive paths are documented in `docs/implementation-path/31-release-paths-todo.md`:

1. **Path 1 — RC Release**: Cut v1 RC tag + publish release notes (this candidate)
2. **Path 2 — Conditional Production Pilot**: Limited production deployment with operator signoff
3. **Path 3 — Phase 3 PostgreSQL**: Begin PostgreSQL implementation per ADR-50

**No path claims full production-ready status.** Path 1 is RC candidate only (now published).
Path 2 is conditional pilot requiring operator signoff. Path 3 requires Phase P1–P4 completion.

---

*Document generated: 2026-04-28. Updated: v0.1.0-rc.1 published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.*
