# Rate-Limit Tuning Guide

> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Scope**: Single-node SQLite

---

## Goal

Help operators choose an appropriate `tower_governor` per-IP rate-limit
configuration for their FerrumGate deployment, and explain why the built-in
defaults are conservative.

## Notes

- This doc covers rate-limit selection only. Backend capacity, connection pooling, and store tuning are separate topics.
- The numbers below are derived from a specific canonical workload. Your actual safe limits depend on traffic patterns, client IP distribution, and hardware.

---

## Root cause: why defaults fail high-throughput validation

FerrumGate uses two `tower_governor` token-bucket governors on the workload
router:

1. **Outer pre-auth governor** — IP-only bucketing based on the resolved client
   IP. It runs before authentication and is always enabled.
2. **Inner post-auth governor** — `AuthActor` + IP bucketing. It runs after
   authentication for `scoped`, `oidc`, and `agent` auth modes and prevents a
   single IP from carrying too many distinct authenticated identities.

`disabled` and `bearer` auth modes use only the outer governor. Monitoring
routes (`/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`) bypass
both governors.

The built-in defaults are:

```toml
rate_limit_per_second = 2
rate_limit_burst = 50
```

These defaults are **intentionally safety-oriented**. They protect a
single-node deployment from accidental overload and from a single client IP
generating excessive traffic.

### Trusted proxy and client IP resolution

The resolved client IP is normally the immediate transport peer. If `ferrumd`
is behind an operator-owned reverse proxy, set `trusted_proxy_cidrs` so that a
trusted immediate peer may supply the real client address via a single
`X-Real-IP` header:

- `trusted_proxy_cidrs` defaults to `[]` (trust-none). This is the safest
  default and the recommended starting point.
- Only immediate peers inside a configured CIDR may supply `X-Real-IP`.
  Malformed, multiple, or missing values fall back to the peer IP.
- `X-Forwarded-For` is ignored entirely.
- Universal CIDRs (`0.0.0.0/0`, `::/0`) are rejected at startup.
- To revert to peer-IP bucketing, set `trusted_proxy_cidrs = []` or omit it.

A canonical validation workload (five steps: baseline → low → target →
spike → cooldown) generates sustained request volume that exceeds conservative
limits when executed from a small number of client IPs. Because the limiter is
per-IP, the total server capacity is much higher than 2 req/s, but each
individual load-generator IP is capped.

**Canonical validation run results:**

| Run | Config | 429 rate | Result |
|-----|--------|----------|--------|
| #1 | Default `2/50` | 46.8% | **FAIL** |
| #2 | Tuned `20/500` | 73.4% | **FAIL** |
| #3 | Max-valid `1000/10000` | 0% | **PASS** |

Run #2 (tuned 20/500) performed **worse** than Run #1 (default 2/50) because
the higher sustained rate drained the burst bucket faster under per-IP
enforcement, leaving less headroom for the spike step. This confirms the
issue is a config-vs-workload mismatch, not a simple "raise the limits"
problem.

---

## Three supported profiles

| Profile | `rate_limit_per_second` | `rate_limit_burst` | When to use |
|---------|------------------------|--------------------|-------------|
| **Default safety** | 2 | 50 | Low-traffic local development, accidental-overload protection. This is the built-in default. |
| **High-throughput** | 1000 | 10000 | Running the canonical five-step validation workload. |
| **Deployment-specific** | TBD | TBD | Derive from observed traffic volume, number of distinct client IPs, peak RPS per IP, and backend capacity. |

---

## How to select an operator-tuned profile

1. **Measure real traffic**
   - Collect per-IP request rates from your reverse-proxy logs or metrics.
   - Identify the p99 per-IP sustained RPS and peak burst per IP.

2. **Add headroom**
   - Set `rate_limit_per_second` to at least 2× the observed p99 per-IP
      sustained RPS.
   - Set `rate_limit_burst` to at least 2× the observed peak burst per IP.

3. **Validate under load**
   - Run a representative workload (e.g., the canonical steps or your own
      stress test) and verify 429 rate is acceptable for your deployment
      (e.g., < 5%).
   - Monitor `ferrumgate_governance_errors_total{status="429"}` in
      `/v1/metrics`.

4. **Revisit after topology changes**
    - If you add more load-balancer IPs (NAT), the effective per-IP limit
      becomes more restrictive because all traffic behind a single NAT IP
      shares the outer bucket.  For `scoped`, `oidc`, and `agent` auth modes the
      inner `AuthActor`+IP governor isolates distinct authenticated actors
      behind the same IP, so those clients are not affected by anonymous noisy
      neighbors.  In `bearer` or `disabled` mode there is no inner governor;
      all traffic behind the same IP shares the outer bucket.
    - If authenticated clients still need higher limits, consider raising
      the per-second/burst values or deploying additional gateway nodes.
    - There is no operator-facing toggle for the inner governor; when the auth
      mode enables it, it is always on.

---

## Configuring rate limits

Rate limits can be set via CLI, environment variable, or config file.
Precedence: CLI > env > config file > defaults.

### CLI flags

```bash
ferrumd \
  --rate-limit-per-second 1000 \
  --rate-limit-burst 10000 \
  --trusted-proxy-cidrs "10.0.0.0/8,172.16.0.0/12" \
  --pre-auth-rate-limit-per-second 100 \
  --pre-auth-rate-limit-burst 200
```

### Environment variables

```bash
export FERRUMD_TRUSTED_PROXY_CIDRS="10.0.0.0/8,172.16.0.0/12"
export FERRUMD_RATE_LIMIT_PER_SECOND=1000
export FERRUMD_RATE_LIMIT_BURST=10000
export FERRUMD_PRE_AUTH_RATE_LIMIT_PER_SECOND=100
export FERRUMD_PRE_AUTH_RATE_LIMIT_BURST=200
```

### Config file

```toml
[server]
trusted_proxy_cidrs = []
rate_limit_per_second = 1000
rate_limit_burst = 10000
pre_auth_rate_limit_per_second = 100
pre_auth_rate_limit_burst = 200
```

When `pre_auth_rate_limit_*` is omitted, the outer governor inherits the
`rate_limit_*` values. Precedence is the same as all other config:
CLI > env > config file > defaults.

Validation rules:
- `rate_limit_per_second` must be > 0
- `rate_limit_burst` must be > 0 and ≤ 10000
- `pre_auth_rate_limit_per_second` must be ≥ 1 when set
- `pre_auth_rate_limit_burst` must be ≥ 1 and ≤ 10000 when set
- `trusted_proxy_cidrs` entries must be valid CIDRs; universal CIDRs
  (`0.0.0.0/0`, `::/0`) are rejected at startup

---

## Conservative invariants (do not break)

- Do **not** silently change the built-in defaults to the max-valid config.
  Default safety exists for a reason.
- Do **not** treat a high 429 rate under the default profile as a code defect.
  It is expected behavior for that config/workload combination.

---

## Related docs

- [`configs/examples/nonprod-ferrumgate.toml`](../../configs/examples/nonprod-ferrumgate.toml) — Example config with rate-limit comments

---

*Rate-limit tuning guide.*
