# 01 — SLO/SLA Draft and Validation Runbook

> **Status**: Planning artifact. Targets not yet measured.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Formalize "production acceptable" as measurable SLO targets and create a repeatable validation runbook so that workload evidence can be collected, compared against targets, and signed off.

## Current state

- RPO/RTO are formally accepted for conditional pilot: RPO=15min, RTO=15min.
- Stress test baselines exist for SQLite (see [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md)).
- No formal SLO/SLA document exists.
- No target-host sustained workload evidence exists.
- Existing scripts:
  - `scripts/stress/run-all.sh`
  - `scripts/run_real_workload_generator.py`
  - `scripts/run_g36_workload_wrapper.sh`
  - `scripts/check_pilot_readiness.py`

## Gaps

| Gap | Why it matters |
|-----|---------------|
| No SLO doc | Cannot claim production posture without measurable targets. |
| No validation runbook | Workload runs are ad-hoc; not repeatable or comparable. |
| No target latency percentiles | p50/p95/p99 for governance endpoints are unknown on target hardware. |
| No error-rate budget | Acceptable 5xx/rate-limit rate undefined. |
| No durability SLO | Backup age, restore success, and corruption checks not formalized. |
| No correctness SLO | Capability bypass, provenance gap, and scope violation targets not stated. |
| No security SLO | Auth bypass and secret-leak targets not stated. |

## SLO groups (draft — to be ratified)

| Group | Metric | Pilot SLO | Single-node PG SLO | HA SLO |
|-------|--------|-----------|--------------------|--------|
| Availability | `/v1/healthz` uptime | 99.0% | 99.5% | 99.9% |
| Availability | `/v1/readyz/deep` uptime | 99.0% | 99.5% | 99.9% |
| Latency | evaluate p99 | < 500ms | < 300ms | < 200ms |
| Latency | mint p99 | < 500ms | < 300ms | < 200ms |
| Latency | execute pipeline p99 | < 5s | < 3s | < 2s |
| Error rate | 5xx rate | < 1% | < 0.5% | < 0.1% |
| Error rate | 429 rate | < 5% | < 2% | < 1% |
| Durability | backup age | < 15min | < 15min | < 15min |
| Durability | restore success | 100% | 100% | 100% |
| Correctness | capability bypass | 0 | 0 | 0 |
| Correctness | provenance gap | 0 | 0 | 0 |
| Correctness | scope violation | 0 | 0 | 0 |
| Security | auth bypass | 0 | 0 | 0 |
| Security | secret leak in output/logs | 0 | 0 | 0 |
| Operational | incident acknowledgement | < 1h | < 30min | < 15min |

> **Non-claim**: These are draft targets. They are not validated. Do not cite them as committed SLAs until a validation runbook passes and an operator ratifies them.

## Implementation tasks

1. **Draft SLO/SLA doc**
   - Define each group, metric, target, and measurement method.
   - Define observation window (e.g., 7-day rolling).
   - Define alert thresholds (e.g., 2% budget burn in 1 hour).

2. **Create validation runbook**
   - See [`slo-validation-runbook.md`](slo-validation-runbook.md) for the repeatable procedure.
   - Prechecks: hardware, config, store backend, auth mode.
   - Workload phases: baseline → low → target → spike → cooldown.
   - Expected outputs: latency histograms, error counts, queue depth, memory/CPU.
   - Pass/fail criteria mapped to SLO table.
   - Evidence artifact format (markdown + metrics snapshot).

3. **Instrument missing metrics**
   - Governance endpoint latency histograms (currently only public endpoints have them).
   - WAL/page gauges.
   - Connection pool saturation (for PG phase).

4. **Run target/staging workload**
   - Baseline 600s.
   - Low 600s.
   - Target 1800s.
   - Spike 300s.
   - Cooldown 600s.

5. **Refresh workload model**
   - Compare assumed vs observed capacity.
   - Document safe limits and ceiling.

## Acceptance criteria

- [ ] SLO/SLA doc exists and is ratified by operator.
- [ ] Runbook maps each script to a pass/fail gate.
- [ ] At least one target workload run completed with evidence artifact.
- [ ] p95/p99 latency recorded for evaluate, mint, authorize, prepare, execute, verify.
- [ ] Error rate recorded and under threshold.
- [ ] Evidence artifact reviewed and signed.

## Evidence required

- `slo-validation-runbook.md`
- `slo-target-evidence-{date}.md`
- Metrics snapshot (`/v1/metrics` scrape before/during/after)

## Non-claims

- **NOT a committed SLA**: These are draft SLOs for engineering planning only.
- **NOT validated**: No target workload has been run against these thresholds yet.
- **NOT production-ready evidence**: SLO definition alone does not constitute production-ready claim.

## Related docs

- [`slo-validation-runbook.md`](slo-validation-runbook.md) — Repeatable validation procedure
- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.1, §4 Phase 2
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Stress baselines
- [`docs/implementation-path/57-workload-compensation-drill-plan.md`](../../implementation-path/57-workload-compensation-drill-plan.md)
