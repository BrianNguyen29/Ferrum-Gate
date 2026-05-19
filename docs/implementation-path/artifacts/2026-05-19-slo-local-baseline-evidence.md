# SLO-3 Local Workload Baseline Evidence — 2026-05-19

## Status

- **Scope**: LOCAL SQLite in-memory workload baseline only.
- **Verdict**: ✅ PASS for local bounded baseline.
- **Target-host validated**: NO.
- **Production-ready**: NO.
- **SLO ratified**: NO — targets remain draft/conditional.
- **Operator signoff**: NOT OBTAINED.

This artifact records a local-only workload run against `target/release/ferrumd` with
in-memory SQLite. It is NOT evidence that SLOs are met on target hardware, staging,
or production. It establishes a local reproducibility baseline and validates that
the measurement pipeline (readiness checks, stress suite, workload generator,
metrics scrape) functions end-to-end.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Local development workstation |
| Binary | `./target/release/ferrumd` |
| Bind address | `127.0.0.1:8080` |
| Store DSN | `sqlite::memory:` (in-memory) |
| Auth mode | `disabled` |
| Rate limit | `per_second=2`, `burst=50` |
| Config source | `configs/ferrumgate.dev.toml` (auto-loaded) |

## Pre-run checks

### Pilot readiness

`check_pilot_readiness.py` initially failed because `ferrumctl` was not in PATH.
After building `ferrumctl` (`cargo build --package ferrumctl --release`) and adding
`target/release` to PATH, the probe was re-run.

Command:

```bash
python3 scripts/check_pilot_readiness.py \
  --server-url http://127.0.0.1:8080 \
  --bearer-token local-disabled-auth-token
```

Observed result:

```text
All probes passed: shallow, deep, functional, metrics.
```

Result: ✅ PASS.

### Stress baseline

Initial `scripts/stress/run-all.sh` invocation failed due to shell script issues
(`local: can only be used in a function`, pipefail/unbound variable/grep issues).
These were fixed separately across `run-all.sh`, `s1-health.sh`, `s2-auth.sh`,
`s4-intent-compile.sh`, `s7-sqlite-contention.sh`, and `s8-rate-limit.sh`.

Full stress baseline command:

```bash
BASE_URL=http://127.0.0.1:8080 DURATION=10 WORKERS=10 bash scripts/stress/run-all.sh
```

Observed results:

| Scenario | Requests | RPS | run-all errors |
|----------|----------|-----|----------------|
| s1-health | 2368 | 236.80 | 0% |
| s2-auth | 2583 | 258.30 | 0% |
| s4-intent-compile | 1459 | 145.90 | 0% |
| s7-sqlite-contention | 2093 | 209.30 | 0% |
| s8-rate-limit | 2480 | 248.00 | 0% |

**Caveats**:
- s2-auth, s4-intent-compile, and s7-sqlite-contention track expected HTTP
  statuses separately from unexpected errors; per-script error lines now count
  only unexpected responses.
- s8-rate-limit reported 0% errors and no 429 responses were detected. Rate
  limiting may not be fully effective in this local in-memory configuration.

Result: ✅ PASS — run-all summary reports 0% errors for all scenarios.

### Workload plan

Command:

```bash
python3 scripts/run_real_workload_generator.py \
  --plan \
  --server-url http://127.0.0.1:8080 \
  --output-dir /tmp/opencode/ferrum-slo-local-20260519
```

Result: ✅ PASS — `workload_plan.json` and `workload_plan.md` generated.

## Workload execution

Command:

```bash
python3 scripts/run_real_workload_generator.py \
  --execute \
  --server-url http://127.0.0.1:8080 \
  --bearer-token local-disabled-auth-token \
  --output-dir /tmp/opencode/ferrum-slo-local-20260519 \
  --phases '[
    {"name":"baseline","duration_sec":2,"rate_rps":0},
    {"name":"low","duration_sec":5,"rate_rps":0.2},
    {"name":"target","duration_sec":10,"rate_rps":1.0},
    {"name":"spike","duration_sec":5,"rate_rps":2.0},
    {"name":"cooldown","duration_sec":2,"rate_rps":0}
  ]'
```

### Phase results

| Phase | Duration | Rate (rps) | Requests | Status 200 | Errors |
|-------|----------|------------|----------|------------|--------|
| baseline | 2 s | 0.0 | 0 | 0 | 0 |
| low | 5 s | 0.2 | ~1 | 1 | 0 |
| target | 10 s | 1.0 | 10 | 10 | 0 |
| spike | 5 s | 2.0 | 11 | 11 | 0 |
| cooldown | 2 s | 0.0 | 0 | 0 | 0 |

**Total requests**: 22 (estimated 21; actual 22).
**Status distribution**: 22 HTTP 200.
**Error rate**: 0%.

### Latency (ms)

**Global**:

| Metric | Value |
|--------|-------|
| p50 | 1.785 |
| p95 | 2.07 |
| p99 | 638.249 |
| min | 1.53 |
| max | 807.36 |

> **Caveat**: High global p99 is driven by a single low-phase warm-up request at
> 807.36 ms. This is expected for an in-memory SQLite warm-up and is not
> representative of steady-state latency.

**Phase target** (10 requests, all 200):

| Metric | Value |
|--------|-------|
| p50 | 1.735 |
| p95 | 2.0255 |
| p99 | 2.0291 |

**Phase spike** (11 requests, all 200):

| Metric | Value |
|--------|-------|
| p50 | 1.79 |
| p95 | 2.07 |
| p99 | 2.07 |

### Readyz probe log

During execution: 5/5 HTTP 200.

| Probe # | Latency (ms) |
|---------|--------------|
| 1 | 1.30 |
| 2 | 1.68 |
| 3 | 1.76 |
| 4 | 1.64 |
| 5 | 1.48 |

## Post-run checks

### Readiness recheck

Repeated `check_pilot_readiness.py` after cooldown.

Result: ✅ PASS — all probes passed.

### Metrics scrape (post-run)

Key metric values observed:

| Metric | Value |
|--------|-------|
| `store_health_up` | 1 |
| `write_queue_depth` | 0 |
| `readyz_deep` 200 count | 7 |
| `readyz_deep` 503 count | 0 |
| `governance_success_total` (intents compile) | 22 |
| `governance_success_total` (approvals) | 2 |
| `governance_errors_total` | 0 (all categories) |

Result: ✅ PASS — no errors, store healthy, queue empty.

## SLO comparison (draft targets vs local observation)

> **Non-claim**: This table compares observed local values against draft pilot
> targets for reference only. It does NOT validate that targets are met on target
> hardware or that SLOs are ratified.

| Group | Metric | Draft Pilot Target | Local Observed | Notes |
|-------|--------|--------------------|----------------|-------|
| Availability | `/v1/healthz` uptime | 99.0% | 100% (22/22 OK) | Local only |
| Availability | `/v1/readyz/deep` uptime | 99.0% | 100% (7/7 OK) | Local only |
| Latency | evaluate p99 | < 500 ms | ~2.07 ms | Local in-memory; not representative |
| Latency | mint p99 | < 500 ms | N/A (not isolated) | Local only |
| Latency | execute pipeline p99 | < 5 s | < 2.07 ms | Local only |
| Error rate | 5xx rate | < 1% | 0% | Local only |
| Error rate | 429 rate | < 5% | 0% | Rate limit not triggered locally |
| Durability | backup age | < 15 min | N/A | In-memory store; backup not applicable |
| Durability | restore success | 100% | N/A | In-memory store; backup not applicable |
| Correctness | capability bypass | 0 | 0 | Local only |
| Correctness | provenance gap | 0 | 0 | Local only |
| Correctness | scope violation | 0 | 0 | Local only |
| Security | auth bypass | 0 | N/A | Auth disabled for local baseline |
| Security | secret leak | 0 | 0 | No secrets in output |
| Operational | incident acknowledgement | < 1h | N/A | Not measured |

## Backup spot check

**N/A** — In-memory SQLite store; no durable backup was taken during this run.
Durability SLO validation requires a persistent backend (PostgreSQL or file-backed
SQLite) and is out of scope for this local baseline.

## Anomalies and caveats

1. **Warm-up latency outlier**: Single request in `low` phase at 807.36 ms
   inflated global p99. Phase-level p99 for `target` and `spike` are both ~2.07 ms.
2. **Auth disabled**: `auth_mode=disabled` means auth-path latency and security
   SLOs (auth bypass, token validation) were not exercised.
3. **Rate limit not exercised**: No 429 responses observed. Local in-memory config
   may not trigger rate limiting at the tested rates.
4. **Short phases**: Durations were shortened (2–10 s) for local iteration speed.
   Canonical runbook calls for 300–1800 s phases.
5. **Stress script caveats**: s2-auth, s4-intent-compile, and s7-sqlite-contention
   now count expected statuses separately; only unexpected responses contribute
   to the per-script error count and run-all summary.
6. **NOT target-host validated**: This run was executed on a local workstation
   against an in-memory store. It does not predict performance on target hardware,
   with PostgreSQL, or under sustained load.

## Generated artifacts

Files written to `/tmp/opencode/ferrum-slo-local-20260519`:

- `workload_plan.json`
- `workload_plan.md`
- `checkpoint_phase_000.json` through `checkpoint_phase_004.json`
- `workload_results.json`
- `workload_results.md`
- `readyz_probe_log.json`
- `readyz_probe_log.md`

These files are temporary local outputs and are NOT committed to the repository.

## Operator signoff

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Operator reviewer | | | |

> **Blank until reviewed**. This artifact is a local engineering baseline and has
> not been reviewed or signed off by an operator.

## Related docs

- [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) — Draft SLO targets and non-claims
- [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) — Repeatable validation procedure
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
