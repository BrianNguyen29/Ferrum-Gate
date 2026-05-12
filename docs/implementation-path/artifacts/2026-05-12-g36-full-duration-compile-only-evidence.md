# 2026-05-12 — G3.6 Full-Duration Compile-Only Evidence + Schema/B1 Update

> **Status**: EXTENDED PARTIAL EVIDENCE — operator-owned. No production-ready claim.  
> **Purpose**: Record the full-duration compile-only G3.6 phase sequence, target-host schema review (resolving the earlier `table_count=0` caveat), B1 D1–D6 limitation, and firewall restoration. Distinguishes technical evidence from operator signoff.  
> **Scope**: Single-node SQLite pilot target host only. No PostgreSQL/multi-node/HA.  
> **Constraint**: This artifact does NOT close B1, does NOT achieve full G3.6 acceptance, and does NOT authorize production deployment. No secret values are recorded.

---

## 1. Context

This artifact documents evidence gathered during a second SSH session to the SQLite Path 2 target host on 2026-05-12. It resolves the `table_count=0` caveat from the earlier partial evidence, records the B1 limitation, and captures a full-duration compile-only G3.6 phase sequence.

Prior artifacts:
- [`artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md`](./2026-05-12-sqlite-path2-target-host-blocked-attempt.md) — initial blocked attempt
- [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./2026-05-12-sqlite-path2-target-host-partial-evidence.md) — first partial evidence (bounded probe, temp restore drill)

---

## 2. SSH Firewall Restoration

The firewall was temporarily opened for evidence collection and then restored to its original narrow allowlist.

| Attribute | During Evidence Collection | Restored After Evidence |
|---|---|---|
| Firewall rule name | `ferrumgate-nonprod-fw-ssh` | `ferrumgate-nonprod-fw-ssh` |
| Source ranges | `118.69.4.63/32,118.68.117.136/32` | `118.69.4.63/32` |
| Protocol/Port | tcp:22 | tcp:22 |
| Action | Allow | Allow |

**Final source range**: `118.69.4.63/32`

> **Security note**: Runner IP `118.68.117.136/32` was removed after evidence collection to restore least-privilege posture.

---

## 3. Target Database Schema Review

### 3.1 DSN Parsing

| Check | Result |
|---|---|
| `ENV_DSN_PRESENT` | `yes` |
| Raw DSN observation | Contains query parameters |
| Actual DB file path | Query stripped; path points to configured database file |

The raw `FERRUMD_STORE_DSN` includes query-string parameters. When the query is stripped, the resulting file path is the actual on-disk SQLite database.

### 3.2 Database Integrity and Schema Counts

| Metric | Value |
|---|---|
| `DB_SIZE_BYTES` | 4,444,160 |
| `PRAGMA integrity_check` | `ok` |
| `sqlite_master` count | 55 |
| `type='index'` | 41 |
| `type='table'` | 14 |

Representative `sqlite_master` entries include tables such as `approvals`, `capabilities`, `executions`, and their associated indexes.

### 3.3 Caveat Resolution

The earlier `TABLE_COUNT=0` observation (in [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./2026-05-12-sqlite-path2-target-host-partial-evidence.md) §7) was caused by the counting query operating against the raw DSN string (including query parameters) rather than the resolved file path. **This caveat is now resolved.** The configured database has 14 tables, 41 indexes, and passes `PRAGMA integrity_check`.

---

## 4. Latest Backup Schema Review

| Metric | Value |
|---|---|
| `BACKUP_SIZE_BYTES` | 4,444,160 |
| `PRAGMA integrity_check` | `ok` |
| `sqlite_master` count | 55 |
| `type='index'` | 41 |
| `type='table'` | 14 |

The latest backup (`ferrumgate_20260508_154446.db`) matches the current database in schema counts and integrity status. Representative names are consistent with the live database.

> **B2 implication**: The safe temp-copy restore drill integrity and schema counts are now confirmed. A full **restore-to-production** (stop ferrumd, overwrite live DB, restart, verify `readyz/deep`) remains **not executed**.

---

## 5. B1 D1–D6 Target-Host Limitation

### 5.1 Current State

| Check | Result |
|---|---|
| Local adapter runner available | Yes (local development environment) |
| Target-host end-to-end adapter drill runner | **Not available** |
| Valid API payload for FS/Git/HTTP/SQLite/Maildraft adapter execution on target host | **Not currently available** |

### 5.2 Limitation Statement

A local adapter runner exists for development and local testing, but **there is no valid end-to-end adapter drill runner or API payload currently available for target-host B1 closure**. The local runner exercises adapter logic locally; it does not constitute target-host B1 D1–D6 evidence because:

- It does not run against the deployed `ferrumd` on the target host
- It does not validate the full request path through Caddy → `ferrumd` → store → adapter → side effect
- It does not verify target-host rollback, provenance emission, or fail-closed behavior under real network and disk conditions

> **B1 remains open**. Target-host D1–D6 execution requires either:
> 1. Engineering delivery of a target-host-capable adapter drill script/API payload, or
> 2. Manual operator execution of each adapter drill via `ferrumctl` or direct API calls with verified rollback paths.

---

## 6. G3.6 Full-Duration Compile-Only Phase Sequence

A full-duration compile-only phase sequence was executed on the target host to collect extended metrics. This is **not** the full G3.6 acceptance workload because:

- The workload is **compile-only**; no adapter mix (FS, Git, HTTP, SQLite, Maildraft) was exercised
- `readyz/deep` success rate degraded during target and spike phases (3/5 and 2/5 respectively)
- HTTP 429 responses were prevalent under load, indicating rate-limiting/backpressure

| Parameter | Value |
|---|---|
| Total duration | ~4,028.43 s (~67 min) |
| Total requests | 3,065 |
| Overall p50 latency | ~203.2 ms |
| Workload type | Compile-only (intent compile) |
| Adapter mix | None |

---

## 7. Phase Results

### 7.1 Baseline (Idle) — 600 s

| Metric | Value |
|---|---|
| Load | 0 req/s |
| Duration | 600 s |
| `readyz/deep` success | 5/5 |
| `store_health_up` | 1 |
| `write_queue_depth` | 0 |
| Compile success total | 1,938 (unchanged) |
| Governance errors | 0 |

### 7.2 Low Load — 0.1 req/s, 600 s

| Metric | Value |
|---|---|
| HTTP 200 | 57 |
| `readyz/deep` success | 5/5 |
| Compile success total | 1,995 |
| p50 latency | ~220.94 ms |
| Governance errors | 0 |

### 7.3 Target Load — 1 req/s, 1,800 s

| Metric | Value |
|---|---|
| HTTP 200 | 880 |
| HTTP 429 | 819 |
| `readyz/deep` success | 3/5 |
| Compile success total | 2,875 |
| p50 latency | ~206.53 ms |
| Governance errors | 0 |

> **Note**: `readyz/deep` dropped to 3/5 during this phase, indicating readiness degradation under sustained load.

### 7.4 Spike Load — 5 req/s, 300 s

| Metric | Value |
|---|---|
| HTTP 200 | 141 |
| HTTP 429 | 1,168 |
| `readyz/deep` success | 2/5 |
| Compile success total | 3,016 |
| p50 latency | ~199.37 ms |
| Governance errors | 0 |

> **Note**: The majority of requests received HTTP 429. `readyz/deep` dropped to 2/5.

### 7.5 Cooldown (Idle) — 600 s

| Metric | Value |
|---|---|
| Load | 0 req/s |
| Duration | 600 s |
| `readyz/deep` success | 5/5 |
| `store_health_up` | 1 |
| `write_queue_depth` | 0 |
| Compile success total | 3,016 (unchanged) |
| Governance errors | 0 |

### 7.6 Summary Table

| Phase | Duration | Rate | 200s | 429s | Ready | p50 (ms) | Success Total |
|---|---|---|---|---|---|---|---|
| Baseline | 600 s | 0 rps | — | — | 5/5 | — | 1,938 |
| Low | 600 s | 0.1 rps | 57 | — | 5/5 | ~220.94 | 1,995 |
| Target | 1,800 s | 1 rps | 880 | 819 | 3/5 | ~206.53 | 2,875 |
| Spike | 300 s | 5 rps | 141 | 1,168 | 2/5 | ~199.37 | 3,016 |
| Cooldown | 600 s | 0 rps | — | — | 5/5 | — | 3,016 |
| **Total** | **~4,028 s** | — | **1,078** | **1,987** | — | **~203.2** | — |

---

## 8. Post-Sequence Metrics

| Metric | Value | Interpretation |
|---|---|---|
| `ferrumgate_store_health_up` | 1 | Store healthy after full sequence |
| `ferrumgate_write_queue_depth` | 0 | No backlog after cooldown |
| `governance_errors_total{route="/v1/intents/compile"}` | 0 | No governance errors on compile route |
| `governance_success_total{route="/v1/intents/compile"}` | 3,016 | Successful compiles accumulated |

### 8.1 Metrics Scrape Caveats

During the sequence, metrics scrapes at **spike-start** and **cooldown-start** encountered `HTTPError` (likely HTTP 429 or transient unavailability). End-of-phase metrics scrapes succeeded. These gaps are noted; they do not invalidate the end-state metrics but mean intra-phase snapshots are incomplete.

---

## 9. Blocker Status Update

### 9.1 B2 — SQLite Restore Drill

| Aspect | Status | Evidence |
|---|---|---|
| Temp-copy integrity | **RESOLVED** | `integrity_check: ok`, 14 tables, 41 indexes; see §3, §4 |
| Full restore-to-production | **NOT EXECUTED** | Stop ferrumd → overwrite live DB → restart → verify `readyz/deep` not performed |

### 9.2 B1 — D1–D6 Target-Host Evidence

| Aspect | Status | Note |
|---|---|---|
| Target-host execution | **NOT EXECUTED** | No valid end-to-end adapter drill runner/API payload available for target host; see §5 |
| Local adapter runner | Exists | Not sufficient for B1 closure |

### 9.3 B8 / G3.6 — Real Workload / Post-Deploy Monitoring

| Aspect | Status | Evidence |
|---|---|---|
| Full phase sequence (compile-only) | **EXECUTED** | Baseline → low → target → spike → cooldown completed; see §7 |
| Adapter mix | **NOT EXECUTED** | No FS/Git/HTTP/SQLite/Maildraft adapter paths exercised |
| `readyz/deep` ≥ 99% | **NOT MET** | 3/5 at target, 2/5 at spike; degradation under load |
| Sustained write rate ≥1h at target | **PARTIAL** | 1,800 s at 1 rps compile-only; not adapter-exercised |
| G3.6 full acceptance (A1–A6) | **NOT ACHIEVED** | Compile-only, readiness degradation, no adapter mix, no operator signoff |

---

## 10. Explicit Non-Claims

- **No production-ready claim**: This artifact does not make FerrumGate production-ready.
- **No B1 closure**: B1 target-host D1–D6 evidence remains **not executed**.
- **No G3.6 full acceptance**: G3.6 remains **conditionally accepted only**. The full-duration compile-only sequence does **not** satisfy A1–A6 because it lacks adapter mix, exhibits readiness degradation, and has no operator signoff.
- **No PostgreSQL production deployment**: PostgreSQL/multi-node/HA remains out of scope.
- **No HA/multi-node claim**: Single-node SQLite only.
- **No secret recording**: No bearer token value, password, DSN detail, or private key path is recorded in this artifact.
- **No fabricated evidence**: All observations are from real commands and probes executed on the target host on 2026-05-12.
- **No operator signoff**: This is technical evidence only. Final acceptance requires operator review and signature.

---

## 11. Cross-References

| Artifact | Links To | Purpose |
|---|---|---|
| This artifact | `artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md` | Prior partial evidence (bounded probe, temp restore drill) |
| This artifact | `artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md` | Initial blocked attempt |
| This artifact | `115-sqlite-path2-target-host-checklist.md` | Blocker definitions B1–B5, B8 |
| This artifact | `116-g36-monitoring-execution-plan.md` | G3.6 execution plan and acceptance criteria |
| This artifact | `112-post-p5c-completion-execution-plan.md` | Track 4 and Phase 3–5 context |
| This artifact | `66-path-2-operator-handoff.md` §B.0 | Consolidated operator blockers B1–B8 |
| This artifact | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 conditional acceptance baseline |

---

## 12. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial extended evidence artifact — schema review, B1 limitation, full-duration compile-only G3.6 sequence | Engineering |

---

*Artifact created: 2026-05-12. G3.6 Full-Duration Compile-Only Evidence + Schema/B1 Update — technical evidence only. No blocker closed. No production-ready claim. Operator-owned signoff still required.*
