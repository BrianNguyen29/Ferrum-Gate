# SLO Target-Host Preflight Evidence — 2026-05-19

## Status

- **Scope**: Target-host SLO validation preflight only.
- **Verdict**: 🚫 BLOCKED — preflight did not pass; no workload executed.
- **Target-host validated**: NO.
- **Production-ready**: NO.
- **SLO ratified**: NO — targets remain draft/conditional.
- **Operator signoff**: NOT OBTAINED.

This artifact records an **attempted** target-host validation preflight against the
public DuckDNS endpoint. It does **not** claim that SLOs are met, that the target
is validated, or that FerrumGate is production-ready. The preflight was blocked
before any workload or stress execution per the runbook stop criteria.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Host scope | Target host (public DuckDNS) |
| Target URL | `https://ferrumgate.duckdns.org` |
| Store backend | Unknown from probes (deep readiness body parsing skipped) |
| Auth mode | `bearer` (inferred from 401 on functional probe) |

## Safe env presence check

| Variable | Status |
|----------|--------|
| `FERRUM_BEARER_TOKEN` | missing |
| `FERRUMCTL_BEARER_TOKEN` | missing |
| `TOKEN` | missing |

No valid bearer token was available in the environment. A placeholder token was
used for pilot-readiness probes, which is expected to fail authenticated
endpoints.

## Public unauth probes

These probes require no authentication and confirm the target is reachable and
serving HTTP.

| Endpoint | HTTP status | Notes |
|----------|-------------|-------|
| `GET /v1/healthz` | 200 | Target reachable |
| `GET /v1/readyz/deep` | 200 | Deep readiness reports up |
| `GET /v1/metrics` | 200 | Metrics endpoint accessible |

Result: ✅ PASS — target is online and responding.

## Pilot readiness check (`check_pilot_readiness.py`)

### First attempt — `ferrumctl` missing from PATH

Command:

```bash
python3 scripts/check_pilot_readiness.py \
  --server-url https://ferrumgate.duckdns.org \
  --bearer-token local-disabled-auth-token
```

Observed result:

- shallow readiness: FAIL
- deep readiness: FAIL
- functional readiness: FAIL
- metrics: PASS

Root cause: `ferrumctl` binary not found in PATH.

### Second attempt — PATH includes `target/release/ferrumctl`

After adding `target/release` to PATH:

| Probe | Result | Detail |
|-------|--------|--------|
| shallow readiness | ✅ PASS | `/v1/healthz` returned 200 |
| deep readiness | ✅ PASS | `/v1/readyz/deep` returned 200; body parsing skipped (`components` field not found / not an array) |
| functional readiness | ❌ FAIL | `GET /v1/approvals?limit=1` returned **401 Unauthorized** (placeholder token) |
| metrics | ✅ PASS | `/v1/metrics` returned 200 |

**Overall**: `SOME PROBES FAILED`

## Blocker

The functional readiness probe failed with **401 Unauthorized** because the only
available token was a placeholder (`local-disabled-auth-token`). The runbook
prerequisite for a valid bearer token was not satisfied:

> `Auth token valid` — Block if 401/403

Because the preflight failed, **no workload or stress execution was attempted**.
Per the runbook:

> **Fail**: Stop. Fix environment before proceeding.

## Next action

1. Obtain a valid bearer token for the target host (operator-generated via
   `openssl rand -hex 32` or existing target secret).
2. Re-run `check_pilot_readiness.py` with the valid token.
3. If all probes pass, proceed to Step 2 (stress baseline) of the validation
   runbook.
4. If the target still returns 401 with a valid token, investigate target auth
   configuration before proceeding.

## Workload execution

**NOT EXECUTED**. The runbook was halted at Step 1 (pre-run readiness check)
because the functional probe failed. No stress suite, workload generator, or
metrics scrape beyond the probe was run against the target host.

## SLO comparison

N/A — no workload was executed; no latency percentiles or error rates were
measured against the target.

## Backup spot check

N/A — no workload was executed; no state mutation occurred.

## Anomalies and caveats

1. **Deep readiness body parsing skipped**: The deep readiness probe returned 200,
   but the JSON body did not contain a parseable `components` array. This is
   informational and did not block the preflight.
2. **Placeholder token**: `local-disabled-auth-token` is a local-development
   placeholder. It is not expected to authenticate against a target host running
   with `auth_mode=bearer`.
3. **No secrets recorded**: No real tokens, passwords, or credentials appear in
   this artifact.
4. **NOT target-host validated**: This artifact records only a blocked preflight.
   It does not validate SLOs, performance, or correctness on the target host.

## Operator signoff

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Operator reviewer | | | |

> **Blank until reviewed**. This artifact is a blocked preflight record and has
> not been reviewed or signed off by an operator.

## Related docs

- [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) — Draft SLO targets and non-claims
- [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) — Repeatable validation procedure
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) — Evidence checklist
- [`docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md) — Local baseline evidence (separate; not target-host)
