# 01 — SLO/SLA Draft and Validation Runbook

> **Status**: Planning artifact. Canonical SLO target runs executed 2026-05-21. Default-config gap resolved as conservative default.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

> **Delegated signoff (planning-only)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: SLO/SLA draft ratified for validation baseline; default-config gap resolved as conservative default.
> - **Nature**: Planning/decision document signoff only. This does not constitute evidence of implementation, deployment, or production readiness. Does not substitute for missing evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

---

## Goal

Formalize "production acceptable" as measurable SLO targets and create a repeatable validation runbook so that workload evidence can be collected, compared against targets, and signed off.

## Current state

- RPO/RTO are formally accepted for conditional pilot: RPO=15min, RTO=15min.
- Stress test baselines exist for SQLite (see [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md)).
- Formal SLO/SLA draft exists (this doc) and was ratified for validation baseline on 2026-05-20.
- Local workload baseline evidence generated on 2026-05-19 (see `docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md`). This is a local SQLite in-memory baseline only; it is **not** target-host validated and **not** a production-ready claim.
- Target-host preflight attempted on 2026-05-19 and **blocked** due to missing valid bearer token (see `docs/implementation-path/artifacts/2026-05-19-slo-target-preflight-blocked-evidence.md`). **Unblocked** on 2026-05-21; token installed.
- Canonical SLO target-host certification attempted on 2026-05-21 (see `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`):
  - Run #1 (default rate limits `2/50`): **FAIL** — 429 rate 46.8%
  - Run #2 (tuned rate limits `20/500`): **FAIL** — 429 rate 73.4%
  - Run #3 (max-valid rate limits `1000/10000`): **PASS** — 0 errors, 0 429s
- **SLO default-config gap closed with conservative resolution** (2026-05-21): The default safety profile (`2/50`) is intentionally conservative and remains unchanged. It is documented as unsuitable for the canonical SLO validation workload. SLO certification requires explicit high-throughput profile selection. Operator must tune based on real traffic/IP distribution. See `docs/operations/rate-limit-tuning-guide.md`.
- Abbreviated target workload executed 2026-05-21 (light load only, not full certification).
- Existing scripts:
  - `scripts/stress/run-all.sh`
  - `scripts/run_real_workload_generator.py`
  - `scripts/run_g36_workload_wrapper.sh`
  - `scripts/check_pilot_readiness.py`

## Gaps

| Gap | Why it matters | Status |
|-----|---------------|--------|
| No SLO doc | Cannot claim production posture without measurable targets. | ✅ CLOSED — draft exists and ratified for validation baseline |
| No validation runbook | Workload runs are ad-hoc; not repeatable or comparable. | ✅ CLOSED — `slo-validation-runbook.md` exists |
| No target latency percentiles | p50/p95/p99 for governance endpoints are unknown on target hardware. | ✅ PARTIAL — local baseline measured; target p99 measured under max-valid config only; default/tuned configs failed |
| No error-rate budget | Acceptable 5xx/rate-limit rate undefined. | ✅ CLOSED — pilot targets defined; 429 behavior documented per profile |
| No durability SLO | Backup age, restore success, and corruption checks not formalized. | ✅ CLOSED — targets defined; local evidence exists |
| No correctness SLO | Capability bypass, provenance gap, and scope violation targets not stated. | ✅ CLOSED — targets defined (0 tolerance) |
| No security SLO | Auth bypass and secret-leak targets not stated. | ✅ CLOSED — targets defined (0 tolerance) |
| Default config SLO certification | Default rate limits fail canonical workload | ✅ CLOSED WITH CONSERVATIVE RESOLUTION — default `2/50` remains safety-oriented; SLO certification requires explicit `1000/10000` profile; see `docs/operations/rate-limit-tuning-guide.md` |

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

- [x] SLO/SLA doc exists as a draft and was reviewed in the Phase 0 sweep.
- [x] SLO/SLA doc is ratified by operator for validation baseline (2026-05-20). **NOT a committed SLA**.
- [x] Runbook maps each script to a pass/fail gate.
- [x] At least one target workload run completed with evidence artifact (abbreviated run 2026-05-21; canonical runs 2026-05-21).
- [x] p95/p99 latency recorded for evaluate, mint, authorize, prepare, execute, verify (local baseline only; target p99 recorded under max-valid config).
- [x] Error rate recorded and under threshold (max-valid config only; default/tuned configs documented as failure evidence).
- [x] Evidence artifact reviewed and conditionally signed (2026-05-21).
- [x] Default-config gap formally resolved as conservative default with explicit operator-tuning requirement (2026-05-21).

## Evidence required

- `slo-validation-runbook.md`
- `slo-target-evidence-{date}.md`
- `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` — Canonical SLO runs (pass and fail)
- `docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md` — Local baseline
- `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` — Abbreviated target run
- Metrics snapshot (`/v1/metrics` scrape before/during/after)

## Non-claims

- **NOT a committed SLA**: These are draft SLOs for engineering planning only.
- **NOT validated for all configs**: Target workload ran under max-valid rate-limit config only. Default and tuned configs failed and are documented as failure evidence.
- **NOT production-ready evidence**: SLO definition and selective pass do not constitute production-ready claim.
- **NOT full certification**: Abbreviated target run and canonical runs are bounded evidence only. Full SLO certification requires sustained observation window (7–30 days) and operator final signoff.
- **NOT a code defect**: High 429 rates under default/tuned configs are expected behavior for those config/workload combinations, not service defects.

## Related docs

- [`slo-validation-runbook.md`](slo-validation-runbook.md) — Repeatable validation procedure
- [`docs/ROADMAP.md`](../../ROADMAP.md) §3.1, §4 Phase 2
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) — Stress baselines
- [`docs/implementation-path/57-workload-compensation-drill-plan.md`](../../implementation-path/57-workload-compensation-drill-plan.md)
