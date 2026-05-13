# Artifact: 2026-05-13 D1–D6 Target-Host Drill Evidence

> **Type**: Evidence artifact (execution results, not a readiness claim)
> **Date**: 2026-05-13
> **Scope**: D1–D6 target-host drill pass, smoke sub-check clarification, and security remediation
> **Status**: Evidence recorded. No production-ready claim. No pilot-ready claim.

---

## Summary

This artifact documents the latest D1–D6 target-host drill execution:
- All six drills (D1–D6) passed overall.
- Applicable lineage steps (compile, evaluate, mint, authorize, prepare, execute, compensate, capture) passed.
- Post-run readiness probes (readyz / deep) returned HTTP 200.
- The runner-reported "Server smoke" status remained FAIL due to a standalone-runner limitation (aggregation/import context), not due to actual server unavailability.
- Manual, token-safe smoke sub-checks performed after the run confirmed functional readiness.
- A bearer token was rotated after accidental exposure during a readiness script execution; post-rotation verification succeeded.
- Secret scan on the local artifact directory found no committed secrets.

---

## Run Metadata

| Field | Value |
|-------|-------|
| Total drills | 6 |
| Passed | 6 |
| Partial | 0 |
| Failed | 0 |
| Latest commits | `840e379`, `3fd8e30` |
| SSH firewall restored to | `118.69.4.63/32` |
| Post-run readyz | HTTP 200 |
| Post-run deep | HTTP 200 |

---

## D1–D6 Drill Results

All drills (D1, D2, D3, D4, D5, D6) reported **overall PASS**.

### Applicable Steps (all passed)

For each drill, the following steps passed where applicable:
- `compile`
- `evaluate`
- `mint`
- `authorize`
- `prepare`
- `execute`
- `compensate`
- `capture`

### Verify Step Behavior

The `verify` step was **skipped by design** for compensation drills.
**Reason**: The verify transition moves the contract away from `ExecutedAwaitingVerify`; executing it would block the compensation path, which is the intended test behavior for those drills.

---

## Smoke Check Clarification

### Runner-Reported Status

The standalone runner at `/tmp` reported **Server smoke FAIL**.

**Root cause (aggregation/import context)**:
- The standalone runner could not import or check the smoke context.
- The runner did not persist `server_smoke_output.txt`, so the aggregation step had no file to evaluate.
- This is a runner-side artifact limitation, not evidence of server unavailability or readiness probe failure.

### Manual Token-Safe Smoke Sub-Checks (Post-Run)

The following sub-checks were performed manually after D1–D6 completion. No token values or prefixes are recorded.

| Check | Result |
|-------|--------|
| Shallow readiness (`ferrumctl readiness`) | Exit 0 |
| Deep readiness (`ferrumctl readiness --deep`) | Exit 0 |
| Functional auth endpoint (`GET /v1/approvals?limit=1`) | HTTP 200 |
| Metrics: `ferrumgate_write_queue_depth` found | Yes |
| Metrics: `method="GET"` label found | Yes |

**Interpretation**: The manual sub-checks demonstrate that the actual readiness probes and functional endpoints were operational at the time of verification. The runner FAIL should be read as an aggregation/import gap, not a server readiness failure.

---

## Security Remediation: Token Rotation

### Incident

A bearer token was accidentally exposed during readiness script execution.

### Remediation

1. The exposed bearer token was **rotated**.
2. Post-rotation verification:
   - `AUTH_STATUS`: 200
   - `DEEP_STATUS`: 200

No token value, prefix, or authorization header is recorded in this artifact.

---

## Secret Scan Results

A secret scan was run against the local artifact directory (`/tmp/ferrum-d1-d6-live-3fd8e30-d3remote`).

| Pattern | Result |
|---------|--------|
| `FERRUMD_BEARER_TOKEN` | NOT FOUND |
| `dummy_secret` | NOT FOUND |
| `Bearer dummy` | NOT FOUND |
| `Authorization: Bearer` | NOT FOUND |

No secrets were present in the artifact directory at scan time.

---

## Conservative Claims & Non-Claims

### What This Evidence Supports

- D1–D6 target-host drills executed and passed.
- Applicable lineage steps behaved as designed (including verify-skip for compensation drills).
- Post-run readiness probes returned HTTP 200.
- Manual smoke sub-checks confirmed shallow readiness, deep readiness, and functional endpoint availability.
- Token rotation was completed successfully after an exposure incident.

### What This Evidence Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| G2 complete | **NO** | Not evaluated or claimed. |
| G3.6 full accepted | **NO** | Not evaluated or claimed. |
| Conditional SQLite pilot ready | **NO** | Drills pass is necessary, not sufficient. |
| Production-ready | **NO** | No production-ready claim is made. |

---

## Next Actions

| Action | Owner | Notes |
|--------|-------|-------|
| Root-cause the runner smoke aggregation/import gap | Engineering | Prevent future false-negative smoke status. |
| Decide whether to gate on runner smoke or manual sub-checks | Operator + Engineering | Document the canonical smoke contract. |
| Evaluate G3.6 acceptance criteria separately | Operator + Engineering | Drill pass is one input; not sufficient alone. |

---

*Artifact created: 2026-05-13. Evidence only — no secrets, no token values, no production-ready claim, no pilot-ready claim.*
