# 112 — Post-P5c Completion Execution Plan

> **Status**: Planning artifact. No execution claimed. No production-ready claim.  
> **Purpose**: Phased completion execution plan for the four remaining tracks after P5c local smoke evidence (commit `1e7adca`).  
> **Scope**: Documentation and planning only. No live infra changes. No secrets.  
> **Constraint**: `production-ready = NO` throughout. Target-host blockers remain operator-owned. P5c local smoke evidence is local-only and schema-only/0-row.

---

## 1. Context & Baseline

### 1.1 Current State (Post-P5c Local Smoke)

| Item | State |
|---|---|
| Base commit | `1e7adca` (`docs: add P5c local Docker drill evidence`) |
| P5c local smoke | **PASSED** — local Docker PostgreSQL backup/restore mechanics verified |
| Populated local drill | **PASSED** — local Docker PostgreSQL backup/restore with non-zero rows verified; see [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) |
| Path decision | **DONE** — Option A (SQLite Path 2) selected; see [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`) |
| Production-ready claim | **NO** |
| Target-host operator blockers | **PARTIAL EVIDENCE** — SSH unblocked, authenticated probe and safe restore drill executed; B1 D1–D6 not executed; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) and [`artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md`](./artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md) |
| PostgreSQL production deployment | **NO** |
| HA/multi-node | **NO** |

### 1.2 Strategic Verdict

| Phase | Verdict | Owner | Rationale |
|---|---|---|---|
| **Phase 1** | Populated-data local drill **completed** | Engineering | Evidence: [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) |
| **Phase 2** | Operator path decision **completed** — Option A (SQLite) selected | Operator + Engineering | Recorded in [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`); B6/B7 waived |
| **Phase 3–5** | Target-host / operator-executed only — **blocked** on target host access | Operator | Requires real environment; see [`artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md`](./artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md) |

### 1.3 Explicit Non-Claims

- **No production-ready claim**: This plan does not make FerrumGate production-ready.
- **No target-host blocker closure**: All target-host evidence remains operator-owned and is not closed by this document.
- **No PostgreSQL production deployment**: P5c local smoke validates mechanics only; production PostgreSQL deployment remains gated on P5b–P5e and P6.
- **No HA/multi-node**: Single-node scope only.
- **No secret values**: No passwords, tokens, or full DSNs are recorded in this plan.
- **No fabricated evidence**: All evidence items are planned only; execution is separate and tracked per-track.

---

## 2. Phase Overview

| Phase | Name | Tracks Covered | Owner | Executable Now? | Blocker |
|---|---|---|---|---|---|
| **Phase 0** | P5c local smoke baseline | Track 1 baseline | Engineering | ✅ Done | None |
| **Phase 1** | Populated-data local drill | Track 2 | Engineering | ✅ Done | None — see [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) |
| **Phase 2** | Operator path decision gate | Track 1, 3, 4 | Operator + Engineering | ✅ Done — Option A SQLite selected (commit `b320f5c`) | See [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) |
| **Phase 3** | Target-host drill prep & execution | Track 1, 3, 4 | Operator | ☐ Partial evidence | SSH unblocked 2026-05-12; Phase3E evidence script passed; safe restore drill done (`table_count=0` caveat); B1 D1–D6 not executed — see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) |
| **Phase 4** | G3.6 real workload / post-deploy monitoring | Track 3 | Operator + Engineering | ☐ Partial evidence | Authenticated bounded compile-only probe executed (133×200, 40×429); full phase sequence and adapter mix not performed — see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) and [`116-g36-monitoring-execution-plan.md`](./116-g36-monitoring-execution-plan.md) |
| **Phase 5** | Final evidence consolidation & conditional signoff | All tracks | Operator | ☐ No | All prior phases complete |

### 2.1 Dependency Map

```text
Phase 0 (P5c local smoke) ──► Phase 1 (populated local drill)
                                     │
                                     ▼
                         Phase 2 (operator path decision)
                        ┌────────────┴────────────┐
                        ▼                         ▼
              [SQLite path selected]      [PostgreSQL path selected]
                        │                         │
                        ▼                         ▼
              Phase 3a (SQLite target)    Phase 3b (PG target drills)
              Track 4 blockers D1–D6      Track 1 P5c.V1/V2 target
              + restore + backup          + Track 1 target
              + TLS + bearer              + Track 4 deferred to P5b–P5e
                        │
                        └────────────┬────────────┘
                                     ▼
                         Phase 4 (G3.6 real workload)
                                     │
                                     ▼
                         Phase 5 (consolidation & signoff)
```

---

## 3. Track Details

### Track 1 — Target-Host P5c.V1/V2 Drill Planning (Real Operator Blocker Path)

> **Purpose**: Close P5c operator blockers 6–7 from `66-path-2-operator-handoff.md` on a real PostgreSQL target, or obtain explicit N/A waiver if SQLite path is chosen.

#### Track 1 Summary

| Field | Value |
|---|---|
| Blocker IDs | B6 (P5c.V1 backup), B7 (P5c.V2 restore) from `66-path-2-operator-handoff.md` |
| Conditionality | Active only if operator selects PostgreSQL path in Phase 2 |
| Owner | Operator (execution); Engineering (planning / template support) |
| Current gap | No automated target-host PG drill script exists; populated local drill completed |

#### Track 1 — Phase Breakdown

| Sub-phase | Task | Owner | Inputs | Outputs | Status |
|---|---|---|---|---|---|
| T1-P0 | Local smoke baseline (schema-only) | Engineering | `111-p5c-local-docker-drill-plan.md` | `artifacts/2026-05-12-p5c-local-docker-drill-evidence.md` | ✅ Done |
| T1-P1 | Populated-data local drill | Engineering | Populated SQLite fixture or seeded local PG | Local evidence with non-zero row counts | ✅ Done — [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) |
| T1-P2 | Operator selects PG or SQLite path | Operator + Engineering | Path 2 pilot outcome, workload model | Signed path decision in [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`) | ✅ Done — Option A SQLite selected |
| T1-P3 | If PG: Target-host PG drill plan adaptation | Engineering | `109-p5c-postgresql-backup-restore-runbook.md`, target env spec (doc 63) | Adapted target-host drill plan (this doc §5) | ☐ N/A (SQLite selected); readiness prep in [`117-postgresql-readiness-acceleration-plan.md`](./117-postgresql-readiness-acceleration-plan.md) |
| T1-P4 | If PG: Operator executes P5c.V1 on target | Operator | Adapted plan, target credentials, `pg_dump` | Completed `110-p5c-postgresql-drill-evidence-template.md` | ☐ N/A (SQLite selected) |
| T1-P5 | If PG: Operator executes P5c.V2 on target | Operator | Backup artifact from T1-P4, drill DB | Completed evidence template + restore log | ☐ N/A (SQLite selected) |
| T1-P6 | If SQLite: Explicit N/A waiver for B6/B7 | Operator | `105-g3-5-operator-d1-d3-signoff-packet.md` | Signed waiver acknowledging P5c deferred | ✅ Done — waived per doc113 §6 |

#### Track 1 — Checklist

- [x] **T1.1** (Eng) Create or obtain a populated SQLite fixture (≥100 rows across `intents`, `proposals`, `executions`) for local seeding.
- [x] **T1.2** (Eng) Run populated-data local P5c.V1: `pg_dump` with non-zero row counts, record size/checksum/`pg_restore -l`.
- [x] **T1.3** (Eng) Run populated-data local P5c.V2: restore into drill DB, verify row counts match source, cleanup.
- [x] **T1.4** (Eng) Record populated local evidence in artifact: [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md).
- [x] **T1.5** (Op + Eng) Phase 2 decision gate: operator selects SQLite (continue Path 2) or PostgreSQL (proceed to P5b–P5e). Decision recorded in [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`).
- [ ] **T1.6** (Eng, if PG) Adapt `111-p5c-local-docker-drill-plan.md` for target host: replace `localhost:5432` with target host, add `.pgpass` guidance, add scheduler verification — N/A (SQLite selected); readiness prep continues in [`117-postgresql-readiness-acceleration-plan.md`](./117-postgresql-readiness-acceleration-plan.md).
- [ ] **T1.7** (Op, if PG) Execute target-host P5c.V1 per adapted plan; fill `110-p5c-postgresql-drill-evidence-template.md` — N/A (SQLite selected).
- [ ] **T1.8** (Op, if PG) Execute target-host P5c.V2 per adapted plan; fill evidence template — N/A (SQLite selected).
- [x] **T1.9** (Op, if SQLite) Sign explicit N/A waiver for B6/B7 in `105-g3-5-operator-d1-d3-signoff-packet.md` refresh — waived per [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) §6.

#### Track 1 — Stop Conditions

| Trigger | Action |
|---|---|
| Populated local drill fails (T1-P1) | Do not proceed to target-host; investigate schema/migration mismatch |
| Operator defers Phase 2 decision >14 days | Document risk: P5c blockers remain open; Path 2 pilot cannot advance to full production |
| Target-host `pg_dump` fails (T1-P4) | Do not schedule backups; investigate credentials, disk, permissions |
| Target-host row counts differ post-restore (T1-P5) | Do not claim P5c.V2 pass; investigate backup consistency or schema drift |

#### Track 1 — Acceptance Criteria

| Criterion | Expected | Owner |
|---|---|---|
| Local populated drill produces non-zero row counts | `intents` + `proposals` + `executions` ≥ 100 rows total | Engineering |
| `pg_restore -l` lists all expected tables | 11 tables present | Engineering / Operator |
| Target-host backup exits 0 and `pg_restore -l` succeeds | true (if PG path) | Operator |
| Target-host restore row counts match source | ±0 (if PG path) | Operator |
| SQLite N/A waiver signed | Signed by operator (if SQLite path) | Operator |

---

### Track 2 — Populated-Data Local Drill (Stronger Local Evidence Before Target-Host)

> **Purpose**: Increase confidence in P5c mechanics by exercising backup/restore with realistic row counts before exposing operator to target-host risk.

#### Track 2 Summary

| Field | Value |
|---|---|
| Rationale | P5c local smoke was schema-only/0-row; populated data validates that `pg_dump`/`pg_restore` handle real tuples, indexes, and foreign keys |
| Leverage | Highest immediate return; no operator dependency; no target-host access needed |
| Owner | Engineering |
| Risk | Low — local Docker only; no production systems |

#### Track 2 — Phase Breakdown

| Sub-phase | Task | Owner | Inputs | Outputs | Status |
|---|---|---|---|---|---|
| T2-P1 | Generate or reuse populated SQLite fixture | Engineering | Existing test fixtures, migration schema | SQLite `.db` file with ≥100 rows | ✅ Done |
| T2-P2 | Seed local PostgreSQL from populated fixture | Engineering | `ferrum-migrate --features postgres`, fixture DB | Local PG with realistic data | ✅ Done |
| T2-P3 | Execute P5c.V1 backup with populated data | Engineering | `pg_dump` inside Docker container | Backup artifact with size > schema-only baseline | ✅ Done |
| T2-P4 | Execute P5c.V2 restore with populated data | Engineering | Backup artifact, drill DB | Restored DB with matching row counts | ✅ Done |
| T2-P5 | Record evidence artifact | Engineering | Drill logs, checksums, row counts | [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) | ✅ Done |

#### Track 2 — Checklist

- [x] **T2.1** Identify or create populated SQLite fixture. Minimum: 50 `intents`, 30 `proposals`, 20 `executions`, plus linked `capabilities`, `provenance_events`, `ledger_entries`.
- [x] **T2.2** Start local PostgreSQL container (`docker-compose.postgres.yml`).
- [x] **T2.3** Migrate from populated SQLite to local PostgreSQL:
  ```bash
  cargo run --package ferrum-migrate --features postgres -- \
    --from "sqlite:/path/to/populated_fixture.db" \
    --to "postgres://ferrumgate_dev@localhost:55432/ferrumgate_p2_test" \
    --apply --chunk-size 100
  ```
- [x] **T2.4** Verify row counts in local PG before backup.
- [x] **T2.5** Run `pg_dump -Fc -v --no-owner --no-privileges` inside container; copy artifact to host.
- [x] **T2.6** Record SHA-256, size, `pg_restore -l` object count.
- [x] **T2.7** Create drill DB; run `pg_restore`; verify all tables present.
- [x] **T2.8** Verify row counts in drill DB match source (±0).
- [x] **T2.9** Drop drill DB; stop container.
- [x] **T2.10** Write evidence artifact with sanitized commands (passwords redacted).

#### Track 2 — Stop Conditions

| Trigger | Action |
|---|---|
| Migration fails (T2.3) | Stop; fix migration or fixture before claiming populated drill valid |
| Row counts differ post-restore (T2.8) | Stop; investigate data loss, encoding issues, or migration bugs |
| Backup artifact smaller than schema-only baseline | Stop; investigate missing data or failed migration |

#### Track 2 — Acceptance Criteria

| Criterion | Expected | Evidence |
|---|---|---|
| Pre-backup row counts documented | `intents` ≥ 50, `proposals` ≥ 30, `executions` ≥ 20 | Screenshot or log excerpt |
| Backup exits 0, artifact > 20K | Size > schema-only baseline (`20K`) | `ls -lh` output |
| `pg_restore -l` succeeds | Object count ≥ 57 TOC entries | `pg_restore -l` output |
| Post-restore row counts match source | ±0 for all key tables | `SELECT COUNT(*)` output |
| No secrets in evidence artifact | Passwords redacted to `<REDACTED>` | Textual review |

---

### Track 3 — G3.6 Real Workload / Post-Deploy Monitoring Plan

> **Purpose**: Transition G3.6 from **conditionally accepted for P5b planning** (compile-only/light workload) to **real workload validation** with adapter execution paths exercised.

#### Track 3 Summary

| Field | Value |
|---|---|
| Current status | G3.6 **conditionally accepted** on 2026-05-11 for initial P5b planning only (`106-g3-6-pilot-metrics-evidence-packet.md`) |
| Gaps | No low/target/spike/cooldown metrics sequence; no adapter execution path workload; no remote-target load generator/Grafana configs |
| Owner | Operator (execution + environment); Engineering (load generator script, Grafana dashboard template, metrics review) |
| Conditionality | Full G3.6 completion is required before P5b–P5e pool tuning can be considered validated |

#### Track 3 — Phase Breakdown

| Sub-phase | Task | Owner | Inputs | Outputs | Status |
|---|---|---|---|---|---|
| T3-P1 | Define real workload profile | Operator + Engineering | Target use cases, adapter mix (FS/Git/HTTP/SQLite/Maildraft) | Workload profile doc with QPS target, adapter mix % | ☐ Ready to plan |
| T3-P2 | Engineering provides load generator script | Engineering | `ferrumctl`, API schema, bearer auth pattern | `scripts/run_real_workload_generator.py` or similar | ☐ Ready to start |
| T3-P3 | Engineering provides Grafana dashboard JSON | Engineering | Metrics schema from `/v1/metrics` | `configs/examples/grafana-ferrumgate.json` | ☐ Ready to start |
| T3-P4 | Operator deploys load generator + monitoring | Operator | Target host, Grafana/Prometheus stack | Running load generator + scraping metrics | ☐ Blocked on target host |
| T3-P5 | Execute baseline → low → target → spike → cooldown sequence | Operator | Load generator, monitoring stack | 5 metrics snapshots with all required counters | ☐ Blocked on T3-P4 |
| T3-P6 | Collect sustained write-rate histograms | Operator | Prometheus query API or manual scrape | p50/p95/p99 write rates under real workload | ☐ Blocked on T3-P5 |
| T3-P7 | Verify queue depth under load | Operator | Prometheus/metrics endpoint | `max_over_time(ferrumgate_write_queue_depth[1h])` at each phase | ☐ Blocked on T3-P5 |
| T3-P8 | Verify `readyz/deep` ≥ 99% under load | Operator | Probe script or Prometheus blackbox | Success rate % per phase | ☐ Blocked on T3-P5 |
| T3-P9 | Refresh G3.6 evidence packet | Operator + Engineering | All snapshots, rates, queue depths | Updated `106-g3-6-pilot-metrics-evidence-packet.md` or new artifact | ☐ Blocked on T3-P8 |
| T3-P10 | Operator re-signs G3.6 (full, not conditional) | Operator | Updated evidence packet | Signed G3.6 with adapter paths exercised | ☐ Blocked on T3-P9 |

#### Track 3 — Checklist

- [ ] **T3.1** (Eng) Draft workload profile template: adapter mix %, target QPS, duration per phase.
- [ ] **T3.2** (Eng) Build `scripts/run_real_workload_generator.py` supporting:
  - Configurable adapter mix (FS write, Git commit, HTTP POST, SQLite mutation, Maildraft create)
  - Configurable QPS ramp: baseline (0) → low (0.1 req/s) → target (1 req/s) → spike (5 req/s) → cooldown (0)
  - Bearer auth injection from env var
  - Structured JSON output of request latencies and outcomes
- [ ] **T3.3** (Eng) Export Grafana dashboard JSON covering:
  - `rate(ferrumgate_http_requests_total[1m])` by route
  - `ferrumgate_write_queue_depth`
  - `ferrumgate_store_health_up`
  - `ferrumgate_request_duration_seconds` histogram heatmap
  - `ferrumgate_governance_errors_total` rate
- [ ] **T3.4** (Op) Confirm Prometheus is scraping target `/v1/metrics` with bearer auth.
- [ ] **T3.5** (Op) Run baseline snapshot (idle, 0 load).
- [ ] **T3.6** (Op) Run low-load phase (0.1 req/s, 10 min).
- [ ] **T3.7** (Op) Run target-load phase (1 req/s, 30 min).
- [ ] **T3.8** (Op) Run spike-load phase (5 req/s, 5 min).
- [ ] **T3.9** (Op) Run cooldown phase (0 load, 10 min).
- [ ] **T3.10** (Op) Capture all 5 metrics snapshots to files.
- [ ] **T3.11** (Op) Compute sustained write rates from load generator output.
- [ ] **T3.12** (Op) Compute queue depth peaks from Prometheus or manual scrape.
- [ ] **T3.13** (Op) Compute `readyz/deep` success rate from probe logs.
- [ ] **T3.14** (Eng + Op) Review all data; update G3.6 evidence packet.
- [ ] **T3.15** (Op) Sign updated G3.6 as **COMPLETE** (not conditional) if A1–A6 fully met.

#### Track 3 — Stop Conditions

| Trigger | Action |
|---|---|
| Sustained write rate > 300 writes/s at target load | Abort single-node SQLite pilot; evaluate PostgreSQL path immediately |
| `readyz/deep` success rate < 95% at any phase | Investigate store health or write queue saturation before claiming G3.6 |
| Queue backlog > 100 sustained | Evaluate backpressure tuning or move to PostgreSQL |
| Load generator fails to exercise adapter paths | Do not claim real workload validation; fix generator or defer G3.6 |
| Metrics endpoint missing required counters | Upgrade build before collecting evidence |

#### Track 3 — Acceptance Criteria

| Criterion | Threshold | Evidence |
|---|---|---|
| A1 — ≥1h sustained write rate at target load | ≥ 1h observation window, adapter paths exercised | Load generator output + metrics snapshot |
| A2 — Queue depth at idle and target load | Peak and sustained values recorded per phase | Prometheus query or manual scrape |
| A3 — `readyz/deep` success rate | ≥ 99% over observation window | Probe log or blackbox exporter |
| A4 — Metrics snapshot at target load | All 5 required counters present | `/v1/metrics` output file |
| A5 — Backup verify + restore drill within RTO | Most recent backup OK, restore < operator RTO | `ferrumctl backup verify` + restore log |
| A6 — Operator signoff (full) | Signed without conditional caveats | Signature in updated evidence packet |

---

### Track 4 — SQLite Path 2 Target-Host Blockers (D1–D6, Restore Drill, Backup Automation, TLS, Bearer Token)

> **Purpose**: Close the 8 consolidated operator blockers from `66-path-2-operator-handoff.md` §B.0 for the SQLite single-node pilot path.

#### Track 4 Summary

| Field | Value |
|---|---|
| Blocker IDs | B1–B5 and B8 from `66-path-2-operator-handoff.md` (B6–B7 are PostgreSQL-specific) |
| Scope | Single-node SQLite only |
| Owner | Operator (execution); Engineering (template support, script verification) |
| Note | If operator selects PostgreSQL in Phase 2, B1–B5 and B8 may be deferred or adapted; this track assumes SQLite path |

#### Track 4 — Phase Breakdown

| Sub-phase | Task | Owner | Inputs | Outputs | Status |
|---|---|---|---|---|---|
| T4-P1 | Target-host D1–D6 evidence | Operator | `62-path-2-operator-runbook.md` §Phase 3, `scripts/run_d1_d6_drills.py` | Completed `58-workload-compensation-drill-evidence-template.md` | ☐ Blocked on target host |
| T4-P2 | SQLite restore drill with `PRAGMA integrity_check` | Operator | `61-path-2-execution-plan.md` §Step 3, `ferrumctl backup restore` | Restore drill log with `integrity_check: ok` | ☐ Blocked on target host |
| T4-P3 | Backup automation / external scheduler | Operator | `configs/examples/ferrumgate-backup.service`, `.timer`, `.cron` | Running cron or systemd timer; retention policy documented | ☐ Blocked on target host |
| T4-P4 | TLS/reverse proxy configuration | Operator | `configs/examples/nginx-ferrumgate.conf` or Caddy equivalent | TLS termination confirmed; probes pass through proxy | ☐ Blocked on target host |
| T4-P5 | Bearer token generation | Operator | `openssl rand -hex 32` | Token stored in `/etc/ferrumgate/ferrumd.env` (value NOT in docs) | ☐ Blocked on target host |
| T4-P6 | G3.6 real workload / post-deploy monitoring | Operator | Track 3 outputs | Post-deploy monitoring shows sustained workload without error | ☐ Blocked on Track 3 |

#### Track 4 — Checklist

- [ ] **T4.1** (Op) Confirm target host access (SSH, sudo).
- [ ] **T4.2** (Op) Complete `65-path-2-target-questionnaire.md` (all PROVIDE fields).
- [ ] **T4.3** (Op) Generate bearer token: `openssl rand -hex 32`; store in env file.
- [ ] **T4.4** (Op) Copy `configs/examples/ferrumd.service` to `/etc/systemd/system/`; adapt paths.
- [ ] **T4.5** (Op) Copy `configs/examples/ferrumgate-backup.service` + `.timer` to `/etc/systemd/system/`; adapt paths.
- [ ] **T4.6** (Op) Enable and start backup timer; verify first backup created.
- [ ] **T4.7** (Op) Configure TLS reverse proxy (nginx or Caddy) with real domain and TLS certs.
- [ ] **T4.8** (Op) Verify `/v1/readyz/deep` returns HTTP 200 through proxy.
- [ ] **T4.9** (Op) Run D1–D6 drills on target host using `run_d1_d6_drills.py` or manual execution.
- [ ] **T4.10** (Op) Capture drill output; fill `58-workload-compensation-drill-evidence-template.md`.
- [ ] **T4.11** (Op) Run SQLite restore drill: create backup, stop ferrumd, restore, verify `PRAGMA integrity_check`, restart, probe.
- [ ] **T4.12** (Op) Capture restore drill log.
- [ ] **T4.13** (Op) Fill `59-pilot-readiness-evidence-packet.md` G2.1–G2.8 with target-host evidence.
- [ ] **T4.14** (Op) Sign `54-operator-signoff-packet.md` (full signoff, not conditional, if all evidence complete).

#### Track 4 — Stop Conditions

| Trigger | Action |
|---|---|
| Any D1–D6 drill `fail_closed_verified: false` | Abort pilot; adapter implementation required before production use |
| Restore drill `integrity_check` fails | Do not proceed; investigate corruption or backup issue |
| Backup scheduler fails to produce backups | Do not begin pilot; fix scheduling first |
| TLS not configured before non-loopback exposure | Abort; do not expose without TLS |
| `readyz/deep` fails through reverse proxy | Fix proxy or FerrumGate health before signoff |

#### Track 4 — Acceptance Criteria

| Criterion | Expected | Evidence |
|---|---|---|
| D1–D6 drills completed with `recovered: true` or accepted risk | All drills executed | `58-workload-compensation-drill-evidence-template.md` |
| Restore drill: `integrity_check` passes | `ok` | Restore drill log |
| Backup scheduler: operational | Timer fires, backup file created, `verify` passes | Systemd status + backup listing |
| TLS: termination at reverse proxy | HTTPS probe passes, cert valid | `curl -I https://.../v1/readyz/deep` |
| Bearer token: generated and configured | `ferrumd` starts with `auth_mode=bearer` | Config file review (value redacted) |
| G3.6: real workload without error | Track 3 acceptance criteria met | Updated `106-g3-6-pilot-metrics-evidence-packet.md` |

---

## 4. Consolidated Master Checklist

### Phase 0 — Baseline (Done)

- [x] P0.1 P5c local smoke drill executed (schema-only)
- [x] P0.2 Evidence artifact committed (`artifacts/2026-05-12-p5c-local-docker-drill-evidence.md`)
- [x] P0.3 Explicit non-claims recorded (production-ready = NO, target-host = NOT CLOSED)

### Phase 1 — Populated Local Drill (Engineering-Owned, Done)

- [x] P1.1 Obtain or create populated SQLite fixture (≥100 rows)
- [x] P1.2 Seed local PostgreSQL from fixture
- [x] P1.3 Execute populated P5c.V1 backup
- [x] P1.4 Execute populated P5c.V2 restore
- [x] P1.5 Verify row counts match (±0)
- [x] P1.6 Write populated local evidence artifact — [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md)
- [x] P1.7 Textual self-check: no secrets, no production-ready claim

### Phase 2 — Operator Path Decision (Done — Option A SQLite Selected)

- [x] P2.1 Operator reviews Path 2 pilot outcome (workload rate, capacity)
- [x] P2.2 Operator selects SQLite (continue Path 2) or PostgreSQL (proceed to P5b–P5e)
- [x] P2.3 Decision recorded in [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`)
- [x] P2.4 If PostgreSQL: operator refreshes `105-g3-5-operator-d1-d3-signoff-packet.md` — N/A (SQLite selected)
- [x] P2.5 If SQLite: operator signs N/A waiver for B6/B7 — waived per doc113 §6

### Phase 3 — Target-Host Drill Prep & Execution (Partial Evidence — B1 Still Not Executed)

- [ ] P3.1 If PG: Engineering adapts drill plan for target host — N/A (SQLite selected); readiness prep continues in [`117-postgresql-readiness-acceleration-plan.md`](./117-postgresql-readiness-acceleration-plan.md)
- [ ] P3.2 If PG: Operator executes target-host P5c.V1 — N/A (SQLite selected)
- [ ] P3.3 If PG: Operator executes target-host P5c.V2 — N/A (SQLite selected)
- [ ] P3.4 SQLite target-host D1–D6 drills executed — **not executed**, remains operator-owned; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P3.5 SQLite target-host restore drill with `integrity_check` passed — partial evidence: safe temp-copy drill passed (`integrity_check: ok`, `table_count=0` caveat); see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P3.6 Backup automation configured and verified — partial evidence: timer enabled, latest backup present; retention pruning not verified; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P3.7 TLS/reverse proxy configured and probed — partial evidence: HTTPS probes pass, caddy active; operator-independent cert-path verification not done; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P3.8 Bearer token generated and deployed — partial evidence: token present, auth_mode=bearer; generation command not independently witnessed; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)

### Phase 4 — G3.6 Real Workload (Partial Evidence — Full Acceptance Not Achieved)

- [ ] P4.1 Engineering delivers load generator script — not yet delivered
- [ ] P4.2 Engineering delivers Grafana dashboard JSON — not yet delivered
- [ ] P4.3 Operator deploys load generator + monitoring — not yet done
- [ ] P4.4 Baseline → low → target → spike → cooldown sequence executed — **not executed**; bounded compile-only probe only; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P4.5 Sustained write-rate histograms collected — partial: p50 ~205.12ms from compile-only probe; adapter-mixed histograms not collected; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P4.6 Queue depth verified under load — partial: idle/post-workload depth=0; peak under target/spike load not measured; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P4.7 `readyz/deep` success rate ≥ 99% confirmed — partial: probes passed during evidence script; not measured across full workload sequence; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md)
- [ ] P4.8 G3.6 evidence packet refreshed with real workload data — not yet done

### Phase 5 — Consolidation & Conditional Signoff (Blocked on All Prior)

- [ ] P5.1 All evidence artifacts linked and cross-referenced
- [ ] P5.2 `59-pilot-readiness-evidence-packet.md` G2.1–G2.8 updated with target-host evidence
- [ ] P5.3 `54-operator-signoff-packet.md` signed (full or conditional as appropriate)
- [ ] P5.4 This plan updated with actual dates and outcomes
- [ ] P5.5 Final textual review: no secret values, no production-ready claim, no fabricated evidence

---

## 5. Next Immediate Execution Steps

These steps are **executable now** (no operator dependency, no target-host access):

| # | Step | Owner | ETA | Evidence |
|---|---|---|---|---|
| 1 | Create populated SQLite fixture or identify existing test fixture with ≥100 rows | Engineering | 1h | ✅ Done — fixture created |
| 2 | Run Track 2 populated-data local drill (T2.1–T2.10) | Engineering | 2h | ✅ Done — [`artifacts/2026-05-12-p5c-populated-local-drill-evidence.md`](./artifacts/2026-05-12-p5c-populated-local-drill-evidence.md) |
| 3 | Draft `scripts/run_real_workload_generator.py` scaffold (T3.2) | Engineering | 4h | ☐ Ready to start |
| 4 | Draft `configs/examples/grafana-ferrumgate.json` (T3.3) | Engineering | 2h | ☐ Ready to start |
| 5 | Update this plan with actual Track 2 execution dates and outcomes | Engineering | 0.5h | ✅ Done — this edit |
| 6 | Present Track 2 evidence + Phase 2 decision brief to operator | Engineering | 0.5h | ✅ Done — Option A recorded in [`113-operator-path-selection-packet.md`](./113-operator-path-selection-packet.md) (commit `b320f5c`) |

---

## 6. Inputs & Outputs

### Inputs

| Input | Source | Purpose |
|---|---|---|
| `artifacts/2026-05-12-p5c-local-docker-drill-evidence.md` | Local smoke commit `1e7adca` | Phase 0 baseline |
| `66-path-2-operator-handoff.md` §B.0 | Operator handoff doc | Blocker checklist (B1–B8) |
| `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 conditional acceptance | Baseline for Track 3 real workload plan |
| `109-p5c-postgresql-backup-restore-runbook.md` | P5c runbook | Commands and acceptance criteria for Track 1 |
| `110-p5c-postgresql-drill-evidence-template.md` | Fillable template | Evidence structure for Track 1 target drills |
| `111-p5c-local-docker-drill-plan.md` | Local drill plan | Procedure basis for Track 2 and Track 1 adaptation |
| `61-path-2-execution-plan.md` | Path 2 execution plan | Ordered steps for Track 4 |
| `67-production-readiness-roadmap.md` | Roadmap | Priority context (P0–P3) |

### Outputs

| Output | Producer | Consumer |
|---|---|---|
| `artifacts/2026-XX-XX-p5c-populated-local-drill-evidence.md` | Engineering | Operator (confidence), Track 1 planning |
| `scripts/run_real_workload_generator.py` | Engineering | Operator (Track 3 execution) |
| `configs/examples/grafana-ferrumgate.json` | Engineering | Operator (Track 3 monitoring) |
| Adapted target-host PG drill plan (edit to `111` or new doc) | Engineering | Operator (Track 1 target execution) |
| Updated `106-g3-6-pilot-metrics-evidence-packet.md` | Operator + Engineering | P5b engineering validation |
| Completed operator blockers B1–B8 | Operator | Phase 5 signoff |

---

## 7. Risk Register

| ID | Risk | Likelihood | Impact | Mitigation | Owner |
|---|---|---|---|---|---|
| R1 | Operator defers Phase 2 decision indefinitely | Medium | High — blocks all target-host progress | Set 14-day decision window; document default = SQLite continuation | Engineering |
| R2 | Populated local drill reveals migration/data-loss bug | Low | High — blocks P5c confidence | Stop and fix migration before target-host exposure | Engineering |
| R3 | Load generator script cannot exercise all adapter paths | Medium | Medium — G3.6 remains conditional | Document which adapters are covered; defer uncovered adapters to post-P5b | Engineering |
| R4 | Target-host `pg_dump` unavailable or version-mismatched | Medium | Medium — blocks Track 1 target | Use client tools inside container; document minimum version | Operator |
| R5 | Real workload exceeds 300 writes/s on SQLite | Low | High — forces PostgreSQL path | Monitor during low-load phase; abort early if threshold approached | Operator + Engineering |
| R6 | Secrets accidentally committed in evidence artifacts | Low | High — security incident | Textual self-check required on every artifact; redact all credentials | Engineering + Operator |

---

## 8. Cross-References

| This Doc | Links To | Purpose |
|---|---|---|
| `112-post-p5c-completion-execution-plan.md` | `artifacts/2026-05-12-p5c-local-docker-drill-evidence.md` | Phase 0 baseline evidence |
| `112-post-p5c-completion-execution-plan.md` | `66-path-2-operator-handoff.md` | Blocker ownership and checklist |
| `112-post-p5c-completion-execution-plan.md` | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 baseline and real workload plan |
| `112-post-p5c-completion-execution-plan.md` | `109-p5c-postgresql-backup-restore-runbook.md` | P5c commands and acceptance criteria |
| `112-post-p5c-completion-execution-plan.md` | `110-p5c-postgresql-drill-evidence-template.md` | Fillable evidence template |
| `112-post-p5c-completion-execution-plan.md` | `111-p5c-local-docker-drill-plan.md` | Local drill procedure |
| `112-post-p5c-completion-execution-plan.md` | `61-path-2-execution-plan.md` | Path 2 ordered execution |
| `112-post-p5c-completion-execution-plan.md` | `67-production-readiness-roadmap.md` | Priority and blocker context |
| `112-post-p5c-completion-execution-plan.md` | `31-release-paths-todo.md` §Path 3 | G3 gates and Phase 3 decision |
| `112-post-p5c-completion-execution-plan.md` | `55-phase-3-go-no-go-review.md` | Phase 2 decision recording |
| `112-post-p5c-completion-execution-plan.md` | `113-operator-path-selection-packet.md` | Phase 2 operator path decision packet |
| `112-post-p5c-completion-execution-plan.md` | `114-target-host-p5c-drill-checklist.md` | Track 1 target-host P5c drill checklist |
| `112-post-p5c-completion-execution-plan.md` | `115-sqlite-path2-target-host-checklist.md` | Track 4 SQLite target-host blocker checklist |
| `112-post-p5c-completion-execution-plan.md` | `116-g36-monitoring-execution-plan.md` | Track 3 G3.6 real workload monitoring plan |
| `112-post-p5c-completion-execution-plan.md` | `117-postgresql-readiness-acceleration-plan.md` | PostgreSQL readiness acceleration (parallel to Path 2 pilot) |

---

## 9. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial phased completion execution plan for four tracks post-P5c | Engineering |
| 2026-05-12 | Target-host execution attempted from runner IP `118.68.117.136`; blocked by SSH firewall (`118.69.4.63/32` only) and absent bearer token. See [`artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md`](./artifacts/2026-05-12-sqlite-path2-target-host-blocked-attempt.md). B1–B5 and B8 remain open. | Engineering |
| 2026-05-12 | Updated Phase 1/2 status: populated local drill completed, Option A SQLite path selected (commit `b320f5c`). Phase 3/4 remain blocked on target host. PostgreSQL readiness linked to [`117-postgresql-readiness-acceleration-plan.md`](./117-postgresql-readiness-acceleration-plan.md). | Engineering |
| 2026-05-12 | Firewall unblocked; SSH OK; Phase3E evidence script passed; safe temp restore drill done (`table_count=0` caveat); authenticated compile-only probe executed. Phase 3/4 updated to "partial evidence". B1 still not executed. G3.6 full acceptance not claimed. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](./artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md). | Engineering |

---

*Document updated: 2026-05-12. Post-P5c Completion Execution Plan — planning artifact only. No execution claimed. No production-ready claim. Target-host blockers remain operator-owned.*
