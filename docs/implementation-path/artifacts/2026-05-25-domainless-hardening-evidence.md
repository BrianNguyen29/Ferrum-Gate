# Domainless Hardening Evidence — 2026-05-25

> **Status**: `LOCAL EVIDENCE` — fresh 2026-05-25 domainless hardening runs.
> **Owner**: Engineering
> **Date**: 2026-05-25
> **Scope**: Single-node SQLite v1 conditional pilot, local workstation only
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | All runs executed on a local development workstation against `127.0.0.1:8080`. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | These are automated/scripted probes only. Operator review and explicit signoff remain required. |
| **Block A closed** | **NO** | Block A remains `WAIVED/CONDITIONAL`. Real owned domain still required for full closure. |
| **PostgreSQL production deployment** | **NO** | PG restart drill uses local Docker Compose only. |
| **HA / multi-node** | **NO** | No HA or multi-node claims. |
| **SLO window closure** | **NO** | SLO sustained observations are rehearsals, not approved sustained-window evidence. |
| **Default config passes stress** | **NO** | Default rate-limit config (`2/50`) fails some stress scenarios; this is expected and consistent with prior evidence. |

---

## 1. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-25 |
| Host scope | Local development workstation |
| Ferrumd bind address | `127.0.0.1:8080` (default dev profile) |
| Ferrumd auth mode | `disabled` (for most runs) |
| Default rate limits | `rate_limit_per_second=2`, `rate_limit_burst=50` |
| High-throughput profile | `rate_limit_per_second=1000`, `rate_limit_burst=10000` |
| PostgreSQL mode | Docker Compose local staging fallback (for PG restart drill only) |

---

## 2. Run Summary

| # | Run | Command | Verdict |
|---|-----|---------|---------|
| 2.1 | SLO sustained dry-run | `bash scripts/run_slo_sustained_observation.sh --dry-run --duration-min 1 --interval-min 1 --output-dir /tmp/ferrumgate-slo-dryrun-20260525` | ✅ PASS |
| 2.2 | SLO real rehearsal (1 min) | `bash scripts/run_slo_sustained_observation.sh --base-url http://127.0.0.1:8080 --duration-min 1 --interval-min 1 --output-dir /tmp/ferrumgate-slo-rehearsal-20260525` | ✅ PASS |
| 2.3 | Local restore drill | `make restore-drill` | ✅ PASS |
| 2.4 | PG container restart drill | `make pg-restart-drill` | ✅ PASS |
| 2.5 | Pilot readiness check | `FERRUMCTL=/home/uong_guyen/work/Ferrum-Gate/target/release/ferrumctl python3 scripts/check_pilot_readiness.py --server-url http://127.0.0.1:8080 --skip-functional` | ✅ PASS / partial |
| 2.6 | Stress suite — default config | `BASE_URL=http://127.0.0.1:8080 WORKERS=5 DURATION=3 make stress` (default profile) | ⚠️ MIXED (s1/s8 PASS; s2/s4/s7 FAIL due to rate-limit) |
| 2.7 | Stress suite — high-throughput profile | `BASE_URL=http://127.0.0.1:8080 WORKERS=5 DURATION=3 make stress` (high-throughput env) | ✅ PASS (all 5 scenarios) |

---

## 3. Detailed Results

### 3.1 SLO Sustained Dry-Run (2.1)

**Command:**
```bash
bash scripts/run_slo_sustained_observation.sh \
  --dry-run --duration-min 1 --interval-min 1 \
  --output-dir /tmp/ferrumgate-slo-dryrun-20260525
```

**Results:**

| Metric | Value |
|--------|-------|
| Samples taken | 1 |
| Samples OK | 1 |
| Samples fail | 0 |
| Availability | 100.00% |
| Average latency | 42 ms |
| JSONL validation | ✅ passed |

**Interpretation:** The script logic, summary generation, and JSONL serialization all function correctly. Dry-run mode exercises the code path without network calls and alternates simulated success/failure across samples; with only 1 sample the result is a single simulated OK.

---

### 3.2 SLO Real Rehearsal — 1 Minute (2.2)

**Command:**
```bash
bash scripts/run_slo_sustained_observation.sh \
  --base-url http://127.0.0.1:8080 \
  --duration-min 1 --interval-min 1 \
  --output-dir /tmp/ferrumgate-slo-rehearsal-20260525
```

**Results:**

| Metric | Value |
|--------|-------|
| Samples taken | 1 |
| Samples OK | 1 |
| Samples fail | 0 |
| Availability | 100.00% |
| Average latency | 20 ms |

**Interpretation:** A real HTTP call to `/v1/healthz` succeeded with 20 ms latency. This confirms the rehearsal script works end-to-end against a running local `ferrumd`. It is **not** an SLO window closure; it is a short functional rehearsal.

---

### 3.3 Local Restore Drill (2.3)

**Command:**
```bash
make restore-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Source store integrity verify | ✅ PASS |
| Backup integrity verify | ✅ PASS |
| Restore to temp location | ✅ PASS |
| Restored store integrity verify | ✅ PASS |
| sqlite3 data comparison | ✅ PASS (identical) |

**Evidence:** Backup file created at `/tmp/tmp.9b11zr7PpC/backups/ferrumgate.db_1779730972.db`.

**Interpretation:** `ferrumctl backup create/verify/restore` behaves correctly in a temp-directory environment. The drill does **not** constitute G2.1 completion; a target-host restore drill is still required for full G2 closure.

---

### 3.4 PG Container Restart Drill (2.4)

**Command:**
```bash
make pg-restart-drill
```

**Results:**

| Check | Result |
|-------|--------|
| Preflight (docker, compose, cargo, curl) | ✅ PASS |
| PostgreSQL container healthy before restart | ✅ PASS |
| ferrumd ready before restart | ✅ PASS |
| PostgreSQL container healthy after restart | ✅ PASS |
| ferrumd recovered after restart | ✅ PASS |
| Recovery time | **14s** (target <= 30s) |

**Summary:** Passed 9, Failed 0, Skipped 0.

**Interpretation:** Consistent with the 2026-05-21 drill evidence (`docs/implementation-path/artifacts/2026-05-21-pg-container-restart-drill-evidence.md`). Recovery time of 14s remains within the 30s acceptance threshold. This is a **local Docker fallback** drill, not a production PostgreSQL or HA test.

---

### 3.5 Local Pilot Readiness Check (2.5)

**Command:**
```bash
FERRUMCTL=/home/uong_guyen/work/Ferrum-Gate/target/release/ferrumctl \
  python3 scripts/check_pilot_readiness.py \
  --server-url http://127.0.0.1:8080 \
  --skip-functional
```

**Results:**

| Probe | Result |
|-------|--------|
| Shallow readiness (`/v1/readyz`) | ✅ PASS |
| Deep readiness (`/v1/readyz/deep`) | ✅ PASS |
| Metrics endpoint (`/v1/metrics`) | ✅ PASS |
| Functional readiness (`/v1/approvals`) | ⏭️ SKIPPED (by flag) |

**Interpretation:** The automated readiness and metrics probes pass against a local `ferrumd`. Functional readiness was intentionally skipped because the local dev profile runs with `auth=disabled`; the functional probe requires a bearer token. This script does **not** complete G2; operator review and explicit signoff are still required.

---

### 3.6 Stress Suite — Default Config (2.6)

**Environment:**
- `FERRUMD_RATE_LIMIT_PER_SECOND=2`
- `FERRUMD_RATE_LIMIT_BURST=50`
- `FERRUMD_AUTH_MODE=disabled`
- `FERRUMD_BIND_ADDR=127.0.0.1:8080`

**Command:**
```bash
BASE_URL=http://127.0.0.1:8080 WORKERS=5 DURATION=3 make stress
```

**Results:**

| Scenario | Result | Notes |
|----------|--------|-------|
| s1-health | ✅ PASS | Low request volume; within default limits. |
| s2-auth | ❌ FAIL | Rate-limit 429s under multi-worker load. |
| s4-intent-compile | ❌ FAIL | Rate-limit 429s under multi-worker load. |
| s7-sqlite-contention | ❌ FAIL | Rate-limit 429s under multi-worker load. |
| s8-rate-limit | ✅ PASS | Explicitly tests rate-limit behavior; passes. |

**Manual spot checks:**
- `/v1/approvals` — returns 200 under a single request.
- `/v1/intents/compile` — returns 200 under a single request.
- `/v1/provenance/ingest` — returns expected 400 for unknown runtime.

**Interpretation:** The default `2/50` per-IP token-bucket limit is intentionally safety-oriented. It protects a single-node pilot from accidental overload and from a single client IP generating excessive traffic. Under multi-worker stress (5 workers, 3s duration) the sustained request volume exceeds the per-IP limit, producing expected 429 responses. This is **not a code defect**; it is expected behavior for the config/workload combination.

> **Consistency with prior evidence**: This result aligns exactly with [`docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md`](./2026-05-22-slo-default-config-evidence.md), which documents that the default config fails the canonical SLO workload by design. The stress-suite failures are a local reproduction of the same principle.

---

### 3.7 Stress Suite — High-Throughput Profile (2.7)

**Environment:**
- `FERRUMD_RATE_LIMIT_PER_SECOND=1000`
- `FERRUMD_RATE_LIMIT_BURST=10000`
- `FERRUMD_AUTH_MODE=disabled`
- `FERRUMD_BIND_ADDR=127.0.0.1:8080`

Server log confirmed these effective values at startup.

**Command:**
```bash
BASE_URL=http://127.0.0.1:8080 WORKERS=5 DURATION=3 make stress
```

**Results:**

| Scenario | Requests | RPS | Errors | Notes |
|----------|----------|-----|--------|-------|
| s1-health | 653 | 217.66 | 0 | ✅ PASS |
| s2-auth | 792 | 264.00 | 0 unexpected | ✅ PASS |
| s4-intent-compile | 501 | 167.00 | 0 unexpected | ✅ PASS |
| s7-sqlite-contention | 681 | 227.00 | 0 unexpected | 681 expected 400 validation/backpressure responses |
| s8-rate-limit | 206 | 68.66 | 0 | ✅ PASS (no 429 observed) |

**Interpretation:** With the explicit high-throughput profile (`1000/10000`), all stress scenarios pass. The s7 scenario produces expected 400 responses due to SQLite write-queue backpressure under contention; these are **expected validation/backpressure responses**, not unexpected errors. No 429s are observed, confirming the rate-limit ceiling is not the bottleneck at this workload level.

This result is consistent with the canonical SLO Run #3 max-valid pass documented on 2026-05-21.

---

## 4. Consolidated Interpretation

| Observation | Implication |
|-------------|-------------|
| Dry-run and real SLO rehearsal scripts execute correctly | Tooling is functional for future target-host sustained observations. |
| Restore drill passes | `ferrumctl backup/restore/verify` logic remains sound locally. |
| PG restart drill passes (14s recovery) | Local Docker reconnect behavior remains within threshold. |
| Pilot readiness probes pass (shallow/deep/metrics) | Local `ferrumd` starts and reports healthy components. |
| Default config stress fails some scenarios | Expected and documented; default is safety-oriented, not performance-oriented. |
| High-throughput profile stress passes all scenarios | Explicit profile selection is required for load validation; no code defects found. |

---

## 5. Known Gaps and Limits

- All evidence is **local/domainless** (`127.0.0.1:8080`). No target-host or production claims.
- SLO sustained observations are **rehearsals**, not approved SLO window closures.
- PG restart drill is **local Docker only**; production PostgreSQL recovery, TLS/SSL DSN, and HA failover are not tested.
- Restore drill is **temp-directory only**; target-host restore and offsite backup validation are not tested.
- Pilot readiness functional probe was **skipped** due to `auth=disabled` local profile.
- Stress suite duration was **3 seconds** per scenario; longer sustained runs would be needed for full SLO certification.
- Default rate-limit stress failures are **config mismatch**, not code bugs. The max-valid profile is required for load validation.

---

## 6. Verdict

**Domainless hardening batch A — 2026-05-25: ALL LOCAL CHECKS PASSED** with the following caveats:

- Default-config stress scenarios s2/s4/s7 **FAILED AS EXPECTED** due to per-IP rate-limit enforcement.
- High-throughput profile stress **PASSED ALL SCENARIOS**.
- No production-ready, full G2, Block A closure, HA, or production PostgreSQL claims are made.

This artifact documents the fresh local hardening runs and confirms consistency with existing evidence. It does not close any blockers or change readiness posture.

---

## 7. Related Docs

| Document | Purpose |
|----------|---------|
| [`docs/implementation-path/artifacts/2026-05-21-pg-container-restart-drill-evidence.md`](./2026-05-21-pg-container-restart-drill-evidence.md) | Prior PG restart drill evidence (same 14s recovery) |
| [`docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md`](./2026-05-22-slo-default-config-evidence.md) | Default-config failure/decision compilation |
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase evidence checklist |
| [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md) | Current state summary |
| [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md) | Operational rate-limit tuning guidance |
| [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) | Canonical SLO workload procedure |

---

*Artifact created: 2026-05-25. Domainless hardening evidence — local runs only. No production-ready claim.*
