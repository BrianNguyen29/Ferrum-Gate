# SLO Default-Config Evidence — 2026-05-22

> **Status**: `FAILURE/DECISION EVIDENCE` — formalizes that the default rate-limit config intentionally fails the canonical SLO workload, and that SLO certification requires an explicit high-throughput profile.
> **Owner**: Engineering
> **Date**: 2026-05-22
> **Scope**: Single-node SQLite v1 conditional pilot
> **Parent**: [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Default config passes canonical SLO** | **NO** | Run #1 failed (46.8% 429). Default `2/50` is intentionally safety-oriented, not SLO-certification-oriented. |
| **Production-ready** | **NO** | This artifact documents a decision and failure evidence, not production readiness. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Full G2 requires default-config SLO pass **or** an accepted explicit-profile policy. This artifact provides the accepted policy rationale only. |
| **SLO certification for all configs** | **NO** | SLO certification is only claimed for the max-valid profile (`1000/10000`). |

---

## 1. Source Evidence

All measured facts below are drawn from prior evidence artifacts. No new live workload was executed for this artifact.

| Document | Date | Relevant Section |
|----------|------|------------------|
| [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) | 2026-05-21 | §3.2–3.5 — three canonical runs with default, tuned, and max-valid configs |
| [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md) | 2026-05-21 | Root-cause analysis, three-profile table, conservative invariants |
| [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) | 2026-05-18 | Rate-limit profile selection warning |
| [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) | 2026-05-21 | Conservative resolution statement |

---

## 2. Canonical Run Results (Compiled Facts)

### 2.1 Run #1 — Default Rate Limits `2/50` (FAIL)

| Metric | Value |
|--------|-------|
| Date | 2026-05-21 |
| Total requests | 2382 |
| HTTP 429 | 1114 |
| 429 rate | **46.767%** |
| Target phase p99 | 403.424 ms |
| Spike phase p99 | 378.343 ms |
| Readyz probes | All HTTP 200 |
| SLO result | **FAIL** |

### 2.2 Run #2 — Tuned Rate Limits `20/500` (FAIL)

| Metric | Value |
|--------|-------|
| Date | 2026-05-21 |
| Total requests | 2444 |
| HTTP 429 | 1795 |
| 429 rate | **73.445%** |
| Target phase p99 | 382.939 ms |
| Spike phase p99 | 305.626 ms |
| Readyz probes | All HTTP 200 |
| SLO result | **FAIL** |

### 2.3 Run #3 — Max-Valid Rate Limits `1000/10000` (PASS)

| Metric | Value |
|--------|-------|
| Date | 2026-05-21 |
| Total requests | 2380 |
| HTTP 200 | 2380 |
| HTTP 429 | 0 |
| Error rate | 0.0% |
| Target phase p99 | 394.054 ms |
| Spike phase p99 | 379.684 ms |
| Readyz probes | 47 records, all HTTP 200 |
| SLO result | **PASS** |

---

## 3. Measured Facts vs. Engineering Decisions

| # | Measured Fact | Decision |
|---|---------------|----------|
| 3.1 | Default `2/50` produced 46.8% 429 under canonical workload | Default remains unchanged. It is a safety profile, not a performance profile. |
| 3.2 | Tuned `20/500` produced 73.4% 429 (worse than default) | The tuned config is not a viable middle ground for this workload. |
| 3.3 | Max-valid `1000/10000` produced 0% 429 and 0% error | This is the only profile that passes canonical SLO certification. |
| 3.4 | All readyz probes were HTTP 200 across all runs | The service itself is healthy; failures are rate-limit enforcement, not service defects. |
| 3.5 | Per-IP token-bucket enforcement (`tower_governor`) causes the 429s | This is expected behavior for the config/workload combination. It is not a code bug. |

---

## 4. Root Cause

FerrumGate uses `tower_governor` with **per-IP** token-bucket rate limiting.
The canonical SLO validation workload (five phases: baseline → low → target → spike → cooldown)
generates sustained request volume that exceeds conservative limits when executed from a small
number of client IPs. Because the limiter is per-IP, the total server capacity is much higher
than 2 req/s, but each individual load-generator IP is capped.

Run #2 (tuned `20/500`) performed **worse** than Run #1 (default `2/50`) because the higher
sustained rate drained the burst bucket faster under per-IP enforcement, leaving less headroom
for the spike phase. This confirms the issue is a **config-vs-workload mismatch**, not a simple
"raise the limits" problem.

> **Source**: `docs/operations/rate-limit-tuning-guide.md` §Root cause: why defaults fail the canonical SLO workload.

---

## 5. Three Supported Profiles

| Profile | `rate_limit_per_second` | `rate_limit_burst` | Canonical SLO Result | When to use |
|---------|------------------------|--------------------|---------------------|-------------|
| **Default safety** | 2 | 50 | **FAIL** (expected) | Low-traffic pilots, local development, accidental-overload protection. Built-in default. |
| **SLO certification** | 1000 | 10000 | **PASS** | Running the canonical five-phase SLO validation workload. Explicit opt-in required. |
| **Production / operator-tuned** | TBD | TBD | TBD | Real deployments. Must be derived from observed traffic volume, distinct client IPs, peak RPS per IP, and backend capacity. |

---

## 6. Conservative Invariants

The following invariants are accepted and must not be broken:

1. **Do not silently change the built-in defaults to the max-valid config.**
   Default safety (`2/50`) exists for a reason: it protects a single-node pilot from
   accidental overload and from a single client IP generating excessive traffic.

2. **Do not claim SLO certification unless you explicitly used the SLO-certification
   profile or a validated operator-tuned equivalent.**

3. **Do not treat a high 429 rate under the default profile as a code defect.**
   It is expected behavior for that config/workload combination.

4. **SLO certification is an explicit operator decision, not an automatic default.**
   The operator must consciously select the `1000/10000` profile (or their own validated
   equivalent) before running the canonical workload.

> **Source**: `docs/operations/rate-limit-tuning-guide.md` §Conservative invariants (do not break).

---

## 7. Final Decision

**The default rate-limit configuration (`2/50`) is intentionally safety-oriented and remains unchanged.**

**SLO certification requires an explicit high-throughput profile (`1000/10000`).**
This is not a workaround or a bug fix; it is a documented, accepted design decision.
Operators who wish to run the canonical SLO validation workload must explicitly configure
the SLO-certification profile. Operators who wish to run production workloads must derive
and validate their own profile based on real traffic and IP distribution.

**This artifact compiles the failure evidence and the conservative decision into a single
documented record.** It closes the default-config SLO gap by formally stating:

> *Default fails by design. Pass requires explicit profile selection.*

---

## 8. Impact on Templates and Checklist

### 8.1 NO→YES Completion Plan

Steps 1.3 and 2.3 in [`2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md)
originally listed `YYYY-MM-DD-slo-default-config-pass-evidence.md` as an open prerequisite.
Those steps are now updated to reference this artifact as **failure/decision evidence**:

- Step 1.3 is updated to: `SLO default-config failure/decision evidence compiled` —
  the prerequisite for `production-ready = YES` is not a default-config pass, but rather
  an accepted explicit-profile policy (which this artifact documents).

- Step 2.3 is updated to: `SLO default-config failure/decision evidence compiled` —
  full G2 requires either a default-config pass or an accepted documented rationale for
  explicit-profile certification. This artifact provides the latter.

### 8.2 Final Production Readiness Signoff Template

[`TEMPLATE-final-production-readiness-signoff.md`](./TEMPLATE-final-production-readiness-signoff.md)
P.3 is updated to reference this artifact. The template now states that the prerequisite
is either a default-config pass or this compiled decision evidence, not an unchecked open item.

### 8.3 Full G2 Re-Signoff Template

[`TEMPLATE-full-g2-resignoff.md`](./TEMPLATE-full-g2-resignoff.md)
P.3 and G2.6 are updated to reference this artifact as the documented rationale for
explicit-profile SLO certification.

### 8.4 Evidence Checklist

[`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md)
Phase 2 already records the default-config gap as **CLOSED WITH CONSERVATIVE RESOLUTION**.
This artifact is the formal evidence compilation that supports that closure.

---

## 9. Related Docs

| Document | Purpose |
|----------|---------|
| [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md) | Operational guide with profile selection and tuning procedure |
| [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) | Canonical workload procedure and pass/fail criteria |
| [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) | SLO/SLA draft and conservative resolution statement |
| [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) | Source canonical run evidence |
| [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) | Updated to reference this artifact |
| [`docs/implementation-path/artifacts/TEMPLATE-final-production-readiness-signoff.md`](./TEMPLATE-final-production-readiness-signoff.md) | Updated to reference this artifact |
| [`docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md`](./TEMPLATE-full-g2-resignoff.md) | Updated to reference this artifact |

---

*Artifact created: 2026-05-22. SLO default-config evidence — failure/decision compilation. No production-ready claim.*
