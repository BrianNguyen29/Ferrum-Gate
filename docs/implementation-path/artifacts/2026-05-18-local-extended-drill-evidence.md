# 2026-05-18 Local Extended Drill Evidence

> **LOCAL-ONLY / NON-PRODUCTION EVIDENCE — OPERATOR REVIEW REQUIRED**
>
> This artifact documents local extended operational drills run on 2026-05-18 in the
> development environment. It does **NOT** constitute production-ready evidence,
> does **NOT** close any G2 gate, and does **NOT** replace operator-executed drills
> on a target deployment host.
>
> Block A remains **WAIVED/CONDITIONAL** — no real owned domain or DNS is available.
> No production-ready claim is made. FerrumGate v1 remains RC-ready/conditional.

---

## Run Context

| Field | Value |
|-------|-------|
| Date | 2026-05-18 |
| Environment | Local development workspace (`/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify`) |
| Host | Linux workstation (single-node, no target VM) |
| Scope | Local-only; no SSH/GCP/network target ops; no live DuckDNS; no secrets |

---

## Drill Inventory and Status

| Drill | Command | Status | Notes |
|-------|---------|--------|-------|
| **G2.1-local** SQLite backup/restore/verify | `bash scripts/run_local_restore_drill.sh` | **PASS** | ferrumctl backup create, verify, restore all passed; data matched with sqlite3 |
| **B3** Retention pruning | `bash scripts/test_retention_pruning_locally.sh --retention-days 7` | **PASS** | Old matching backup pruned; non-matching preserved; new backup kept |
| **D1–D6** Adapter compensation (cargo-test) | `python3 scripts/run_d1_d6_drills.py` | **PASS** | 61 tests total across 6 adapters (D1=11, D2=9, D3=1, D4=22, D5=11, D6=7) |
| **API lifecycle plan** | `python3 scripts/run_d1_d6_drills.py --api-drills --plan --server-url http://127.0.0.1:8080` | **PASS (plan)** | 6 drill plans generated (9-step lifecycle each); zero live requests sent |
| **G3.6 workload plan** | `python3 scripts/run_real_workload_generator.py --plan --server-url http://127.0.0.1:8080` | **PASS (plan)** | 5-phase plan generated; 3,360 estimated requests; zero live traffic |
| **Pre-target gate** | `bash scripts/run_pre_target_gate.sh` | **PASS** | fmt, compile, ferrumctl smoke, config validation, restore drill, skeleton generator, docs present, bearer-auth smoke all passed |
| **Supplemental WAL sanity** | Custom temp script under `/tmp/opencode` | **PASS** | WAL mode → write → checkpoint(TRUNCATE) → integrity_check = ok; labeled as supplemental local sanity only |

---

## Detailed Outputs

### G2.1-local — Backup/Restore/Verify

```
[INFO] Store DB created: .../store/ferrumgate.db (8192 bytes)
[PASS] Source store integrity check passed
[INFO] Backup created: .../backups/ferrumgate.db_1779068933.db (8192 bytes)
[PASS] Backup integrity check passed
[INFO] Restoring backup to .../restore/ferrumgate_restored.db...
Database restored successfully: .../restore/ferrumgate_restored.db
[PASS] Restored database integrity check passed
[PASS] Data match: original and restored databases are identical
=== LOCAL RESTORE DRILL COMPLETE ===
```

**Stop condition**: Script exits on first failure (`set -euo pipefail`).
**Non-claim**: This is a local temp-environment drill. G2.1 target-host execution remains pending operator action.

---

### B3 — Retention Pruning

```
[INFO] Seeded old matching backup:   .../backups/ferrumgate_test.db_1776476934.db
[INFO] Seeded old nonmatching backup: .../backups/other_db_1776476934.db
[INFO] Running: ferrumctl backup create ... --retention-days 7
Backup created: .../backups/ferrumgate_test.db_1779068935.db (8192 bytes)
Pruned old backup: .../backups/ferrumgate_test.db_1776476934.db
[PASS] Old matching backup was pruned
[PASS] Nonmatching old backup was preserved
[PASS] New backup was kept
B3: ALL CHECKS PASSED
```

**Stop condition**: Assertion failures increment `FAILED` counter; script exits non-zero if `FAILED > 0`.
**Non-claim**: B3 is closed via delegated authority on 2026-05-15 (run id `20260515T1606Z-b3-retention`). This local rerun is confirmatory only.

---

### D1–D6 — Adapter Compensation Tests

| Adapter | Tests | Result |
|---------|-------|--------|
| D1 (fs) | 11 | PASS |
| D2 (git) | 9 | PASS |
| D3 (git remote fail-closed) | 1 | PASS |
| D4 (http) | 22 | PASS |
| D5 (sqlite) | 11 | PASS |
| D6 (maildraft) | 7 | PASS |
| **Total** | **61** | **PASS** |

**Stop condition**: Each cargo test invocation returns non-zero on failure; script counts failures and reports summary.
**Non-claim**: These are unit/integration tests in a local cargo environment. Target-host D1–D6 drills remain operator-owned.

---

### API Lifecycle Plan (No Live Traffic)

Command used:
```bash
python3 scripts/run_d1_d6_drills.py \
  --api-drills --plan --server-url http://127.0.0.1:8080
```

- Generated 6 drill plans covering the full 9-step lifecycle:
  1. Compile Intent
  2. Evaluate ActionProposal
  3. Mint Capability
  4. Authorize Execution
  5. Prepare Execution
  6. Execute Adapter Operation
  7. Compensate Execution
  8. Verify Execution Outcome (noted as skipped for compensation drills)
  9. Capture Evidence
- Mode was explicitly `PLAN`; no HTTP requests were sent.
- Output saved to `/tmp/ferrum-drill-evidence-api-plan/api_drill_commands.md`.

**Non-claim**: Plan mode produces curl-ready templates. Live API execution requires a running ferrumd server and bearer token; no live execution was attempted.

---

### G3.6 Workload Plan (No Live Traffic)

Command used:
```bash
python3 scripts/run_real_workload_generator.py \
  --plan --server-url http://127.0.0.1:8080
```

- Generated 5-phase workload profile:

| Phase | Duration (s) | Rate (rps) | Est. Requests |
|-------|-------------:|-----------:|--------------:|
| baseline | 600 | 0.0 | 0 |
| low | 600 | 0.1 | 60 |
| target | 1,800 | 1.0 | 1,800 |
| spike | 300 | 5.0 | 1,500 |
| cooldown | 600 | 0.0 | 0 |
| **Total** | **3,900** | — | **3,360** |

- Adapter mix: fs 20%, git 20%, http 20%, sqlite 20%, maildraft 20%.
- Zero live requests sent.

**Non-claim**: This is a planning artifact. G3.6 live workload execution on target host remains operator-owned.

---

### Pre-Target Gate

All checks passed:
- Cargo format check
- Cargo workspace compile check
- ferrumctl smoke
- Config examples validation (no real secrets, correct nginx forwarding, no hardcoded tokens)
- Local restore drill (re-run within gate)
- Evidence skeleton generator
- Required Path 2 docs present
- Required config examples present
- Local bearer-auth smoke (7/7 passed: public endpoints 200, protected endpoint 401/401/200)

**Non-claim**: Pre-target gate validates repo-side readiness only. It does not validate target-host deployment or close G2.

---

### Supplemental WAL Sanity Check

There is **no standalone WAL crash-recovery script** in the repository. A bounded supplemental sanity check was run manually under `/tmp/opencode`:

```bash
sqlite3 "$DB" "PRAGMA journal_mode=WAL; CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t VALUES (1),(2),(3);"
sqlite3 "$DB" "PRAGMA wal_checkpoint(TRUNCATE);"
sqlite3 "$DB" "PRAGMA integrity_check;"
```

Result: `ok`; data intact (`1, 2, 3`).

**Gap note**: This is supplemental local sanity only, not a structured crash-recovery drill. No production claim is attached. A full WAL crash-recovery test (e.g., SIGKILL mid-transaction, journal replay) is not covered here and remains a future operator-owned drill if desired.

---

## Non-Claims and Boundaries

1. **No production-ready claim**: FerrumGate v1 remains RC-ready/conditional.
2. **No G2 closure**: G2.1–G2.8 are signed for conditional single-node SQLite pilot scope only. Target-host drills are still required.
3. **No Block A closure**: Block A (real owned domain / DNS) remains WAIVED/CONDITIONAL. DuckDNS is accepted for pilot only; full closure requires operator action.
4. **No live target operations**: No SSH, GCP, network target, or real DNS requests were made.
5. **No secrets introduced**: All tokens used were temporary auto-generated values in temp directories. No real API keys, passwords, or bearer tokens are present in this artifact.
6. **No PostgreSQL production claim**: All drills used on-disk SQLite. PostgreSQL is recommended for sustained high write throughput but was not exercised here.

---

## Evidence Files

- This artifact: `docs/implementation-path/artifacts/2026-05-18-local-extended-drill-evidence.md`
- D1–D6 cargo test evidence: `/tmp/ferrum-drill-evidence-d1d6/drill_summary.md`
- API drill plan: `/tmp/ferrum-drill-evidence-api-plan/api_drill_commands.md`
- G3.6 workload plan: `/tmp/ferrum-g36-workload-plan/workload_plan.md`

---

## Signoff

| Role | Status |
|------|--------|
| Automated local drill runner | Completed 2026-05-18 |
| Operator review | **REQUIRED** |
| Target-host execution | **PENDING** (operator-owned) |

---

*Generated: 2026-05-18T01:51 UTC*
*Label: local/test-drill — not a production evidence packet*
