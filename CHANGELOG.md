# Changelog

All notable changes for FerrumGate v1 release candidates are documented in [RELEASE.md](./RELEASE.md).

**Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials) | **Default version**: `0.1.0` | **Status**: RC candidate v0.1.0-rc.1 published as GitHub prerelease

## v0.1.0-rc.1 (RC Release Candidate)

**Status**: RC-ready / conditional single-node SQLite

See [RELEASE.md](./RELEASE.md) for full release notes, accepted risks, evidence summary, and pre-tag checklist.

### Unreleased — Post-RC Operator Tooling (2026-04-29)

Added operator evidence/templates and bounded helper scripts (no release tag change):

- `56-adapter-compensation-evidence-matrix.md` — Adapter compensation behavior evidence
- `57-workload-compensation-drill-plan.md` — Operator drill plan for compensation verification
- `58-workload-compensation-drill-evidence-template.md` — Operator-fillable drill evidence template
- `59-pilot-readiness-evidence-packet.md` — G2.1–G2.8 evidence packet for Path 2 pilot
- `60-bounded-hardening-examples.md` — Bounded hardening drill examples
- `scripts/check_pilot_readiness.py` — Optional readiness/metrics probe prefill helper
- `scripts/generate_evidence_skeleton.py` — Optional command-output-to-markdown evidence skeleton helper

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
| RC Evidence | `docs/implementation-path/25-EV-v1-single-node-rc-evidence.md` |
| Production Readiness | `docs/implementation-path/23-production-readiness-assessment.md` |
| Release Paths | `docs/implementation-path/31-release-paths-todo.md` |

### Post-v1 Backlog

- PostgreSQL / multi-node / HA (Phase 3 path per ADR-50)
- Real adapter implementations (fs permissions, symlinks, git remote ops, http replay, maildraft)
- U1–U4 upgrade tracks (outside v1 single-node support contract)

---

*RC candidate v0.1.0-rc.1 published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.*
