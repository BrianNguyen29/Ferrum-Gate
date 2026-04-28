# 53 — RC Tag Checklist

> **Status**: Documentation-only. No release action performed.
> **Purpose**: Standalone fillable pre-tag checklist for FerrumGate v1 RC release.
> **Scope**: Single-node SQLite only. No production claim.
> **Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials) | **Default package version**: `0.1.0` (development — RC tag is documentation-only, no official release)

---

## Latest RC Prep Verification Observed

Recorded from fresh G1 gate run (2026-04-28). This table captures observed pass state — it does not check off the fillable pre-tag checklist, which remains for the release engineer to verify before tagging.

| # | Gate Criterion | Observed Result | Evidence |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | **PASS** | exit 0 |
| G1.2 | `cargo fmt --all -- --check` passes | **PASS** | exit 0 |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | **PASS** | exit 0 |
| G1.4 | `cargo test --workspace` passes | **PASS** (~761 tests) | ferrum-gateway 44, integration_gateway_flow 65, ferrum-adapter-maildraft 16, doctests 0 failures |
| G1.5 | `scripts/generate_rc_evidence.py` passes all checks | **PASS** | "Overall: ALL PASS" |
| G1.6 | `bash scripts/validate_repo_layout.sh` passes | **PASS** | "Repository layout looks OK" |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | **PASS** | "VALIDATION PASSED" |

Full G1 chain: `cargo check --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && python3 scripts/generate_rc_evidence.py && bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py` — ALL PASS.

---

## Pre-Tag Gate Checklist

Re-verify all gates immediately before cutting a git tag. Tagging must not proceed if any gate fails.

| # | Gate Criterion | Evidence Reference | Verified |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | Fresh P6 validation (2026-04-28) | ☐ |
| G1.2 | `cargo fmt --all -- --check` passes | Fresh P6 validation | ☐ |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | Fresh P6 validation | ☐ |
| G1.4 | `cargo test --workspace` passes (~761 tests) | Fresh feature-completeness validation | ☐ |
| G1.5 | `scripts/generate_rc_evidence.py` passes all five checks | `docs/artifacts/2026-03-30/05-contract-consistency.txt` or fresh run | ☐ |
| G1.6 | `bash scripts/validate_repo_layout.sh` passes | "Repository layout looks OK" | ☐ |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | "VALIDATION PASSED" | ☐ |

---

## Pre-Tag Todo Checklist

Complete all items before tagging:

- [ ] Re-run all G1 gates immediately before tagging
- [ ] Verify/update CHANGELOG: document all P0/P1/P2 resolutions (scope-mismatch deny, poisoned-context fixtures, Phase F docs pack, clippy clean, RC script)
- [ ] Verify/update RELEASE notes: explicitly state single-node SQLite scope, Phase 3 deferred, conditional production posture
- [ ] Include accepted-risks table in release notes
- [ ] Include signoff language: "This is an RC tag for v1 single-node SQLite. Production deployment requires evaluation against `27-production-evaluation-plan.md` and explicit operator signoff."
- [ ] Do NOT claim production-ready in release notes
- [ ] Do NOT bump Cargo.toml version (unless explicitly requested later)
- [ ] Do NOT create CHANGELOG.md or RELEASE.md files unless needed for external references

---

## Accepted Risks (Must Appear in Release Notes)

| Risk | Reference |
|---|---|
| SQLite single-node write throughput ceiling (~300 writes/s sustained) | `27-production-evaluation-plan.md` §1.2 |
| No PostgreSQL/multi-node/HA in scope | ADR-50; `30-production-roadmap.md` §3 |
| Phase 2 transaction batching reverted — Phase 1 write queue is production target | `30-production-roadmap.md` §2 |
| `ferrumctl backup` bounded offline workflow only; no automated scheduling, no retention policy | `27-production-evaluation-plan.md` §3.5 |
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
| `25-v1-single-node-rc-evidence.md` | Canonical RC evidence record |
| `23-production-readiness-assessment.md` | RC-ready declaration |
| `27-production-evaluation-plan.md` | Production evaluation framework + Operator Signoff Packet |

---

*Document generated: 2026-04-28. Documentation-only — no release action performed.*
