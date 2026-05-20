# SLO Validation Runbook

> **Status**: Draft procedure. Targets are draft/conditional until ratified by operator.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-18
> **Parent**: [`01-slo-sla.md`](01-slo-sla.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Provide a repeatable, script-mapped procedure to validate FerrumGate SLO targets.
This runbook defines the steps, pass/fail criteria, and evidence format.
It does **not** constitute evidence that SLOs are met — execution and artifact
production are required separately.

## Non-claims

- **NOT validated**: No workload has been executed against these thresholds yet.
- **NOT production-ready evidence**: This runbook is a procedure document only.
- **NOT ratified**: SLO targets remain draft until an operator reviews workload
evidence and signs off.
- **NOT target-host validated**: Execution against a live target host is out of
scope for this document; it must be performed separately and recorded in the
evidence artifact.

## Prerequisites

| Check | How | Gate |
|-------|-----|------|
| Server reachable | `curl ${BASE_URL}/v1/healthz` | Block if not 200 |
| Auth token valid | `curl -H "Authorization: Bearer ${TOKEN}" ${BASE_URL}/v1/approvals?limit=1` | Block if 401/403 |
| Metrics endpoint up | `curl ${BASE_URL}/v1/metrics` | Block if not 200 |
| Store backend healthy | Deep readiness probe | Block if `/v1/readyz/deep` ≠ 200 |
| Config frozen for run | Document `rate_limit_per_second`, `rate_limit_burst`, store mode | Warn if drift likely |

## SLO targets (draft — to be ratified)

Use the table from [`01-slo-sla.md`](01-slo-sla.md). The pilot-tier targets are
the validation baseline for the first run:

| Group | Metric | Pilot SLO |
|-------|--------|-----------|
| Availability | `/v1/healthz` uptime | 99.0% |
| Availability | `/v1/readyz/deep` uptime | 99.0% |
| Latency | evaluate p99 | < 500ms |
| Latency | mint p99 | < 500ms |
| Latency | execute pipeline p99 | < 5s |
| Error rate | 5xx rate | < 1% |
| Error rate | 429 rate | < 5% |
| Durability | backup age | < 15min |
| Durability | restore success | 100% |
| Correctness | capability bypass | 0 |
| Correctness | provenance gap | 0 |
| Correctness | scope violation | 0 |
| Security | auth bypass | 0 |
| Security | secret leak in output/logs | 0 |
| Operational | incident acknowledgement | < 1h |

> These targets are draft/conditional. Do not cite them as committed SLAs until
> a validation run produces evidence and an operator ratifies them.

## Script map

| Script | Purpose | SLO groups covered | Pass criteria |
|--------|---------|-------------------|---------------|
| [`scripts/stress/run-all.sh`](../../scripts/stress/run-all.sh) | Master stress suite (health, auth, intent-compile, SQLite contention, rate-limit) | Availability, Latency, Error rate | All sub-scenarios exit 0; error pct < 1%; no 5xx during run |
| [`scripts/run_real_workload_generator.py`](../../scripts/run_real_workload_generator.py) | G3.6 workload generator with phased RPS | Latency, Error rate, Availability | `--plan` produces valid plan; `--execute` completes all phases; p99 < thresholds; 5xx < 1%; 429 < 5% |
| [`scripts/run_g36_workload_wrapper.sh`](../../scripts/run_g36_workload_wrapper.sh) | Robust wrapper with drift probes, sentinel, revert | Operational (config stability), Error rate (abort on drift) | Pre-run E-check passes; generator exits 0; no drift abort; sentinel = COMPLETE.status |
| [`scripts/check_pilot_readiness.py`](../../scripts/check_pilot_readiness.py) | Shallow/deep/functional readiness + metrics probe | Availability, Operational | All probes PASS; `/v1/readyz/deep` returns 200 with store + write_queue (+ pool for backends that expose pool status); required metrics present |

## Workload phases

The canonical validation sequence uses five phases. Durations and rates can be
adjusted per environment, but the phase order must be preserved.

| Phase | Duration | Rate (rps) | Purpose |
|-------|----------|------------|---------|
| baseline | 600 s | 0.0 | Establish idle metrics; verify no background errors |
| low | 600 s | 0.1 | Warm-up and early latency sampling |
| target | 1800 s | 1.0 | Primary measurement window for p50/p95/p99 |
| spike | 300 s | 5.0 | Burst tolerance and rate-limit behavior |
| cooldown | 600 s | 0.0 | Recovery observation; verify return to baseline |

> These are the default phases used by `run_real_workload_generator.py` and the
> wrapper. Custom phases may be supplied via `--phases` JSON.

## Validation procedure

### Step 1 — Pilot readiness check

Run before any workload to confirm the environment is stable:

```bash
export FERRUM_BEARER_TOKEN="<token>"
python3 scripts/check_pilot_readiness.py \
  --server-url https://<host> \
  --bearer-token "${FERRUM_BEARER_TOKEN}"
```

- **Pass**: All probes report PASS.
- **Fail**: Stop. Fix environment before proceeding.
- **Evidence**: Capture full output.

### Step 2 — Stress baseline

Run the stress suite at low intensity to confirm no regressions:

```bash
export BASE_URL=https://<host>
export TOKEN="<token>"
export DURATION=10
export WORKERS=10
bash scripts/stress/run-all.sh
```

- **Pass**: All scenarios exit 0.
- **Fail**: Stop. Investigate errors or latency outliers.
- **Evidence**: Summary table from stdout.

### Step 3 — Generate workload plan

Produce a plan without sending live requests:

```bash
python3 scripts/run_real_workload_generator.py \
  --plan \
  --server-url https://<host> \
  --output-dir /tmp/ferrum-slo-plan-$(date +%Y%m%d)
```

- **Pass**: `workload_plan.json` and `workload_plan.md` are created and reviewed.
- **Fail**: Stop. Fix generator invocation or adapter mix.
- **Evidence**: `workload_plan.md` human-readable plan.

### Step 4 — Execute workload (operator signoff required)

> **WARNING**: This sends live requests. Requires operator awareness.
> Do not run against production without explicit approval.

Option A — Direct generator (single-node, no drift monitoring):

```bash
python3 scripts/run_real_workload_generator.py \
  --execute \
  --server-url https://<host> \
  --bearer-token "${FERRUM_BEARER_TOKEN}" \
  --output-dir /tmp/ferrum-slo-run-$(date +%Y%m%d_%H%M%S)
```

Option B — Wrapper with drift probes and sentinel (recommended):

> **Note**: The wrapper default phases omit `spike`. For the canonical five-phase
> SLO sequence, pass `--phases` explicitly:

```bash
bash scripts/run_g36_workload_wrapper.sh \
  --server-url https://<host> \
  --rate-limit-ps 1.0 \
  --rate-limit-burst 100 \
  --output-dir /tmp/ferrum-slo-run-$(date +%Y%m%d_%H%M%S) \
  --require-revert-command \
  --revert-command "systemctl restart ferrumd || true" \
  --phases '[{"name":"baseline","duration_sec":600,"rate_rps":0},{"name":"low","duration_sec":600,"rate_rps":0.1},{"name":"target","duration_sec":1800,"rate_rps":1.0},{"name":"spike","duration_sec":300,"rate_rps":5.0},{"name":"cooldown","duration_sec":600,"rate_rps":0}]'
```

- **Pass**: Generator exits 0; sentinel = COMPLETE.status; p99 latencies under draft thresholds; 5xx < 1%; 429 < 5%.
- **Fail**: Generator exits non-zero; sentinel = FAILED.status; drift abort; latency or error rate exceeds draft threshold.
- **Evidence**:
  - `workload_results.json`
  - `workload_results.md`
  - `readyz_probe_log.json`
  - `checkpoint_phase_*.json`
  - `RUN_SUMMARY.txt`
  - `generator_stdout.log`
  - `generator_stderr.log`

### Step 5 — Post-run readiness check

Repeat Step 1 after cooldown to confirm the environment recovered:

```bash
python3 scripts/check_pilot_readiness.py \
  --server-url https://<host> \
  --bearer-token "${FERRUM_BEARER_TOKEN}"
```

- **Pass**: All probes PASS (same criteria as Step 1).
- **Fail**: Document degradation; consider whether run invalidates evidence.
- **Evidence**: Capture full output.

### Step 6 — Metrics scrape

Scrape `/v1/metrics` before, during (if possible), and after the run:

```bash
curl -s -H "Authorization: Bearer ${FERRUM_BEARER_TOKEN}" \
  https://<host>/v1/metrics > metrics_postrun.txt
```

- **Evidence**: `metrics_prerun.txt`, `metrics_postrun.txt` (mid-run optional).

### Step 7 — Backup/restore spot check (durability SLO, backend-specific)

For SQLite or file-backed stores, if the run mutated state, perform a backup and restore drill. For PostgreSQL, refer to the existing PG-3 restore drill evidence instead of re-running.

```bash
# SQLite / file-backed only
ferrumctl backup create --output /tmp/slo-backup-$(date +%Y%m%d_%H%M%S).db
ferrumctl backup verify --db-path /tmp/slo-backup-*.db
```

- **Pass**: Backup age < 15 min; verify returns 0; restore success = 100%.
- **Fail**: Document backup age and verify result.
- **Evidence**: Backup path, verify output, timestamp, or reference to `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md` for PostgreSQL.

## Evidence artifact template

After completing the procedure, create the evidence artifact:

**Path**: `docs/implementation-path/artifacts/YYYY-MM-DD-slo-target-evidence.md`

Template sections:

1. **Run metadata** — date, operator, server URL, store backend, config snapshot.
2. **Pre-run checks** — pilot readiness output, stress summary, plan review.
3. **Phase results** — per-phase request count, latency p50/p95/p99, status distribution, connection counts.
4. **SLO comparison table** — metric, observed value, draft target, pass/fail.
5. **Readiness post-check** — recovery confirmation.
6. **Metrics snapshot** — link to `metrics_prerun.txt` and `metrics_postrun.txt`.
7. **Backup spot check** — age, verify result.
8. **Anomalies and aborts** — any drift, signals, or threshold breaches.
9. **Operator signoff** — signature block (blank until signed).

> The artifact must remain marked **DRAFT / PENDING SIGNOFF** until an operator
> reviews and signs.

## Pass/fail criteria summary

| Gate | Criteria | Evidence |
|------|----------|----------|
| Pre-run readiness | All probes PASS | `check_pilot_readiness.py` output |
| Stress baseline | All scenarios PASS | `run-all.sh` summary table |
| Plan validity | Valid JSON + Markdown plan | `workload_plan.json` + `.md` |
| Generator completion | Exit 0, COMPLETE.status | `RUN_SUMMARY.txt` |
| Latency p99 evaluate | < 500 ms (pilot draft) | `workload_results.json` |
| Latency p99 mint | < 500 ms (pilot draft) | `workload_results.json` |
| Latency p99 execute pipeline | < 5 s (pilot draft) | `workload_results.json` |
| Error rate 5xx | < 1% (pilot draft) | `workload_results.json` |
| Error rate 429 | < 5% (pilot draft) | `workload_results.json` |
| Config drift | Zero drift aborts | `config_drift_log.jsonl` |
| Post-run readiness | All probes PASS | `check_pilot_readiness.py` output |
| Backup age | < 15 min | Backup timestamp |
| Backup verify | Exit 0 | `ferrumctl backup verify` output |

## Related docs

- [`01-slo-sla.md`](01-slo-sla.md) — Draft SLO targets and non-claims
- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Stress baselines
- [`docs/implementation-path/57-workload-compensation-drill-plan.md`](../../docs/implementation-path/57-workload-compensation-drill-plan.md) — Workload drill context
- [`docs/implementation-path/116-g36-monitoring-execution-plan.md`](../../docs/implementation-path/116-g36-monitoring-execution-plan.md) — G3.6 execution plan
- [`docs/ROADMAP.md`](../ROADMAP.md) — Parent roadmap with full phase plan.
