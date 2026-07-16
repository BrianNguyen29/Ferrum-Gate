# P2 Baseline Evidence Capture

> **Status: SAMPLE / NON-AUTHORITATIVE**
>
> This evidence is local development visibility and CI scaffolding. It is **not**
> an authoritative, controlled-runner, or SLO-grade baseline. Do not use it for
> blocking gates or release sign-off without promotion via ADR 011 and a
> validated runner.

## Repository state

- Current commit: `95a70b864772b3aac4aea5b68b041faca9f3b843` (`95a70b8`)
- Historical commit: `a14150e3a1956d4244b0fda551486d3fc3d47209` (`a14150e`)
- Branch: `roadmap/p0-p2-governance-stack`
- Captured at: 2026-07-16T13:19:21+00:00
- Runner: local uncontrolled development environment (Linux, single host)
- Operator: automated coverage refresh task

## Coverage baseline

### Full-workspace refresh (95a70b8)

#### Commands

1. Installed project-supported tooling: `cargo install --locked cargo-llvm-cov`
2. Captured full-workspace coverage: `cargo llvm-cov --workspace --text --output-path baselines/coverage/coverage-summary-95a70b8.txt`
3. Generated LCOV report: `cargo llvm-cov report --lcov --output-path baselines/coverage/coverage-95a70b8.info`
4. Derived evidence: `python3` (parsed LCOV, computed per-crate weighted line coverage)
5. Validated thresholds: `python3 scripts/check_coverage_threshold.py baselines/coverage/coverage-threshold-summary-95a70b8.txt --config coverage-thresholds.toml`

#### Scope and constraints

- **Full-workspace**: Includes library crates, binary crates (`ferrumd`, `ferrumctl`, `ferrum-stress`, `ferrum-migrate`, `ferrum-tui`), and integration tests in `ferrum-integration-tests`.
- The previously failing integration tests now pass:
  - `test_i6_single_use_with_valid_approval_binding`
  - `test_single_use_capability_cannot_be_reused_via_gateway`
- PostgreSQL live tests are excluded from this local capture; they remain CI-owned and require a running PostgreSQL service.
- One binary crate with no test coverage in this run (`ferrum-stress` at 0.0%) is included in the JSON evidence but is not represented in the synthetic text summary used by the advisory threshold script.
- The raw LCOV file is 2.4 MB and the raw text summary is 6.2 MB; both are stored as diagnostic sources; the derived JSON is the primary evidence artifact.

#### Result status

- Workspace total (full-workspace): **81.94%** line coverage
- All critical library crates that have coverage meet their soft thresholds.
- One threshold warning remains: `ferrumd` has no coverage data in the synthetic `crates/` summary (expected; it is a binary crate and the advisory script only parses `crates/` prefixes). Its actual full-workspace coverage is **73.07%** in the JSON evidence.
- Soft mode completed with warnings only; no hard gate claimed.

#### Artifacts

- `baselines/coverage/coverage-95a70b8.info` — raw LCOV (full-workspace)
- `baselines/coverage/coverage-evidence-95a70b8.json` — parsed per-crate summary
- `baselines/coverage/coverage-threshold-summary-95a70b8.txt` — synthetic text summary compatible with `scripts/check_coverage_threshold.py`
- `baselines/coverage/coverage-summary-95a70b8.txt` — raw `cargo llvm-cov --workspace --text` output

### Historical library-only capture (a14150e)

> Retained for comparison. The artifacts below are **not overwritten**.

#### Commands

1. Installed project-supported tooling: `cargo install --locked cargo-llvm-cov`
2. Captured library coverage: `cargo llvm-cov test --lib --workspace --ignore-run-fail`
3. Generated LCOV summary: `cargo llvm-cov report --lcov --summary-only --output-path coverage-lcov.info`
4. Derived evidence: `python3` (parsed LCOV, computed per-crate weighted line coverage)
5. Validated thresholds: `python3 scripts/check_coverage_threshold.py baselines/coverage/coverage-threshold-summary-a14150e.txt --config coverage-thresholds.toml`

#### Scope and constraints

- **Library-only**: The full workspace run (`cargo llvm-cov --workspace`) failed
  in `ferrum-integration-tests` with 2 failures:
  - `test_i6_single_use_with_valid_approval_binding`
  - `test_single_use_capability_cannot_be_reused_via_gateway`
- To capture a reproducible, bounded artifact, coverage was computed from
  `--lib` tests only. Binary-only crates (e.g., `ferrumd`, `ferrumctl`, `ferrum-stress`,
  `ferrum-tui`, `ferrum-migrate`) are not represented in this library-only report.
- The raw LCOV file is 16 KB and stored as a diagnostic source; the derived JSON
  is the primary evidence artifact.

#### Result status

- Workspace total (lib-only): **82.67%** line coverage
- All critical crates that have library coverage meet their soft thresholds.
- One threshold miss: `ferrumd` has no coverage data in this library-only run
  (expected; it is a binary crate).
- Soft mode completed with warnings only; no hard gate claimed.

#### Artifacts

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

- `python3 -m json.tool` validated the 95a70b8 JSON evidence file.
- `scripts/check_coverage_threshold.py` parsed the 95a70b8 synthetic coverage summary
  and produced the expected soft-mode warnings (advisory/non-blocking).
- The historical a14150e artifacts remain intact and were not overwritten.

## Known limitations

- This is a single uncontrolled runner; variance is expected.
- The 95a70b8 capture is local/non-authoritative and not promotion eligible.
- PostgreSQL live tests and S3 MinIO live tests are excluded from this local run
  (they remain CI-owned and require external services).
- One flaky unit test (`test_mfa_credential_lockout_recovery_one_strike_relock` in
  `ferrum-store`) failed on the first full-workspace coverage attempt due to a
  wall-clock race; it passed on retry and in isolation. This is a pre-existing
  timing sensitivity, not related to the integration fixes being verified.
- No hard threshold or SLO claim is made.
- The advisory threshold script only parses `crates/` paths in the synthetic
  summary; binary crates (`ferrumd`, `ferrumctl`, etc.) are reported via JSON.
