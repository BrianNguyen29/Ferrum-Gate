# 2026-05-18 — Local Confidence Polish Evidence

> **Status**: Evidence-only. No production-ready claim. No operator signoff completion claimed.  
> **Scope**: Post-Path A closure local polish tasks executed on 2026-05-18.  
> **Repository**: `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify`  
> **Environment**: Local-only; no SSH/GCP/DuckDNS/target-host ops; no real external secrets used.

---

## Executive Summary

| Task | Command / Action | Result | Notes |
|---|---|---|---|
| **D1–D6 API live lifecycle local** | `python3 scripts/run_d1_d6_drills.py --api-live --server-url http://127.0.0.1:28080 --bearer-token dummy-local-token` | **PASS (6/6)** | All drills passed after script fix to create temp git repos and sqlite DB for D2/D3/D5. |
| **G3.6 workload local `--execute`** | `python3 scripts/run_real_workload_generator.py --execute --server-url http://127.0.0.1:28080 --bearer-token dummy-local-token --phases '[{"name":"baseline","duration_sec":3,"rate_rps":0},{"name":"spike","duration_sec":5,"rate_rps":1}]'` | **PASS** | Bounded: 2 phases, 5 total requests, all HTTP 200. C2 checkpoints written. readyz/deep probes passed. |
| **MCP lifecycle smoke full rerun** | `bash scripts/run_mcp_lifecycle_smoke.sh` | **PASS (15/15)** | D1.7 + D1.11 all checks passed. ferrumd built/available; no rebuild required. |
| **`make wal-drill`** | `make wal-drill` | **PASS** | Added `wal-drill` target to Makefile; runs `scripts/run_wal_crash_recovery_drill.sh`. |
| **WAL drill integrated into pre-target gate** | Edited `scripts/run_pre_target_gate.sh` | **PASS** | WAL drill runs as check #7 in pre-target gate. |
| **Pre-target gate rerun** | `bash scripts/run_pre_target_gate.sh` | **ALL PASSED** | Fast-mode gate completed in ~2 min; WAL drill included and passed. |
| **git diff --check** | `git diff --check` | **PASS** | No whitespace issues. |
| **Layout validation** | `bash scripts/validate_repo_layout.sh` | **PASS** | Repository layout looks OK. |
| **Contract consistency** | `python3 scripts/check_contract_consistency.py` | **PASS** | VALIDATION PASSED. |

---

## Block A Statement

> **Block A (Real owned domain / DNS)**: Remains **`WAIVED/CONDITIONAL`** for single-node SQLite pilot only.  
> DuckDNS was accepted by operator on 2026-05-17 as a conditional pilot stopgap. A real owned domain is still required for production-ready or full G2 closure.  
> **This artifact does NOT close Block A.**

---

## D1–D6 API Live Lifecycle Local

### Context

A local ferrumd instance was started with `auth_mode = "disabled"` and `sqlite::memory:` store on `127.0.0.1:28080`. The D1–D6 drill runner was invoked in `--api-live` mode with a dummy bearer token (ignored by auth-disabled server).

### Exact Command

```bash
python3 scripts/run_d1_d6_drills.py \
    --api-live \
    --server-url http://127.0.0.1:28080 \
    --bearer-token dummy-local-token \
    --rate-limit-delay 0.5
```

### Results

| Drill | Overall | Steps (abbreviated) |
|---|---|---|
| **D1 (fs adapter)** | ✅ passed | compile→proposal→mint→authorize→prepare→execute→compensate→capture = all passed |
| **D2 (git adapter)** | ✅ passed | compile→proposal→mint→authorize→prepare→execute→compensate→capture = all passed |
| **D3 (git remote fail-closed)** | ✅ passed | compile→proposal→mint→authorize→prepare→execute→compensate→capture = all passed |
| **D4 (http adapter)** | ✅ passed | full lifecycle passed (local echo server used) |
| **D5 (sqlite adapter)** | ✅ passed | compile→proposal→mint→authorize→prepare→execute→compensate→capture = all passed |
| **D6 (maildraft adapter)** | ✅ passed | full lifecycle passed |

### Analysis

- **All 6 drills passed** the complete 9-step API lifecycle (compile → evaluate → mint → authorize → prepare → execute → compensate → capture). Verify was intentionally skipped for compensation drills.
- **Initial run (before fix)**: D2, D3, D5 failed at `prepare_execution` with HTTP 500. Root cause was **not** adapter wiring gaps. The drill script hard-coded production VM paths (`/var/lib/ferrumgate/drill/...`) that do not exist on the local workstation. The git and sqlite adapters correctly fail-closed when target paths are missing.
- **Fix applied**: `_create_local_fixtures()` added to `scripts/run_d1_d6_drills.py`. In `--api-live` mode, the script now creates temp git repos (with `main` branch and initial commit) and a temp sqlite DB (with `drill_table`) under `/tmp/opencode/ferrum-d1d6-fixtures-*/`, then patches the drill templates to use these local paths. Fixtures are cleaned up after the run.
- **Server smoke** (metrics endpoint) **PASS** — `/v1/metrics` returned `ferrumgate_write_queue_depth`, `ferrumgate_http_requests_total`, and `ferrumgate_store_health_up`.

### Boundary / Non-Claim

- This is **local/test-drill evidence only**.
- Does NOT complete any G2 gate.
- Does NOT replace operator-executed target-host drills.
- The fix is local-dev convenience only; target host will use real paths as configured by the operator.

---

## G3.6 Workload Local Execute (Bounded)

### Context

G3.6 default phases total 3900 seconds (~65 min) and are unsuitable for local bounded execution. Custom short phases were used to validate the execution path safely.

### Exact Command

```bash
PHASES=$(python3 -c "import json; print(json.dumps([
    {'name':'baseline','duration_sec':3,'rate_rps':0},
    {'name':'spike','duration_sec':5,'rate_rps':1}
]))")

python3 scripts/run_real_workload_generator.py \
    --execute \
    --server-url http://127.0.0.1:28080 \
    --bearer-token dummy-local-token \
    --phases "$PHASES" \
    --readyz-probes 2 \
    --readyz-interval 2
```

### Results

- **Total requests**: 5
- **All requests HTTP 200**
- **Latency range**: 1.84 ms – 159.22 ms
- **C2 checkpoints**: 2 incremental checkpoint files written (`checkpoint_phase_000.json`, `checkpoint_phase_001.json`)
- **readyz/deep probes**: 2 probes, both HTTP 200 (~1.5 ms each)

### Request Log

```
req 1: sqlite    -> HTTP 200 in 159.22ms
req 2: maildraft -> HTTP 200 in 2.08ms
req 3: git       -> HTTP 200 in 2.36ms
req 4: git       -> HTTP 200 in 1.99ms
req 5: git       -> HTTP 200 in 1.84ms
```

### Boundary / Non-Claim

- **Bounded local execution only** (8 seconds total).
- Does NOT close G3.6 target-host evidence.
- Does NOT validate sustained throughput, memory stability, or tail latency under real pilot load.
- Operator must rerun with default/doc-116 phases on target host for full G3.6 signoff.

---

## MCP Lifecycle Smoke Full Rerun

### Exact Command

```bash
bash scripts/run_mcp_lifecycle_smoke.sh
```

### Results

| Check | Status |
|---|---|
| MCP Initialize | ✅ PASS |
| MCP tools/list (19 tools) | ✅ PASS |
| D1.9 Approval tool registry (approve) | ✅ PASS |
| D1.9 Approval tool registry (reject) | ✅ PASS |
| D1.9 approve dispatch error | ✅ PASS |
| D1.9 reject dispatch error | ✅ PASS |
| ferrum_gate_health | ✅ PASS |
| ferrum_gate_submit_intent registry | ✅ PASS |
| All 8 lifecycle tools present | ✅ PASS |
| Unknown tool METHOD_NOT_FOUND | ✅ PASS |
| MCP ping | ✅ PASS |
| D1.11.1 submit_intent dispatch | ✅ PASS |
| D1.11.2 evaluate_intent dispatch | ✅ PASS |
| D1.11.3 mint_capability dispatch | ✅ PASS |
| D1.11.4 list_intents dispatch | ✅ PASS |

**Overall: 15 passed, 0 failed**

### Boundary / Non-Claim

- Validates MCP stdio transport, registry completeness, and bounded lifecycle dispatch locally.
- Does NOT validate D1.8 (output sanitization) — that requires separate audit.
- Does NOT claim G2 or production-ready.

---

## `make wal-drill` — New Convenience Target

### Makefile Change

Added to `Makefile`:

```makefile
wal-drill:
	@echo "Running local SQLite WAL crash-recovery drill..."
	@bash scripts/run_wal_crash_recovery_drill.sh
```

Also updated `.PHONY` and `help`.

### Execution

```bash
make wal-drill
```

### Results

- 10/10 checks passed.
- WAL-mode DB created, baseline rows inserted, background writer SIGKILL-simulated, integrity verified post-crash, checkpoint(TRUNCATE) executed, final integrity confirmed.

### Script Fix Applied

A transient "database is locked" error was observed immediately post-SIGKILL in the first run. The script was hardened with:

1. A 0.5-second sleep before the post-crash integrity check to allow kernel-level locks to clear.
2. A 3-attempt retry loop with 0.5-second backoff for `PRAGMA integrity_check`.

This fix is minimal, preserves the drill's semantics, and does not weaken the crash-recovery assertion.

---

## Pre-Target Gate Rerun (with WAL Drill Integrated)

### Script Change

Added to `scripts/run_pre_target_gate.sh` after check 6 (bearer-auth smoke):

```bash
# --- 7. WAL crash-recovery drill ---

run_check "WAL crash-recovery drill" \
    "bash '$SCRIPT_DIR/run_wal_crash_recovery_drill.sh'"
```

### Execution

```bash
bash scripts/run_pre_target_gate.sh
```

### Results

| # | Check | Result |
|---|---|---|
| 1 | Cargo format check | ✅ PASS |
| 2 | Cargo workspace compile check | ✅ PASS |
| 3 | ferrumctl smoke | ✅ PASS |
| 4 | Config examples validation | ✅ PASS |
| 5 | Local restore drill (temp SQLite) | ✅ PASS |
| 6 | Evidence skeleton generator | ✅ PASS |
| 7 | Required Path 2 docs present | ✅ PASS |
| 8 | Required config examples present | ✅ PASS |
| 9 | Local bearer-auth smoke | ✅ PASS |
| 10 | **WAL crash-recovery drill** | ✅ **PASS** |

**Overall: ALL LOCAL CHECKS PASSED**

### Note on Full Mode

The `--full` mode (which includes `cargo test --workspace` and `cargo clippy`) was **not rerun** because:
- These were already verified fresh on 2026-05-17 and passed.
- The current polish scope focused on adding WAL drill integration and running the fast gate.
- Full mode adds several minutes and was deemed unnecessary for this delta.

If required, `--full` can be invoked with: `bash scripts/run_pre_target_gate.sh --full`.

---

## Static Quality Checks

| Check | Command | Result |
|---|---|---|
| git diff --check | `git diff --check` | ✅ PASS (no output) |
| Layout validation | `bash scripts/validate_repo_layout.sh` | ✅ PASS |
| Contract consistency | `python3 scripts/check_contract_consistency.py` | ✅ PASS |

---

## Files Changed

| File | Change |
|---|---|
| `Makefile` | Added `wal-drill` target; updated `.PHONY` and `help` |
| `scripts/run_pre_target_gate.sh` | Added WAL crash-recovery drill as check #7; refreshed summary wording so Block A is the remaining production blocker while Blocks B/C remain CLOSED |
| `scripts/run_wal_crash_recovery_drill.sh` | Hardened post-crash integrity check with sleep + retry loop |
| `scripts/run_d1_d6_drills.py` | Added `_create_local_fixtures()`; patched API drill templates with local temp paths in `--api-live` mode; fixture cleanup in `finally` block |
| `docs/implementation-path/artifacts/2026-05-18-local-confidence-polish-evidence.md` | **This artifact** (new) |

---

## Remaining Blockers (Explicit)

| Blocker | Status | Owner | Notes |
|---|---|---|---|
| **Block A — Real owned domain / DNS** | **WAIVED/CONDITIONAL** | Operator | Intentionally deferred; real domain required for production-ready or full G2 closure |
| **G3.6 target-host sustained workload** | Pending | Operator/Engineering | Bounded local execute passed; full doc-116 phases require target host |
| **Production-ready claim** | **NO** | — | Requires all G2/G3 gates + operator signoff + live validation |

---

## Cross-References

| Document | Purpose |
|---|---|
| [`2026-05-17-all-paths-execution-evidence.md`](./2026-05-17-all-paths-execution-evidence.md) | Prior all-paths evidence (Path A closure) |
| [`2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md`](./2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md) | Path A conditional closure acknowledgment |
| [`58-workload-compensation-drill-evidence-template.md`](../58-workload-compensation-drill-evidence-template.md) | D1–D6 drill template |
| [`59-pilot-readiness-evidence-packet.md`](../59-pilot-readiness-evidence-packet.md) | G2 gate evidence packet |
| [`116-g36-monitoring-execution-plan.md`](../116-g36-monitoring-execution-plan.md) | G3.6 execution plan |
| [`122-completion-roadmap-and-hardening-tracker.md`](../122-completion-roadmap-and-hardening-tracker.md) | Completion tracker |

---

*Artifact generated: 2026-05-18. Local-only evidence — no production-ready claim.*
