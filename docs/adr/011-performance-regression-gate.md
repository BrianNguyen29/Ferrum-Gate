# ADR 011 — Performance Regression Gate

## Status
Proposed

## Context

FerrumGate has baseline performance metrics (SQLite write throughput, p99 latency, store health) documented in `docs/PRODUCTION_NOTES.md` and measured via `ferrum-stress`. However, there is no automated gate in CI or release that blocks a change if it regresses these baselines beyond an acceptable threshold. This has led to near-misses where store-layer changes degraded p99 latency or increased write-queue depth without being caught until manual review.

## Decision

Propose an automated **performance regression gate** that runs in CI and release preflight, comparing the current commit against a pinned baseline.

### 1. Baseline definition
- A `baselines/` directory containing JSON baseline files keyed by scenario (e.g., `sqlite_write_throughput.json`, `gateway_p99_latency.json`).
- Each baseline contains:
  - Metric name and unit.
  - Threshold value (e.g., p99 latency <= 50ms).
  - Measurement scenario (endpoint, concurrency, duration).
  - Last validated commit SHA.
- Baselines are updated manually via a PR that includes evidence from a controlled run (not auto-updated by CI, to avoid threshold creep).

### 2. Gate implementation
- A new `make perf-gate` target that:
  - Runs `ferrum-stress` scenarios with fixed parameters (`--duration 60s --concurrency 64`).
  - Collects Prometheus metrics or structured log output.
  - Compares each metric against the baseline threshold.
  - Fails with a clear diff if any metric exceeds its threshold.
- The gate runs in the `release.yml` workflow (mandatory) and in the `validate.yml` workflow (advisory, `continue-on-error: true` initially to avoid blocking unrelated PRs while baselines stabilize).

### 3. Threshold policy
- **Hard threshold**: store write throughput must not drop below X rows/sec; p99 latency must not exceed Y ms.
- **Soft threshold**: memory usage, binary size, and compile time are tracked but do not fail the gate (informational only).
- Thresholds are per-scenario; a change that improves one scenario at the cost of another must be justified in the PR.

### 4. Local reproducibility
- The gate must run on developer laptops with the same parameters as CI (same SQLite pragmas, same concurrency, same duration).
- A `make perf-baseline-update` script is provided to regenerate baselines after a deliberate optimization PR.

## Consequences

- **Positive**: Prevents silent performance regressions from reaching `main`.
- **Positive**: Creates a documented, reproducible performance contract.
- **Negative**: Baselines require hardware-normalization consideration (CI runners vs. local dev). The initial implementation uses relative thresholds (e.g., "no worse than 110% of baseline") to account for runner variance.
- **Negative**: Adds CI time (~2-3 minutes per gate run).
- **Non-goal**: This does not optimize performance; it only detects regressions. Performance optimization is handled by separate work.

## Acceptance criteria

1. `baselines/` directory exists with at least two validated baseline files (`sqlite_write_throughput.json`, `gateway_p99_latency.json`).
2. `make perf-gate` runs `ferrum-stress` and compares results against baselines.
3. The gate fails with a structured diff if thresholds are exceeded.
4. The gate runs in `.github/workflows/release.yml` as a mandatory step.
5. The gate runs in `.github/workflows/validate.yml` as an advisory step (`continue-on-error: true`).
6. `make perf-baseline-update` regenerates baseline files with current commit SHA and timestamp.
7. Documentation updated: `docs/PRODUCTION_NOTES.md`, `RELEASE.md`, `CONTRIBUTING.md`.
8. The gate is tested against a known-bad commit (e.g., one that intentionally removes SQLite write queue) to confirm it catches regressions.

## Non-goals

- Hardware-specific tuning (e.g., CPU pinning, NUMA awareness). The gate uses stock runners and relative thresholds.
- Database benchmarking (PostgreSQL performance is operator-dependent and not suitable for a fixed baseline).
- End-to-end load testing of MCP stdio throughput (too variable; focused on HTTP gateway and store metrics).
- Auto-optimization or auto-tuning of SQLite pragmas (the gate detects changes; tuning is operator work).
