# Local Baseline Verification — 2026-05-08

> **Status**: Local-only verification artifact.
> **Purpose**: Record repo-side checks run before requesting real Path 2 target values.
> **Scope**: No target host, no operator evidence, no G2 completion, no production-ready claim.

---

## Summary

All local repo-side checks passed after fixing MCP hygiene/doctest issues.

This artifact records local preparation only. It does **not** replace target-host Path 2 evidence.

---

## Commands and Results

| # | Command | Result | Notes |
| --- | --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | PASS | No output. |
| 2 | `cargo check --workspace` | PASS | Cargo reported a corrupt incremental artifact under `target/debug/incremental/ferrum_proto...`; it was ignored/deleted automatically. `ferrum-proto` generated one non-blocking warning. |
| 3 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Workspace clippy passed. |
| 4 | `cargo test --workspace` | PASS after fix/rerun | First run found two MCP doctest failures; see below. |
| 5 | `bash scripts/validate_repo_layout.sh` | PASS | Output: `Repository layout looks OK`. |
| 6 | `python3 scripts/check_contract_consistency.py` | PASS | Output: `VALIDATION PASSED`. |
| 7 | `bash scripts/run_pre_target_gate.sh` | PASS | Output: `ALL LOCAL CHECKS PASSED`. |

---

## Test Failure / Fix / Rerun Notes

The first `cargo test --workspace` run failed in two MCP doctests from
`crates/ferrum-integrations-mcp/src/mapping_helpers.rs` because the examples imported
private/internal helper APIs as executable doctests.

Fix applied:

- internal/private helper examples were marked `ignore`
- inline explanation was added so the examples remain documentation but are not executed as public API doctests
- no behavior tests were removed

Rerun result:

- `cargo test --workspace`: PASS
- MCP doctests: 2 ignored, 0 failed
- `ferrum-sync` doctest: 1 passed
- all unit/integration/doc tests otherwise passed

---

## Pre-Target Gate Details

`bash scripts/run_pre_target_gate.sh` passed all local checks:

- cargo format check: PASS
- cargo workspace compile check: PASS
- ferrumctl smoke: PASS
- config examples validation: PASS
- local restore drill: PASS
- evidence skeleton generator: PASS
- required Path 2 docs present: PASS
- required config examples present: PASS
- local bearer-auth smoke: PASS (`Passed: 7`, `Failed: 0`)

Restore drill note:

- `sqlite3` was not available in this environment
- source/backup/restored DB integrity checks passed through `ferrumctl`
- data comparison was skipped by the script because `sqlite3` was unavailable

---

## Explicit Non-Claims

- No G2 gate is complete.
- No production pilot is authorized.
- No operator signoff exists.
- No target host was used.
- No real target evidence was collected.
- FerrumGate remains RC-ready / conditional single-node SQLite.

---

## Next Required Evidence Before Production Pilot

Target/operator-owned evidence remains required:

- real target values from `71-path-2-target-values-intake-packet.md`
- filled `63-path-2-target-environment-spec.md`
- filled `65-path-2-target-questionnaire.md`
- target-host probes and metrics
- target-host D1–D6 compensation drill evidence
- target-host backup/restore evidence
- completed `59-pilot-readiness-evidence-packet.md`
- signed `54-operator-signoff-packet.md`
