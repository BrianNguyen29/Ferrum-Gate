# 2026-05-18 WAL Crash-Recovery Evidence

> **LOCAL-ONLY / NON-PRODUCTION EVIDENCE — OPERATOR REVIEW REQUIRED**
>
> This artifact documents a local SQLite WAL crash-recovery drill and two small
> script-hygiene fixes applied on 2026-05-18. It does **NOT** constitute
> production-ready evidence, does **NOT** close any G2 gate, and does **NOT**
> replace operator-executed drills on a target deployment host.
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

## Changes Applied

### 1. `scripts/run_wal_crash_recovery_drill.sh` (new)

A local-only, temp-directory-based SQLite WAL crash-recovery drill. It does **not**
require a running `ferrumd` instance.

**What it tests:**
1. Creates a WAL-mode SQLite database and inserts 3 baseline rows.
2. Verifies journal mode is `wal`.
3. Starts a background writer process that continuously inserts rows.
4. After ~1.5 seconds, sends `SIGKILL` to the writer (simulating a crash).
5. Reopens the database and runs `PRAGMA integrity_check`.
6. Verifies baseline rows survive the crash.
7. Verifies committed row count is internally consistent (`MAX(id) >= COUNT(*)`).
8. Runs `PRAGMA wal_checkpoint(TRUNCATE)`.
9. Verifies final integrity and that the WAL file is truncated.

**Command:**
```bash
bash scripts/run_wal_crash_recovery_drill.sh
```

**Output (representative):**
```
[INFO] Creating WAL-mode SQLite DB: /tmp/tmp.XXXX/wal_drill.db
wal
[PASS] Baseline rows inserted (count=3)
[PASS] Journal mode is WAL
[INFO] Starting background writer (1.5s active window)...
[INFO] Writer PID: 12012
[INFO] Sending SIGKILL to writer (simulating crash)...
[INFO] Reopening DB after crash...
[PASS] PRAGMA integrity_check after crash = ok
[PASS] Baseline rows survive crash (count=14 >= 3)
[PASS] Row count internally consistent (MAX(id)=14 >= COUNT=14)
[PASS] All baseline values (1001,1002,1003) present after crash
[INFO] Running PRAGMA wal_checkpoint(TRUNCATE)...
[PASS] Checkpoint executed (result: 1|0|0)
[PASS] Final integrity_check after checkpoint = ok
[PASS] Final row count >= baseline (count=14)
[PASS] WAL file truncated after checkpoint (size=0b)
========================================
WAL CRASH-RECOVERY DRILL SUMMARY
========================================
Passed: 10
Failed: 0
WAL CRASH-RECOVERY DRILL: ALL CHECKS PASSED
```

**Status:** **PASS**

---

### 2. `scripts/run_mcp_lifecycle_smoke.sh` — `--help` / `-h` handler

Added early `--help` / `-h` argument handling that prints usage and exits `0`
without building or running any smoke tests.

**Commands:**
```bash
bash scripts/run_mcp_lifecycle_smoke.sh --help
bash scripts/run_mcp_lifecycle_smoke.sh -h
```

**Status:** Both exit `0` and do not run smoke. **PASS**

---

### 3. `scripts/check_pilot_readiness.py` — Bearer token redaction

Added `_redact_cmd()` helper that strips sensitive tokens before printing commands:
- Redacts the value immediately following `--bearer-token`.
- Redacts the token in `Authorization: Bearer <token>` headers.

**Verification:**
```python
# _redact_cmd(['ferrumctl', '--bearer-token', 'secret123', ...])
# -> ['ferrumctl', '--bearer-token', '<REDACTED>', ...]

# _redact_cmd(['curl', '-H', 'Authorization: Bearer supersecret', ...])
# -> ['curl', '-H', 'Authorization: Bearer <REDACTED>', ...]
```

**Status:** Verified via ad-hoc import during the validation session; `python3 -m py_compile` passes. **PASS**

---

## WAL Limitations and Non-Claims

1. **Single-process SQLite WAL only**: This drill exercises SQLite's native WAL
   replay behavior after a single writer process is killed. It does **not** test
   multi-reader/writer concurrency, multi-node replication, or PostgreSQL WAL.
2. **No live ferrumd**: The drill uses standalone `sqlite3` CLI. It does not
   validate the FerrumGate store layer's WAL configuration (`synchronous=NORMAL`,
   `wal_autocheckpoint=1000`, `busy_timeout=5000ms`) under application load.
3. **Deterministic but synthetic**: The crash is a `SIGKILL` to a tight-loop
   writer. Real-world crashes may involve power loss, disk failures, or kernel
   panics, which are not simulated here.
4. **No production-ready claim**: FerrumGate v1 remains RC-ready/conditional.
5. **No G2 closure**: This is a local repo-side drill. Target-host execution
   remains operator-owned.
6. **No Block A closure**: Block A (real owned domain / DNS) remains
   WAIVED/CONDITIONAL.

---

## Evidence Files

- This artifact: `docs/implementation-path/artifacts/2026-05-18-wal-crash-recovery-evidence.md`
- WAL drill script: `scripts/run_wal_crash_recovery_drill.sh`
- MCP smoke script: `scripts/run_mcp_lifecycle_smoke.sh`
- Pilot readiness script: `scripts/check_pilot_readiness.py`

---

## Signoff

| Role | Status |
|------|--------|
| Automated local drill runner | Completed 2026-05-18 |
| Operator review | **REQUIRED** |
| Target-host execution | **PENDING** (operator-owned) |

---

*Generated: 2026-05-18T00:00 UTC*  
*Label: local/test-drill — not a production evidence packet*
