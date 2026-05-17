# 53 — RC Tag Checklist

> **Status**: Path 1 complete. v0.1.0-rc.1 published as GitHub prerelease. v0.1.0-rc.2 prepared, not yet tagged.
> **Purpose**: Standalone fillable pre-tag checklist for FerrumGate v1 RC release (retained for reference).
> **Scope**: Single-node SQLite only. No production claim.
> **Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials) | **Default package version**: `0.1.0` | **RC status**: v0.1.0-rc.2 prepared (not yet tagged)

---

## Latest RC Prep Verification Observed (rc.2 — 2026-05-17)

Recorded from fresh G1 gate run (2026-05-17). This table captures observed pass state for the rc.2 refresh.

| # | Gate Criterion | Observed Result | Evidence |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | **PASS** | exit 0 |
| G1.2 | `cargo fmt --all --check` passes | **PASS** | exit 0 |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | **PASS** | exit 0 |
| G1.4 | `cargo test --workspace` passes | **PASS** (~797 tests) | ferrum-gateway 41, integration_gateway_flow 65, ferrum-adapter-maildraft 16, doctests 0 failures |
| G1.5 | `scripts/generate_rc_evidence.py` passes all checks | **PASS** | "Overall: ALL PASS" |
| G1.6 | `bash scripts/validate_repo_layout.sh` passes | **PASS** | "Repository layout looks OK" |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | **PASS** | "VALIDATION PASSED" |
| G1.8 | `bash scripts/run_pre_target_gate.sh --full` passes | **PASS** | "ALL LOCAL CHECKS PASSED" |
| G1.9 | `git diff --check` passes | **PASS** | no trailing whitespace conflicts |

Full G1 chain (rc.2): `cargo check --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && python3 scripts/generate_rc_evidence.py && bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py && bash scripts/run_pre_target_gate.sh --full && git diff --check` — ALL PASS.

---

## Historical — rc.1 RC Prep Verification Observed

Recorded from fresh G1 gate run (2026-04-28). This table captures observed pass state — it does not check off the fillable pre-tag checklist, which remains for the release engineer to verify before tagging.

| # | Gate Criterion | Observed Result | Evidence |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | **PASS** | exit 0 |
| G1.2 | `cargo fmt --all -- --check` passes | **PASS** | exit 0 |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | **PASS** | exit 0 |
| G1.4 | `cargo test --workspace` passes | **PASS** (~797 tests) | ferrum-gateway 44, integration_gateway_flow 65, ferrum-adapter-maildraft 16, doctests 0 failures |
| G1.5 | `scripts/generate_rc_evidence.py` passes all checks | **PASS** | "Overall: ALL PASS" |
| G1.6 | `bash scripts/validate_repo_layout.sh` passes | **PASS** | "Repository layout looks OK" |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | **PASS** | "VALIDATION PASSED" |

Full G1 chain: `cargo check --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && python3 scripts/generate_rc_evidence.py && bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py` — ALL PASS.

---

## Pre-Tag Gate Checklist (Historical — Path 1 Complete)

The following table records the verified pass state at time of tagging (2026-04-28). Tagging is now complete; this checklist is retained for reference.

| # | Gate Criterion | Evidence Reference | Verified |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | Fresh P6 validation (2026-04-28) | ☑ PASS |
| G1.2 | `cargo fmt --all -- --check` passes | Fresh P6 validation | ☑ PASS |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | Fresh P6 validation | ☑ PASS |
| G1.4 | `cargo test --workspace` passes (~797 tests) | Fresh feature-completeness validation | ☑ PASS |
| G1.5 | `scripts/generate_rc_evidence.py` passes all five checks | `docs/artifacts/2026-03-30/05-contract-consistency.txt` or fresh run | ☑ PASS |
| G1.6 | `bash scripts/validate_repo_layout.sh` passes | "Repository layout looks OK" | ☑ PASS |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | "VALIDATION PASSED" | ☑ PASS |

---

## Pre-Tag Todo Checklist (Historical — Completed)

The following items were completed before publishing `v0.1.0-rc.1`:

- [x] Re-run all G1 gates immediately before tagging
- [x] Verify/update CHANGELOG: document all P0/P1/P2 resolutions (scope-mismatch deny, poisoned-context fixtures, Phase F docs pack, clippy clean, RC script)
- [x] Verify/update RELEASE notes: explicitly state single-node SQLite scope, Phase 3 deferred, conditional production posture
- [x] Include accepted-risks table in release notes
- [x] Include signoff language: "This is an RC tag for v1 single-node SQLite. Production deployment requires evaluation against `27-production-evaluation-plan.md` and explicit operator signoff."
- [x] Do NOT claim production-ready in release notes
- [x] Do NOT bump Cargo.toml version; `Cargo.toml` remains `0.1.0`
- [x] Publish CHANGELOG.md and RELEASE.md as release-facing documentation

> **Future RC refresh note**: Any future RC tag refresh (e.g., v0.1.0-rc.3 or later) should wait until the operator blockers in `66-path-2-operator-handoff.md` and P5c drills are resolved.

---

## Pre-Tag Todo Checklist (rc.2 — Prepared, Not Yet Tagged)

The following items are completed for the rc.2 refresh:

- [x] Re-run all G1 gates immediately before tagging
- [x] Verify/update CHANGELOG: document rc.2 delta (MCP D1, auth gate, rate limiting, local smoke, D78-8, docs, T3 scaffolds, clippy cleanup)
- [x] Verify/update RELEASE notes: explicitly state single-node SQLite scope, Block A blocked, SendGrid pending, live MCP smoke open, conditional production posture
- [x] Include accepted-risks table in release notes
- [x] Include signoff language: "This is an RC tag for v1 single-node SQLite. Production deployment requires evaluation against `27-production-evaluation-plan.md` and explicit operator signoff."
- [x] Do NOT claim production-ready in release notes
- [x] Do NOT bump Cargo.toml version; `Cargo.toml` remains `0.1.0`
- [x] Publish CHANGELOG.md and RELEASE.md as release-facing documentation
- [ ] Create git tag `v0.1.0-rc.2` (deferred to release engineer)
- [ ] Publish GitHub prerelease for `v0.1.0-rc.2` (deferred to release engineer)

---

## Accepted Risks (Must Appear in Release Notes)

| Risk | Reference |
|---|---|
| SQLite single-node write throughput ceiling (~300 writes/s sustained) | `27-production-evaluation-plan.md` §1.2 |
| No PostgreSQL/multi-node/HA in scope | ADR-50; `30-production-roadmap.md` §3 |
| Phase 2 transaction batching reverted — Phase 1 write queue is production target | `30-production-roadmap.md` §2 |
| `ferrumctl backup` bounded offline workflow with opt-in retention pruning (`--retention-days N`); no automated scheduling, no encryption | `27-production-evaluation-plan.md` §3.5 |
| Compensate may be noop-backed depending on adapter implementation | `27-production-evaluation-plan.md` §3.6 |
| Health endpoints are shallow; functional probe required for readiness | `27-production-evaluation-plan.md` §4.2 |

---

## Rollback / Abort Criteria

| Trigger | Action |
|---|---|
| Any G1 gate fails on final verification | Abort RC tag; resolve gate failure first |
| Integration test regression detected | Abort RC tag; regression is P0 blocker |
| New scope-mismatch or governance regression | Abort RC tag; revert/fix before proceeding |

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `31-release-paths-todo.md` §Path 1 | Full release path with rollback/abort criteria |
| `25-EV-v1-single-node-rc-evidence.md` | Canonical RC evidence record |
| `23-production-readiness-assessment.md` | RC-ready declaration |
| `27-production-evaluation-plan.md` | Production evaluation framework + Operator Signoff Packet |

---

*Document generated: 2026-04-28. Updated: v0.1.0-rc.2 prepared 2026-05-17 (not yet tagged). rc.1 remains published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.*
