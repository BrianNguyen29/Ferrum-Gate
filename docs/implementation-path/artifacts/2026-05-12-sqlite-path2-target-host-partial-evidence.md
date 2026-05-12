# 2026-05-12 — SQLite Path 2 Target-Host Partial Evidence (Post-Firewall-Unblock)

> **Status**: PARTIAL EVIDENCE — operator-owned. No production-ready claim.  
> **Purpose**: Record target-host execution evidence gathered after the SSH firewall was unblocked on 2026-05-12. Distinguishes technical evidence from operator signoff.  
> **Scope**: Single-node SQLite pilot target host only. No PostgreSQL/multi-node/HA.  
> **Constraint**: This artifact does NOT close any blockers that require operator signoff or full phase execution. B1 target D1–D6 remains not executed. G3.6 full acceptance remains not achieved. Do not mark checklist boxes complete based on this evidence. No secret values are recorded.

---

## 1. Context

This artifact documents the evidence gathered after the operator authorized a temporary GCP firewall update to allow the runner IP, enabling direct SSH to the SQLite Path 2 target host. The prior blocked attempt is recorded in [`artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md`](./2026-05-12-sqlite-path2-target-host-blocked-attempt.md). After evidence collection, the SSH firewall source range was restored to its original narrow allowlist.

Pre-execution commit: `f72f0fb docs: record PostgreSQL D1 rehearsal evidence`.

---

## 2. Prior Verified Commit

| Commit | Message | Status |
|---|---|---|
| `f72f0fb` | `docs: record PostgreSQL D1 rehearsal evidence` | **Pushed** |

---

## 3. Firewall Update

| Attribute | Before | During Evidence Collection | Restored After Evidence |
|---|---|---|---|
| Firewall rule name | `ferrumgate-nonprod-fw-ssh` | `ferrumgate-nonprod-fw-ssh` | `ferrumgate-nonprod-fw-ssh` |
| Source ranges | `118.69.4.63/32` | `118.69.4.63/32,118.68.117.136/32` | `118.69.4.63/32` |
| Protocol/Port | tcp:22 | tcp:22 | tcp:22 |
| Action | Allow | Allow | Allow |

**Runner IP added**: `118.68.117.136/32`

This temporary change unblocked direct SSH from the runner environment to the target VM. The runner IP was removed after evidence collection to restore least-privilege posture.

---

## 4. Direct SSH After Firewall Update

| Check | Result |
|---|---|
| Direct SSH to target VM | `SSH_OK` |

SSH connectivity confirmed after the firewall source range was expanded.

---

## 5. VM-Local Token Presence Check

| Check | Result | Note |
|---|---|---|
| VM-local bearer token presence | `TOKEN_PRESENT` | Token value was verified as present on the target host; value is **not printed** in this artifact. |

The presence of a valid bearer token on the target host enabled authenticated API probing.

---

## 6. Phase 3E Evidence Script Results

An evidence verification script was executed on the target host. All automated checks **PASSED**.

| # | Check | Expected | Result |
|---|---|---|---|
| E1 | VM status | RUNNING | **PASSED** |
| E2 | HTTPS `/v1/healthz` | HTTP 200 | **PASSED** |
| E3 | HTTPS `/v1/readyz` | HTTP 200 | **PASSED** |
| E4 | HTTPS `/v1/readyz/deep` | HTTP 200 | **PASSED** |
| E5 | HTTPS `/v1/metrics` | HTTP 200 | **PASSED** |
| E6 | `store_health_up` | 1 | **PASSED** |
| E7 | `write_queue_depth` | 0 | **PASSED** |
| E8 | Approvals without token | HTTP 401 | **PASSED** |
| E9 | Approvals with VM-local token | HTTP 200 | **PASSED** |
| E10 | `caddy.service` status | active | **PASSED** |
| E11 | `ferrumgate.service` status | active | **PASSED** |
| E12 | `ferrumgate-backup.timer` status | enabled | **PASSED** |
| E13 | Latest backup file present | `ferrumgate_20260508_154446.db` | **PASSED** |
| E14 | Backup timer next run visible | visible in `systemctl list-timers` | **PASSED** |

**Overall result**: PASSED all evidence checks.

> **Note**: These checks verify service health, auth enforcement, backup presence, and timer status. They do **not** constitute B1 D1–D6 drill execution, B2 full restore-to-production drill, or B8 full G3.6 phase sequence.

---

## 7. Safe SQLite Temp Restore / Integrity Drill

A safe restore drill was performed using a **temporary copy** (not overwriting the production database).

### 7.1 First Attempt (Without Sudo)

| Attempt | Result | Note |
|---|---|---|
| First attempt | Permission denied | Expected; backup directory requires elevated privileges |

### 7.2 Second Attempt (Sudo Temp-Copy Drill)

| Check | Result |
|---|---|
| Backup present | `BACKUP_PRESENT=yes` |
| `PRAGMA integrity_check` | `INTEGRITY=ok` |
| Table count | `TABLE_COUNT=0` |
| Size (bytes) | `SIZE_BYTES=4239360` |
| Temp cleaned | `TEMP_CLEANED=yes` |

### 7.3 Caveat — Table Count = 0 (RESOLVED in extended evidence)

`TABLE_COUNT=0` was observed on the restored backup. The same `table_count=0` was also observed on the **configured current database** on the target host.

> **Resolution**: The `table_count=0` was caused by the counting query operating against the raw DSN string (including query parameters) rather than the resolved file path. Extended evidence in [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](./2026-05-12-g36-full-duration-compile-only-evidence.md) §3 confirms the actual database has `sqlite_master` count=55 (14 tables, 41 indexes) and `PRAGMA integrity_check: ok`. The latest backup matches these counts. **This caveat is resolved.**
>
> **Remaining gap**: A full **restore-to-production** drill (stop ferrumd, overwrite live DB, restart, verify `readyz/deep`) remains **not executed**.

---

## 8. Sanitized Service Config Review

| Attribute | Value | Note |
|---|---|---|
| `ferrumgate.service` | active | — |
| `caddy` | active | — |
| `FERRUMD_STORE_DSN` | **redacted** | Value verified present; not printed |
| `FERRUMD_BEARER_TOKEN` | **redacted** | Value verified present; not printed |
| `auth_mode` | bearer | — |

---

## 9. Authenticated Bounded G3.6 Compile-Only Probe

A bounded compile-only workload was executed to validate authenticated request handling under load. This is **not** the full G3.6 acceptance workload.

| Parameter | Value |
|---|---|
| Baseline | 10 s idle |
| Target duration | 180 s |
| Target rate | 1 req/s |
| Cooldown | 10 s |
| Workload type | Compile-only (intent compile) |
| Adapter mix | None — compile-only, no FS/Git/HTTP/SQLite/Maildraft adapter exercise |

### 9.1 Results

| Metric | Value |
|---|---|
| Total requests | 173 |
| HTTP 200 | 133 |
| HTTP 429 (rate limited) | 40 |
| p50 latency | ~205.12 ms |

### 9.2 Interpretation

- The service accepted the majority of authenticated compile requests.
- HTTP 429 responses indicate rate-limiting/backpressure is active under this load profile.
- **This is a compile-only probe**: no adapter execution paths (FS, Git, HTTP, SQLite, Maildraft) were exercised.
- **Not full G3.6 acceptance**: the full phase sequence (baseline → low → target → spike → cooldown) was not executed; no adapter mix; bounded duration only.

---

## 10. Post-Workload Metrics Summary

Metrics collected after the bounded compile-only probe:

| Metric | Value | Interpretation |
|---|---|---|
| `ferrumgate_store_health_up` | 1 | Store healthy post-workload |
| `ferrumgate_write_queue_depth` | 0 | No write backlog post-workload |
| `governance_errors_total{route="/v1/intents/compile"}` | 0 | No governance errors on compile route |
| `governance_success_total{route="/v1/intents/compile"}` | 1938 | Successful compiles accumulated (includes pre-existing count) |

---

## 11. Blocker Status Summary

### 11.1 Doc 115 — SQLite Path 2 Target-Host Checklist

| Blocker | Status | Evidence | Caveats |
|---|---|---|---|
| B1 — Target-host D1–D6 evidence | **NOT EXECUTED** | — | Drills not run. Remains operator-owned. |
| B2 — SQLite restore drill | **PARTIAL EVIDENCE** | Safe temp-copy drill passed (`integrity_check: ok`); see §7 | `table_count=0` caveat **resolved** as query/DSN parsing issue; actual DB has 14 tables, 41 indexes. Full restore-to-production still **not executed**. |
| B3 — Backup automation | **PARTIAL EVIDENCE** | Backup timer enabled; latest backup file present (`ferrumgate_20260508_154446.db`); see §6 | Retention pruning and full `ferrumctl backup verify` not yet demonstrated. |
| B4 — TLS/reverse proxy configuration | **PARTIAL EVIDENCE** | HTTPS probes pass; `caddy.service` active; see §6, §8 | Operator has not independently verified cert paths or config adaptation. |
| B5 — Bearer token generation | **PARTIAL EVIDENCE** | Token present on host (`TOKEN_PRESENT`); `auth_mode=bearer`; see §5, §8 | Token generation command (`openssl rand -hex 32`) not independently witnessed. |
| B8 — G3.6 real workload / post-deploy monitoring | **PARTIAL EVIDENCE** | Authenticated compile-only probe executed; 133×200, 40×429; see §9. Full-duration compile-only sequence also executed; see [`artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md`](./2026-05-12-g36-full-duration-compile-only-evidence.md) §7 | Full phase sequence executed **compile-only**; no adapter mix; `readyz/deep` degraded under load (3/5 target, 2/5 spike); not full G3.6 acceptance. |

### 11.2 Doc 116 — G3.6 Monitoring Execution Plan

| Item | Status | Evidence | Caveats |
|---|---|---|---|
| Full G3.6 acceptance (A1–A6) | **NOT ACHIEVED** | — | Requires full phase sequence, adapter mix, ≥1h sustained write rate, and operator signoff. |
| B8-1 Load generator availability | N/A | Manual probe used | Engineering load generator script (`scripts/run_real_workload_generator.py`) not yet delivered. |
| B8-2 Phase sequence execution | **NOT EXECUTED** | Only bounded compile-only probe run; see §9 | Baseline → low → target → spike → cooldown not performed. |
| B8-3 Metrics snapshots at each phase | **NOT EXECUTED** | Single post-workload snapshot; see §10 | Phase-by-phase snapshots not collected. |
| B8-4 Sustained write rate / queue depth / `readyz/deep` | **PARTIAL** | `write_queue_depth=0`, `store_health_up=1`; see §6, §10 | Not measured across all phases under adapter-mixed load. |
| B8-5 Update G3.6 evidence packet | **NOT DONE** | — | `106-g3-6-pilot-metrics-evidence-packet.md` not yet refreshed with real workload data. |
| B8-6 Operator re-signs G3.6 full | **NOT DONE** | — | Operator signoff still pending. |

---

## 12. Explicit Non-Claims

- **No production-ready claim**: This artifact does not make FerrumGate production-ready.
- **No B1 closure**: B1 target-host D1–D6 evidence remains **not executed**.
- **No G3.6 full acceptance**: G3.6 remains **conditionally accepted only** (compile-only/light workload basis from 2026-05-11). This bounded probe does **not** upgrade G3.6 to full acceptance.
- **No PostgreSQL production deployment**: PostgreSQL/multi-node/HA remains out of scope.
- **No HA/multi-node claim**: Single-node SQLite only.
- **No secret recording**: No bearer token value, password, DSN detail, or private key path is recorded in this artifact.
- **No fabricated evidence**: All observations are from real commands and probes executed on the target host on 2026-05-12.
- **No operator signoff**: This is technical evidence only. Final acceptance requires operator review and signature.

---

## 13. Cross-References

| Artifact | Links To | Purpose |
|---|---|---|
| This artifact | `artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md` | Prior blocked attempt |
| This artifact | `115-sqlite-path2-target-host-checklist.md` | Blocker definitions B1–B5, B8 |
| This artifact | `116-g36-monitoring-execution-plan.md` | G3.6 execution plan and acceptance criteria |
| This artifact | `112-post-p5c-completion-execution-plan.md` | Track 4 and Phase 3–5 context |
| This artifact | `66-path-2-operator-handoff.md` §B.0 | Consolidated operator blockers B1–B8 |
| This artifact | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 conditional acceptance baseline |
| This artifact | `artifacts/2026-05-12-g36-full-duration-compile-only-evidence.md` | Extended evidence: schema review, B1 limitation, full-duration compile-only G3.6 sequence |
| This artifact | `58-workload-compensation-drill-evidence-template.md` | D1–D6 drill evidence template (not yet filled) |

---

## 14. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial partial evidence artifact — post-firewall-unblock SSH and authenticated probe | Engineering |
| 2026-05-12 | Recorded SSH firewall restoration to original source range after evidence collection | Engineering |
| 2026-05-12 | Cross-referenced extended evidence artifact; B2 `table_count=0` caveat resolved as DSN-query parsing issue; B8 updated to reference full-duration compile-only sequence | Engineering |

---

*Artifact created: 2026-05-12. SQLite Path 2 Target-Host Partial Evidence — technical evidence only. No blocker closed. No production-ready claim. Operator-owned signoff still required.*
