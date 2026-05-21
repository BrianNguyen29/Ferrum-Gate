# Rate-Limit Tuning Guide

> **Status**: Operational guide. No production-ready claim.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Scope**: Single-node SQLite v1 conditional pilot

---

## Goal

Help operators choose an appropriate `tower_governor` per-IP rate-limit
configuration for their FerrumGate deployment, and explain why the built-in
defaults are conservative.

## Non-claims

- **NOT a performance tuning guide for production**: This doc covers rate-limit
  selection only. Backend capacity, connection pooling, and store tuning are
  separate topics.
- **NOT a guarantee**: The numbers below are derived from a specific canonical
  workload. Your actual safe limits depend on traffic patterns, client IP
  distribution, and hardware.
- **NOT production-ready**: This guide does not make FerrumGate production-ready.

---

## Root cause: why defaults fail the canonical SLO workload

FerrumGate uses `tower_governor` with **per-IP** token-bucket rate limiting.
The built-in defaults are:

```toml
rate_limit_per_second = 2
rate_limit_burst = 50
```

These defaults are **intentionally safety-oriented**. They protect a
single-node pilot from accidental overload and from a single client IP
generating excessive traffic.

The canonical SLO validation workload (five phases: baseline → low → target →
spike → cooldown) generates sustained request volume that exceeds conservative
limits when executed from a small number of client IPs. Because the limiter is
per-IP, the total server capacity is much higher than 2 req/s, but each
individual load-generator IP is capped.

**Canonical SLO run results:**

| Run | Config | 429 rate | Result |
|-----|--------|----------|--------|
| #1 | Default `2/50` | 46.8% | FAIL |
| #2 | Tuned `20/500` | 73.4% | FAIL |
| #3 | Max-valid `1000/10000` | 0% | PASS |

Run #2 (tuned 20/500) performed **worse** than Run #1 (default 2/50) because
the higher sustained rate drained the burst bucket faster under per-IP
enforcement, leaving less headroom for the spike phase. This confirms the
issue is a config-vs-workload mismatch, not a simple "raise the limits"
problem.

---

## Three supported profiles

| Profile | `rate_limit_per_second` | `rate_limit_burst` | When to use |
|---------|------------------------|--------------------|-------------|
| **Default safety** | 2 | 50 | Low-traffic pilots, local development, accidental-overload protection. This is the built-in default. |
| **SLO certification** | 1000 | 10000 | Running the canonical five-phase SLO validation workload from `slo-validation-runbook.md`. Required to pass. |
| **Production / operator-tuned** | TBD | TBD | Real deployments. Must be derived from observed traffic volume, number of distinct client IPs, peak RPS per IP, and backend capacity. |

---

## How to select a production profile

1. **Measure real traffic**
   - Collect per-IP request rates from your reverse-proxy logs or metrics.
   - Identify the p99 per-IP sustained RPS and peak burst per IP.

2. **Add headroom**
   - Set `rate_limit_per_second` to at least 2× the observed p99 per-IP
     sustained RPS.
   - Set `rate_limit_burst` to at least 2× the observed peak burst per IP.

3. **Validate under load**
   - Run a representative workload (e.g., the canonical phases or your own
     stress test) and verify 429 rate is within your operational target
     (e.g., < 5%).
   - Monitor `ferrumgate_governance_errors_total{status="429"}` in
     `/v1/metrics`.

4. **Revisit after topology changes**
   - If you add more load-balancer IPs (NAT), the effective per-IP limit
     becomes more restrictive because all traffic behind a single NAT IP
     shares one bucket. In that case, raise limits or switch to a
     header-based client identifier if supported by your infrastructure.

---

## Configuring rate limits

Rate limits can be set via CLI, environment variable, or config file.
Precedence: CLI > env > config file > defaults.

### CLI flags

```bash
ferrumd \
  --rate-limit-per-second 1000 \
  --rate-limit-burst 10000
```

### Environment variables

```bash
export FERRUMD_RATE_LIMIT_PER_SECOND=1000
export FERRUMD_RATE_LIMIT_BURST=10000
```

### Config file

```toml
[server]
rate_limit_per_second = 1000
rate_limit_burst = 10000
```

Validation rules:
- `rate_limit_per_second` must be > 0
- `rate_limit_burst` must be > 0 and ≤ 10000

---

## Conservative invariants (do not break)

- Do **not** silently change the built-in defaults to the max-valid config.
  Default safety exists for a reason.
- Do **not** claim SLO certification unless you explicitly used the
  SLO-certification profile or a validated operator-tuned equivalent.
- Do **not** treat a high 429 rate under the default profile as a code defect.
  It is expected behavior for that config/workload combination.

---

## Related docs

- [`slo-validation-runbook.md`](../production-readiness-v2/slo-validation-runbook.md) — Canonical workload procedure and pass/fail criteria
- [`configs/examples/nonprod-ferrumgate.toml`](../../configs/examples/nonprod-ferrumgate.toml) — Example config with rate-limit comments
- [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](../implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md) — Run evidence
- [`docs/implementation-path/122-completion-roadmap-and-hardening-tracker.md`](../implementation-path/122-completion-roadmap-and-hardening-tracker.md) — Tracker closure

---

*Guide created: 2026-05-21. Rate-limit tuning guide — operational artifact only.*
