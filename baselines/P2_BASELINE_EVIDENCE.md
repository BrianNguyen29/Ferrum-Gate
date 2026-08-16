# P2 Baseline Evidence Capture

> **Status: SAMPLE / NON-AUTHORITATIVE**
>
> This evidence is local development visibility and CI scaffolding. It is **not**
> an authoritative, controlled-runner, or SLO-grade baseline. Do not use it for
> blocking gates or release sign-off without promotion via ADR 011 and a
> validated runner.

## Repository state

- Current documentation/source HEAD: `db92f0a1bc23fac5bed68f0e54a15aa97e93bcf4` (`db92f0a`)
- Retained full-workspace coverage capture commit: `30d2c9e3b6eb17bd52e19874b71f7005c64f8650` (`30d2c9e`) — local/non-authoritative; not relabeled as current
- Historical commit: `95a70b864772b3aac4aea5b68b041faca9f3b843` (`95a70b8`) — succeeded on retry after MFA wall-clock flake
- Historical commit: `a14150e3a1956d4244b0fda551486d3fc3d47209` (`a14150e`) — library-only capture
- Branch: `roadmap/p0-p2-governance-stack`
- Captured at: 2026-07-16T14:26:53+00:00
- Runner: local uncontrolled development environment (Linux, single host)
- Operator: automated coverage refresh task

## Coverage baseline

### Coverage artifact selection

The advisory ratchet (`scripts/check_advisory_ratchet.py`) loads a default coverage evidence file when invoked without `--coverage-evidence`. That default remains the historical library-only capture:

- `baselines/coverage/coverage-evidence-a14150e.json`

The newer full-workspace first-attempt capture is stored separately:

- `baselines/coverage/coverage-evidence-30d2c9e.json`

Both files are local, non-authoritative samples. Neither is authoritative, neither is promotion-eligible, and neither is used as a ratchet promotion input. The ratchet's default has not been changed to point to the 30d2c9e evidence; changing that default would require an authoritative capture on a controlled runner, a matching current commit, and an ADR 011 promotion.

The 30d2c9e evidence is retained for local visibility and historical comparison only. Its JSON does not match the canonical ratchet evidence schema (`kind`, `format_version`, `scope`, `synthetic`, and controlled-runner fields), so it will be rejected by the ratchet even if supplied as `--coverage-evidence`.

### Full-workspace first-attempt-stable capture (30d2c9e)

#### Commands

1. Captured full-workspace coverage on the first attempt: `cargo llvm-cov --workspace`
2. Derived evidence: `python3` (parsed the `cargo llvm-cov` summary table, computed per-crate weighted line coverage)
3. Validated JSON and totals: `python3` (reloaded the JSON evidence, checked per-crate sums, and verified coverage percent)

#### Scope and constraints

- **Full-workspace**: Includes library crates, binary crates (`ferrumd`, `ferrumctl`, `ferrum-stress`, `ferrum-migrate`, `ferrum-tui`), and integration tests in `ferrum-integration-tests`.
- This capture succeeded on the first full-workspace attempt; the previous `95a70b8` capture required a retry after an MFA wall-clock test flake.
- PostgreSQL live tests and S3 MinIO live tests are excluded from this local capture; they remain CI-owned and require external services.
- The `ferrum-stress` binary crate has no test coverage in this run (0.0%).
- Only the concise JSON evidence is retained here. No giant raw LCOV or `--text` summary was generated or committed in this task.

#### Result status

- Workspace total (full-workspace): **81.94%** line coverage (77,703 total lines, 63,668 hit).
- All suites passed on the first attempt; 0 failed tests.
- All critical library crates that have coverage meet their soft thresholds.
- One advisory threshold warning remains: `ferrumd` has no coverage data in the synthetic `crates/` summary (expected; it is a binary crate and the advisory script only parses `crates/` prefixes). Its actual full-workspace coverage is **73.07%** in the JSON evidence.
- Soft mode completed with warnings only; no hard gate claimed.

#### Artifacts

- `baselines/coverage/coverage-evidence-30d2c9e.json` — parsed per-crate summary with provenance

### Previous full-workspace refresh (95a70b8)

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

- `python3 -m json.tool` validated the 30d2c9e JSON evidence file.
- A custom Python check reloaded the 30d2c9e JSON evidence, confirmed per-crate line totals sum to the workspace totals, and verified coverage percent.
- The 30d2c9e evidence carries exact commit, scope, non-authoritative, and provenance fields.
- The historical 95a70b8 and a14150e artifacts remain intact and were not overwritten.
- No raw LCOV or `--text` summary was generated or committed for 30d2c9e; only the concise JSON evidence is retained.

## Known limitations

- This is a single uncontrolled runner; variance is expected.
- The 30d2c9e capture is local/non-authoritative and not promotion eligible.
- The 30d2c9e capture succeeded on the first full-workspace attempt; the previous 95a70b8 capture required a retry after an MFA wall-clock test flake. Both are retained for history.
- PostgreSQL live tests and S3 MinIO live tests are excluded from this local run (they remain CI-owned and require external services).
- No hard threshold or SLO claim is made.
- The advisory threshold ratchet remains blocked unless separate canonical criteria (controlled runner, ADR 011 promotion, validated release profile) are met.
- The advisory threshold script only parses `crates/` paths in the synthetic summary; binary crates (`ferrumd`, `ferrumctl`, etc.) are reported via JSON.

## Current program state

- A private, GitHub-hosted, `workflow_dispatch`-only advisory workflow exists in `BrianNguyen29/ferrumgate-evidence-control` and was dispatched once for source SHA `db92f0a`.
- Run completion and artifact review are still pending; the dispatch is recorded but it is **not** outcome evidence.
- The GCP pilot project `ferrumgate-ce-497801` was torn down (`DELETE_REQUESTED`, billing detached) and local physical runner work is stopped; no active controlled runner exists.
- Controlled-runner prerequisite and evidence promotion remain required; no bypass or promotion is claimed.

## Recovery contracts publication (Recovery Slice 6)

### Scope

Publication of recovery semantics in public contracts, schemas, API documentation, operator docs, config examples, and ADRs. No recovery runtime mechanics were changed in this slice.

### Key decisions captured

- `RecoveryRequired` is a non-terminal, owner-review state for executions and rollback contracts.
- HTTP/SQLite mutation adapters are R2-rejected; only explicit policy-approved R3 actions (`auto_commit=false`) with manual verification and commit are permitted.
- The HA reconciler transitions stale `Running + Prepared` pairs to `RecoveryRequired` with `ErrorRaised` provenance, not to a terminal state.
- Once `RecoveryRequired` rows are persisted, no downgrade to a version that does not understand the state is allowed without first resolving those rows to a terminal state.

### Artifacts

- `baselines/evidence/p2.recovery-slice6-contracts-evidence.json` — validation evidence for contract/schema/API publication.
- Updated contracts: `contracts/ferrumgate-agent-contract.v1.yaml`, `contracts/ferrumgate-integrator-contract.v1.yaml`.
- Updated docs: `docs/adr/019-ambiguous-side-effect-recovery.md`, `docs/adr/016-ha-reconciler.md`, `docs/adr/README.md`, `docs/PRODUCTION_NOTES.md`, `docs/guides/operator.md`.
- Updated config examples: `configs/ferrumgate.prod.toml`, `configs/examples/ferrumd.env.example`, `configs/examples/nonprod-ferrumgate.toml`.
- Updated API/schema: `openapi/ferrumgate-control-api.v1.yaml`, `schemas/jsonschema/rollback-contract.json`.
- Updated validator: `scripts/check_contract_consistency.py`.
- Updated startup warning: `bins/ferrumd/src/main.rs`.

### Status

SAMPLE / NON-AUTHORITATIVE. This evidence documents the contract publication slice but does not claim controlled-runner or production signoff.
