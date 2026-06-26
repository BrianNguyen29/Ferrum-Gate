# ADR 011 — Performance Regression Gate

## Status
Accepted (advisory / non-blocking in regular CI)

## Context

FerrumGate has baseline performance metrics (SQLite write throughput, p99 latency, store health) documented in `docs/PRODUCTION_NOTES.md` and measured via `ferrum-stress`. However, there is no automated gate in CI or release that blocks a change if it regresses these baselines beyond an acceptable threshold. This has led to near-misses where store-layer changes degraded p99 latency or increased write-queue depth without being caught until manual review.

## Decision

Implement an automated **performance regression gate** scaffold that runs locally and in CI, comparing the current commit against sample baselines.

### 1. Baseline definition
- A `baselines/` directory containing JSON baseline files keyed by scenario (e.g., `sample_health_5s.json`, `sample_intent_compile_5s.json`).
- Each baseline contains:
  - Metric name and unit.
  - Baseline value and threshold ratios (e.g., `min_ratio` for throughput, `max_ratio` for latency).
  - Measurement scenario (endpoint, concurrency, duration).
  - Last validated commit SHA and a `note` field.
- Baselines are **sample / non-authoritative** initially. They are updated manually via a PR that includes evidence from a controlled run (not auto-updated by CI, to avoid threshold creep).

### 2. Gate implementation
- A new `make perf-gate` target that:
  - Runs `ferrum-stress` scenarios with short, fixed parameters (`--duration 5s` by default to avoid long CI waits).
  - Collects JSON output from `ferrum-stress`.
  - Compares each metric against the baseline threshold using `scripts/compare_perf_baselines.py`.
  - Fails with a clear diff if any metric exceeds its threshold, but defaults to **dry-run / advisory mode** so it does not block regular CI.
- A `make perf-baseline-update` target regenerates baseline files with current results (developer must review and label them).

### 3. Threshold policy
- **Relative thresholds**: throughput must not drop below X% of baseline; latency must not exceed Y% of baseline. This accounts for runner variance.
- **Hard threshold**: error rate must not exceed 1%.
- **Soft / informational**: memory usage, binary size, and compile time are not yet tracked.
- Thresholds are per-scenario; a change that improves one scenario at the cost of another must be justified in the PR.

### 4. Local reproducibility
- The gate runs on developer laptops with the same parameters as CI (same SQLite pragmas, same concurrency, same duration).
- A `make perf-baseline-update` script is provided to regenerate baselines after a deliberate optimization PR.

## Consequences

- **Positive**: Prevents silent performance regressions from reaching `main` (once baselines are authoritative).
- **Positive**: Creates a documented, reproducible performance contract scaffold.
- **Negative**: Baselines require hardware-normalization consideration (CI runners vs. local dev). The initial implementation uses relative thresholds to account for runner variance.
- **Negative**: Adds ~1 minute to local validation when run explicitly.
- **Non-goal**: This does not optimize performance; it only detects regressions. Performance optimization is handled by separate work.

## Acceptance criteria

1. `baselines/` directory exists with at least three validated baseline files (`sample_health_5s.json`, `sample_intent_compile_5s.json`, `sample_sqlite_contention_5s.json`).
2. `make perf-gate` runs `ferrum-stress` and compares results against baselines in advisory mode.
3. The gate prints a structured diff if thresholds are exceeded.
4. The gate runs in `.github/workflows/release.yml` as an **advisory** step (not blocking).
5. The gate runs in `.github/workflows/manual-gates.yml` as an optional manual gate.
6. `make perf-baseline-update` regenerates baseline files with current results.
7. Documentation updated: `docs/PRODUCTION_NOTES.md`, `RELEASE.md`, `docs/CONTRIBUTING.md`.
8. The gate is tested against a known-bad commit (e.g., one that intentionally removes SQLite write queue) to confirm it catches regressions.

## Non-goals

- Hardware-specific tuning (e.g., CPU pinning, NUMA awareness). The gate uses stock runners and relative thresholds.
- Database benchmarking (PostgreSQL performance is operator-dependent and not suitable for a fixed baseline).
- End-to-end load testing of MCP stdio throughput (too variable; focused on HTTP gateway and store metrics).
- Auto-optimization or auto-tuning of SQLite pragmas (the gate detects changes; tuning is operator work).
- Blocking CI enforcement while baselines are sample / non-authoritative.
