# P2 Baseline Evidence Capture

> **Status: SAMPLE / NON-AUTHORITATIVE**
>
> This evidence is a first local capture for development visibility and CI
> scaffolding. It is **not** an authoritative, controlled-runner, or SLO-grade
> baseline. Do not use it for blocking gates or release sign-off without
> promotion via ADR 011 and a validated runner.

## Repository state

- Commit: `a14150e3a1956d4244b0fda551486d3fc3d47209` (`a14150e`)
- Branch: `roadmap/p0-p2-governance-stack`
- Captured at: 2026-07-15T01:26:24Z
- Runner: local uncontrolled development environment (Linux, single host)
- Operator: automated P2 capture task

## Coverage baseline

### Commands

1. Installed project-supported tooling: `cargo install --locked cargo-llvm-cov`
2. Captured library coverage: `cargo llvm-cov test --lib --workspace --ignore-run-fail`
3. Generated LCOV summary: `cargo llvm-cov report --lcov --summary-only --output-path coverage-lcov.info`
4. Derived evidence: `python3` (parsed LCOV, computed per-crate weighted line coverage)
5. Validated thresholds: `python3 scripts/check_coverage_threshold.py baselines/coverage/coverage-threshold-summary-a14150e.txt --config coverage-thresholds.toml`

### Scope and constraints

- **Library-only**: The full workspace run (`cargo llvm-cov --workspace`) failed
  in `ferrum-integration-tests` with 2 failures:
  - `test_i6_single_use_with_valid_approval_binding`
  - `test_single_use_capability_cannot_be_reused_via_gateway`
- To capture a reproducible, bounded artifact, coverage was computed from
  `--lib` tests only. Binary-only crates (e.g., `ferrumd`, `ferrumctl`, `ferrum-stress`,
  `ferrum-tui`, `ferrum-migrate`) are not represented in this library-only report.
- The raw LCOV file is 16 KB and stored as a diagnostic source; the derived JSON
  is the primary evidence artifact.

### Result status

- Workspace total (lib-only): **82.67%** line coverage
- All critical crates that have library coverage meet their soft thresholds.
- One threshold miss: `ferrumd` has no coverage data in this library-only run
  (expected; it is a binary crate).
- Soft mode completed with warnings only; no hard gate claimed.

### Artifacts

- `baselines/coverage/coverage-a14150e.info` — raw LCOV summary (library-only)
- `baselines/coverage/coverage-evidence-a14150e.json` — parsed per-crate summary
- `baselines/coverage/coverage-threshold-summary-a14150e.txt` — synthetic text
  summary compatible with `scripts/check_coverage_threshold.py`

## Performance baseline

### Command

`make perf-baseline-update`

### Scenarios

| Scenario | Concurrency | Duration | req/s | p95 ms | p99 ms | Error rate |
|----------|-------------|----------|-------|--------|--------|------------|
| health | 50 | 5s | 3805.4 | 24.76 | 33.12 | 0.0 |
| intent-compile | 5 | 5s | 185.2 | 37.47 | 473.20 | 0.0 |
| sqlite-contention | 50 | 5s | 158.8 | 2207.02 | 2217.56 | 0.0 |

### Result status

- All three scenarios completed without errors.
- Generated `baselines/sample_*_5s.json` files remain SAMPLE / NON-AUTHORITATIVE.
- Promotion to authoritative requires a controlled runner, coverage gate, and
  ADR 011 update.

### Artifacts

- `baselines/evidence/perf-stress-a14150e.json` — raw merged `ferrum-stress` output
- `baselines/evidence/perf-evidence-a14150e.json` — perf capture metadata
- `baselines/sample_health_5s.json`
- `baselines/sample_intent_compile_5s.json`
- `baselines/sample_sqlite_contention_5s.json`

## Validation performed

- `python3 -m json.tool` validated all JSON evidence files.
- `scripts/check_coverage_threshold.py` parsed the synthetic coverage summary
  and produced the expected soft-mode warnings.
- `make perf-baseline-update` exited successfully and wrote the sample baselines.

## Known limitations

- This is a single uncontrolled runner; variance is expected.
- Coverage excludes binary crates and integration tests because of the current
  integration-test failures at this commit.
- Performance samples are short (5 seconds) and advisory.
- No hard threshold or SLO claim is made.
