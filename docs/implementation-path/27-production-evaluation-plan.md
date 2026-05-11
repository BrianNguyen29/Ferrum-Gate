# 27 — Production Evaluation Plan

Single-node v1 scope. Phase 1 SQLite write queue. Phase 2 deferred/regressed.

This document is the canonical production evaluation framework for FerrumGate v1
single-node deployment. It operationalizes the conditional production posture described
in `23-production-readiness-assessment.md` and the accepted risks documented in
`19-v1-single-node-support-contract.md` §4 and `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
Weak Spots 1–4.

**Use this plan** before any production deployment decision. Each section lists
prerequisites, checks, and pass criteria. A production deployment is justified only
when all critical items in every dimension are satisfied or formally accepted with
documented compensating controls.

---

## Production Posture Summary

| Item | Status |
|---|---|
| Production architecture | Phase 1 SQLite write queue |
| Phase 2 (transaction batching + direct UPDATE) | **Deferred/regressed** — perf regression in benchmarking |
| Phase 3 (PostgreSQL) | Path to full production scale |
| RC status | RC-ready (single-node SQLite) |
| Production claim | **Conditional** — operational constraints (SQLite single-node write limits, bounded offline/local `ferrumctl backup` workflow with opt-in retention pruning (`--retention-days N`), no automated scheduling, no encryption; PostgreSQL/multi-node deferred; operator sign-off required) require evaluation |

---

## Dimension 1 — Performance

### 1.1 Stress test evidence (Phase 1 baseline)

All scenarios from the full `ferrum-stress` suite show 0% errors post-Phase-1
write queue. See `docs/PRODUCTION_NOTES.md` for the complete before/after table.

Key thresholds:

| Scenario | Workers | Min Throughput | Max p50 Latency | Max Error Rate |
|---|---|---|---|---|
| S4 intent-compile | 5 | 300 req/s | 5ms | 0% |
| S5 execution-pipeline (6 steps) | 5 | 50 pipelines/s | 50ms | 0% |
| S6 capability mint→revoke | 5 | 40 req/s | 1ms | 0% |
| S7 sqlite-contention (ingest writes) | 50 | 250 req/s | 100ms | 0% |
| S9 mixed workload | 5 | 100 req/s | 10ms | 0% |

**Pass**: All above thresholds met in release build stress tests.
**Fail**: Any scenario below threshold indicates write queue degradation.

### 1.2 Workload profile fit

Phase 1 SQLite write queue is appropriate for:

- **Appropriate**: Low-to-medium write throughput (≤300 writes/s sustained), single-node, file-backed SQLite, bounded execution history.
- **Not appropriate**: High sustained write throughput (>500 writes/s), multi-node or HA topology, read-replica queries, large execution history with complex lineage traversal.

**Action**: Model expected production workload against the stress test evidence. If write rate exceeds Phase 1 capacity or multi-node is required, defer to Phase 3 (PostgreSQL).

### 1.3 No performance regression since Phase 1

**Check**: Confirm no changes to the write queue, SQLite PRAGMA configuration, or migration pipeline since the stress test evidence was captured (2026-03-30).

**Action**: Review recent commits or diffs to `crates/ferrum-gateway/src/server.rs` write queue section, `crates/ferrum-store/src/sqlite/` PRAGMA initialization, and migration version.

---

## Dimension 2 — Security

### 2.1 Authentication

- Bearer token mode validated: `auth_mode = "Bearer"` with constant-time token comparison.
- Health endpoints (`/v1/healthz`, `/v1/readyz`) intentionally unauthenticated — verify these are not used for any governance operation.
- No anonymous mutation endpoints exposed.

**Pass criteria**: Token validation is constant-time; no governance routes bypass auth.

### 2.2 Rate limiting

- Built-in `tower_governor`: 2 req/s sustained, burst of 50.
- Applied per-IP via `GovernorLayer`.
- Periodic cleanup every 60s.

**Pass criteria**: Rate limit configuration matches production policy. Verify burst allowance is appropriate for expected traffic patterns.

### 2.3 Capability TTL enforcement

- Maximum TTL: 300 seconds (hardcoded in `ferrum-cap` service).
- Expired capabilities return `CapabilityExpired` error.

**Pass criteria**: TTL enforcement verified. No way to extend TTL beyond 300s.

### 2.4 Scope-bounds enforcement (Invariant 5 / Weak Spot 3 — RESOLVED)

The scope-bounds mismatch control is implemented (`crates/ferrum-pdp/src/engine.rs:31-46`)
and verified by integration tests. Single-use capability enforcement at the authorize
step is now wired via `mark_capability_used_durable` (`server.rs:751-757`).

**Pass criteria**: No additional action required.

### 2.5 Output sanitization (Invariant 11 — VERIFIED)

Trait-level `sanitize_output` is implemented in `crates/ferrum-firewall/src/lib.rs` with
direct unit tests for control chars, nested JSON, and structure preservation.
Bounded gateway response-path integration is implemented and verified for targeted high-risk
endpoints; broader adapter/response coverage remains a post-v1 hardening item.

**Pass criteria (conditional)**: Trait-level sanitization is verified; gateway integration
is deferred. If the gateway will handle untrusted output from adapters or external systems
in the v1 timeframe, output sanitization must be added as a separate security layer
outside FerrumGate v1, or the risk must be formally accepted.

---

## Dimension 3 — Reliability

### 3.1 FK constraint integrity

The FK chain (`intents → proposals → capabilities → executions → rollback_contracts`)
is enforced synchronously. All FK parent inserts use the write queue and return 200
only after persistence.

**Pass criteria**: No orphaned child records possible through normal API flows.

### 3.2 Write queue stability

Phase 1 uses an in-process `mpsc` write queue with 20-connection pool,
5000ms busy_timeout, WAL mode, and PRAGMA tuning (`synchronous=NORMAL`,
`wal_autocheckpoint=1000`, `cache_size=-64000`).

**Pass criteria**: Queue drain keeps pace with write rate. Monitor for queue
backlog under sustained load before declaring production-ready.

### 3.3 Rollback class handling (Weak Spot 1 — RESOLVED)

Gateway prepare loads `rollback_class` from `proposal.requested_rollback_class`
(`server.rs:856-872`). The R3 `auto_commit=false` control is correctly applied.
The intent creation service (caller) remains responsible for setting the correct
`rollback_class` at intent creation.

**Pass criteria**: No additional action required beyond ensuring callers set
`rollback_class` correctly at intent creation (documented constraint).

### 3.4 Draft-only revalidation (Weak Spot 2 — RESOLVED)

Prepare handler revalidates draft-only status (`server.rs:874-898`), rejecting
with HTTP 403 if `intent.approval_mode == DraftOnly`.

**Pass criteria**: No additional action required.

### 3.5 Backup and restore

`ferrumctl backup` provides a bounded SQLite backup/restore workflow:
- `backup create` — uses rusqlite backup API for consistent snapshot; sets 0600 permissions
- `backup verify` — runs `PRAGMA integrity_check`
- `backup restore` — requires `--confirm`; exclusive lock detection (refuses if server running);
  preserves pre-restore copy; verifies restored DB

Limitations: SQLite-only (no PostgreSQL backup), no incremental backup, no built-in
scheduling, opt-in CLI retention pruning (`--retention-days N`), no encryption.

**Pass criteria**: Operator uses `ferrumctl backup` for create/verify/restore workflow.
RPO (Recovery Point Objective) is understood — any state created after the backup
timestamp is lost on restore. No production claim beyond bounded offline workflow.

**External scheduling**: Backup scheduling and retention are operator-owned external
concerns in v1. See [18-single-node-operations-runbook.md §5.4](../ferrumgate-roadmap-v1/18-single-node-operations-runbook.md#54-external-scheduling-operator-owned) for cron and systemd timer examples.

### 3.6 Compensate may be noop-backed

`POST /v1/executions/{execution_id}/compensate` may return 200 without performing
external undo depending on adapter implementation and rollback class.

**Pass criteria**: Operator verifies resource state manually after compensate.
If guaranteed external undo is required, adapter implementation must be completed
and verified before production use of R1/R2/R3 compensation flows.

---

## Dimension 4 — Operations

### 4.1 CLI surface (inspect-only)

`ferrumctl` provides read/inspect commands only. No mutating commands exist in v1.

**Pass criteria**: Operator uses REST API for all mutating operations. `ferrumctl`
used for health probe, execution inspection, approval queries, lineage queries,
and provenance queries.

### 4.2 Health endpoint depth

`GET /v1/healthz` and `GET /v1/readyz` are shallow — they confirm the server
process is alive and the HTTP endpoint is reachable, but do not validate the
store, migrations, or governance loop.

`GET /v1/readyz/deep` provides a bounded deep readiness probe that verifies
the SQLite store is reachable via a `SELECT 1` query. It returns HTTP 200
when the store is healthy and HTTP 503 when the store check fails.

**Pass criteria**: A functional probe (`ferrumctl server inspect-execution` or
`GET /v1/approvals`) is used after startup to confirm end-to-end readiness.
Do not rely on `healthz`/`readyz` alone for governance loop health. The
`readyz/deep` endpoint is available as an opt-in bounded store probe.

### 4.3 Provenance completeness verification (Weak Spot 4 — RESOLVED)

`test_lineage_chain_full_provenance_events` (`integration_lineage_chain.rs:643-945`)
exercises the full authorize → prepare → execute → verify chain and asserts all
6 event kinds appear in the lineage query response, linked to execution_id.

**Pass criteria**: No additional action required.

### 4.4 Operational monitoring baseline

Minimum observability for production:

- Write queue depth / queue lag metric
- SQLite write latency distribution (p50, p99)
- Error rate per scenario (especially S4/S5/S6/S7)
- Approval queue depth
- Provenance event ingestion rate

See `../ferrumgate-roadmap-v1/21-v1-single-node-observability-minimums.md` for full list.

---

## Dimension 5 — Release Confidence

### 5.1 Workspace quality gate

- `cargo check --workspace` passes
- `cargo fmt --all --check` passes
- `cargo clippy --workspace --all-targets -- -D warnings` passes
- `cargo test --workspace` passes (~797 observed tests)

**Pass criteria**: All above pass in the release build. Evidence: fresh P6 validation (2026-04-28).

### 5.2 Contract consistency

- OpenAPI spec synced to actual routes/auth
- Schema definitions up to date
- Support contract canon intact

**Pass criteria**: `scripts/generate_rc_evidence.py` passes all five checks.
Evidence in `docs/artifacts/2026-03-30/05-contract-consistency.txt`.

### 5.3 Governance behavior verified

Critical behaviors verified by integration tests:

- Scope-mismatch deny (empty scope + non-R0 mutation = Deny)
- Single-use capability returns AlreadyUsed on reuse
- R3 contracts have auto_commit=false; R0 have auto_commit=true
- Rollback and compensate are distinct adapter operations
- High taint score (≥70) triggers Quarantine for non-R0
- Compensate end-to-end flow (state transitions verified)
- Pending approvals pagination and filter by proposal_id

**Pass criteria**: All behaviors confirmed by integration test evidence
(reference `16-release-checklist.md` lines 18–27).

### 5.4 Supported flows documented

The full list of supported governance flows is documented in
`docs/implementation-path/25-EV-v1-single-node-rc-evidence.md` Evidence 9.

**Pass criteria**: Operator confirms the target production workflow is listed
in the supported flows. Unsupported flows must not be used in production.

### 5.5 Post-v1 backlog review

Remaining tasks are documented in `docs/implementation-path/11-remaining-tasks.md`
P3 items.

**Pass criteria**: Operator has reviewed P3 items and confirmed none of the
planned improvements are required for the target production workload.

---

## Evaluation Decision Framework

For each dimension above, mark each item as:

| Symbol | Meaning |
|---|---|
| **SATISFIED** | Pass criteria met; no action needed |
| **CONDITIONAL** | Pass criteria require compensating control or operational procedure; document the control |
| **NOT MET** | Gap requires resolution before production deployment |
| **N/A** | Does not apply to the target workload |

**Overall production readiness**: All critical items must be SATISFIED or CONDITIONAL
(with documented controls). Any NOT MET item blocks production deployment.

---

## Engineer-Side Pre-Fill Table (Advisory / Repo-Side Only)

> **Note**: This table is **repo-side tooling validation only**. It provides engineers with a pre-filled assessment of the production evaluation dimensions to facilitate operator handoff. This table does **not** replace operator signoff and does **not** claim production readiness. All G2 gates remain **operator signoff still required** before any production pilot begins.

| Dimension | Item | Pre-fill Status | Repo-Side Notes |
|---|---|---|---|
| **1. Performance** | Stress test evidence (Phase 1 baseline) | Pre-filled from `docs/PRODUCTION_NOTES.md` | All S4/S5/S6/S7/S9 thresholds met |
| **1. Performance** | Workload profile fit | [ ] FIT  [ ] DEFER TO PG | ≤300 writes/s = FIT; >300 = PostgreSQL |
| **1. Performance** | No regression since Phase 1 | [ ] CONFIRMED | Write queue + PRAGMAs unchanged since 2026-03-30 |
| **2. Security** | Bearer token mode | [ ] CONFIGURED | `auth_mode = "Bearer"`; constant-time comparison |
| **2. Security** | Rate limiting | [ ] CONFIGURED | tower_governor: 2 req/s sustained, burst 50 |
| **2. Security** | Capability TTL | [ ] ENFORCED | Hardcoded max 300s in `ferrum-cap` |
| **2. Security** | Scope-bounds enforcement | [ ] VERIFIED | PDP engine control + `mark_capability_used_durable` |
| **2. Security** | Output sanitization | [ ] CONDITIONAL | Trait-level implemented; gateway integration deferred |
| **3. Reliability** | FK constraint integrity | [ ] VERIFIED | Synchronous FK chain; no orphaned children |
| **3. Reliability** | Write queue stability | [ ] VERIFIED | WAL + PRAGMA tuning; 5000ms busy_timeout |
| **3. Reliability** | Rollback class handling | [ ] VERIFIED | R3 `auto_commit=false` wired; caller sets rollback_class |
| **3. Reliability** | Draft-only revalidation | [ ] VERIFIED | Prepare handler rejects DraftOnly with HTTP 403 |
| **3. Reliability** | Backup/restore | [ ] VERIFIED | `ferrumctl backup` workflow bounded; no auto-scheduling |
| **3. Reliability** | Compensate may be noop | [ ] ACCEPTED-RISK | Operator must verify per-adapter compensate behavior |
| **4. Operations** | CLI surface (inspect-only) | [ ] VERIFIED | `ferrumctl` read/inspect only; mutating via REST API |
| **4. Operations** | Health endpoint depth | [ ] VERIFIED | `healthz`/`readyz` shallow; `readyz/deep` bounded store probe |
| **4. Operations** | Provenance completeness | [ ] VERIFIED | Full lineage chain (6 event kinds) verified |
| **4. Operations** | Observability baseline | [ ] RECOMMENDED | Monitor queue depth/lag, write latency, error rates |
| **5. Release Confidence** | Workspace quality gate | [ ] PASSING | check/fmt/clippy/test all pass |
| **5. Release Confidence** | Contract consistency | [ ] PASSING | `generate_rc_evidence.py` all 5 checks pass |
| **5. Release Confidence** | Governance behavior verified | [ ] VERIFIED | 7 critical behaviors confirmed by integration tests |
| **5. Release Confidence** | Supported flows documented | [ ] VERIFIED | Full list in `25-EV-v1-single-node-rc-evidence.md` Evidence 9 |
| **5. Release Confidence** | Post-v1 backlog reviewed | [ ] RECOMMENDED | P3 items in `11-remaining-tasks.md`; none blocking |

**Pre-fill engineer**: _____________________________ **Date**: ___________

**This table is advisory only. Operator signoff still required for all G2 gates.**

---

## Quick Checklist

Before production deployment, confirm all of:

- [ ] Write workload fits Phase 1 SQLite capacity (≤300 writes/s sustained)
- [ ] Phase 2 not required for target workload (otherwise defer to PostgreSQL)
- [ ] Token auth configured with constant-time comparison
- [ ] Rate limits appropriate for production traffic
- [ ] `rollback_class` correctness delegated to caller; this constraint is documented
- [ ] Backup/restore procedure documented and tested using `ferrumctl backup`
- [ ] Health probe functional (not shallow-only) used for readiness confirmation
- [ ] Compensate noop risk accepted with manual verification procedure
- [ ] `cargo check/fmt/clippy/test` passes in release build
- [ ] Target workflow is in the supported flows list

---

## Operator Signoff Packet

**Purpose**: This packet is the formal operator acceptance checklist before any production
pilot. It is documentation-only — completing these items does not claim production readiness;
it confirms the operator has evaluated and accepted the known constraints.

**Do not mark these items complete on behalf of the operator.** Each item requires explicit
operator acknowledgment and, where indicated, documented accepted-risk signoff.

### 1. SQLite Single-Node Limits — Operator Must Acknowledge

| Item | Required Action | Reference |
|---|---|---|
| Write throughput ceiling | Operator confirms expected sustained writes ≤300 writes/s; above this requires PostgreSQL (Phase 3) | Dimension 1, §1.2 |
| Single-node only | Operator acknowledges no multi-node/HA/replica support in v1 | Support contract §3 |
| Bounded execution history | Operator acknowledges SQLite file size and lineage traversal limits at scale | Support contract §3 |

**Signoff phrase required**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

---

### 2. Authentication and Transport Security

| Item | Required Action | Reference |
|---|---|---|
| Bearer token mode | Operator confirms `auth_mode = "Bearer"` with operator-managed token; `FERRUMD_BEARER_TOKEN` or config file | Dimension 2, §2.1 |
| TLS/reverse proxy | Operator confirms FerrumGate is deployed behind a TLS-terminating reverse proxy (not exposed bare on internet) | Dimension 2, §2.1 |
| Health endpoints unauthenticated | Operator acknowledges `/v1/healthz` and `/v1/readyz` are intentionally unauthenticated; governance routes require auth | Dimension 2, §2.1 |

**Signoff phrase required**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

---

### 3. Backup, Restore, and Recovery Objectives

| Item | Required Action | Reference |
|---|---|---|
| Backup schedule outside FerrumGate | Operator implements backup scheduling external to FerrumGate (cron, CI job, etc.); `ferrumctl backup` does not support automated scheduling | Dimension 3, §3.5 |
| Backup retention | Operator defines retention policy; opt-in CLI retention pruning (`--retention-days N`) available | Dimension 3, §3.5 |
| Restore drill performed | Operator has run `ferrumctl backup restore` in a non-production environment and verified data integrity with `PRAGMA integrity_check` | Operations runbook §4 |
| RPO accepted | Operator understands RPO = time since last backup; any writes after last backup are lost on restore | Dimension 3, §3.5 |
| RTO accepted | Operator understands RTO includes backup restore time + re-start + verification; FerrumGate has no automated recovery | Dimension 3, §3.5 |

**Signoff phrase required**: "Operator has performed a restore drill, confirmed RPO/RTO fit for the target workload, and backup retention policy (including scheduling and offsite needs) is operator-defined."

---

### 4. PostgreSQL / Multi-Node Deferred Status

| Item | Required Action | Reference |
|---|---|---|
| PostgreSQL runtime support (local) | Operator acknowledges local PostgreSQL runtime support exists (`postgres://` DSNs connect at startup) but production deployment, HA/multi-node, and P4.4 data migration remain deferred | ADR-50, `31-release-paths-todo.md` |
| Multi-node/HA not implemented | Operator acknowledges v1 is single-node only; scale-out requires Phase 3 | ADR-50, Production roadmap |
| Phase 3 local runtime complete | Operator acknowledges local PostgreSQL runtime support (P3/P4.1–P4.3) is implemented but production deployment, P4.4 data migration, and P5 production readiness remain deferred and are not part of the current pilot | ADR-50 Phase P1, `31-release-paths-todo.md` |

**Signoff phrase required**: "Operator acknowledges PostgreSQL/multi-node is deferred and not part of the current production pilot scope."

---

### 5. Production Pilot Prerequisites

Before the first production pilot deployment, the following must be true:

| # | Prerequisite | Verification |
|---|---|---|
| 1 | Write workload modeled against SQLite capacity (≤300 writes/s sustained) | Operator signoff on §1 above |
| 2 | Bearer auth + TLS/reverse proxy confirmed | Operator signoff on §2 above |
| 3 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` |
| 4 | Restore drill completed with `PRAGMA integrity_check` passing | Operator evidence of successful restore |
| 5 | RPO/RTO formally accepted for target workload | Operator signoff on §3 above |
| 6 | All production evaluation dimensions SATISFIED or CONDITIONAL (with controls) | This document's Evaluation Decision Framework completed |
| 7 | Accepted-risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed |
| 8 | Compensate noop risk formally accepted | Operator acknowledges compensate may be noop-backed for target adapters |

**This is not a production-ready claim.** FerrumGate v1 is RC-ready with known accepted risks.
Production pilot deployment is conditional on all eight prerequisites above being satisfied.

---

## Conditional Next-Step Decision: RC Tag / Release Notes vs Phase 3 PostgreSQL

### Decision Tree

```
Is the production pilot complete and are you ready to cut an RC tag / release notes?
│
├── NO → Continue pilot evaluation; re-run this checklist before any release decision
│
└── YES → Are you also ready to start Phase 3 (PostgreSQL/multi-node implementation)?
          │
          ├── NO → Cut RC tag / publish release notes for v1 single-node SQLite only
          │        Refer to `25-EV-v1-single-node-rc-evidence.md` for evidence base
          │        Refer to `23-production-readiness-assessment.md` for RC-ready declaration
          │
          └── YES → Begin Phase 3 PostgreSQL implementation
                    First step: ADR-50 Phase P1 — PostgreSQL migrations + testcontainer strategy
                    See `docs/implementation-path/50-p4-postgres-store-facade-adr.md` §3 Phase P1
```

### What "RC Tag / Release Notes" Means for v1

- RC tag is for the **current v1 single-node SQLite release candidate**
- Release notes must document:
  - Supported scope: single-node SQLite only
  - Deferred items: Phase 2 transaction batching, Phase 3 PostgreSQL/multi-node
  - Known constraints: backup scheduling external, no automated restore, compensate noop risk
  - Auth requirements: bearer token mode, TLS/reverse proxy required

### What Phase 3 PostgreSQL Is NOT

- Phase 3 is **NOT** an extension of the v1 RC tag
- Phase 3 is **NOT** a minor feature addition — it requires ~2000–3000 LOC + migrations + container tests (per ADR-50)
- Phase 3 is **NOT** covered by the current v1 support contract
- Starting Phase 3 does not imply v1 is production-ready; v1 RC tag remains a candidate requiring operator signoff

### If Phase 3 Starts: First Step

**ADR-50 Phase P1 — PostgreSQL migrations + testcontainer strategy**

Per `docs/implementation-path/50-p4-postgres-store-facade-adr.md` §3, Phase P1 deliverables are:
- [ ] Enable `sqlx::postgres` feature flag
- [ ] Create `PostgresStore` skeleton with placeholder repo implementations
- [ ] Define migration strategy (SQLite → PostgreSQL compatibility layer)
- [ ] Add container test infrastructure (Docker Compose for postgres)

Do not begin Phase P1 until v1 RC tag is cut and the production pilot has confirmed the single-node SQLite posture is acceptable.

---

## References

- Production notes: `docs/PRODUCTION_NOTES.md`
- RC readiness: `docs/implementation-path/23-production-readiness-assessment.md`
- Support contract: `../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`
- Invariant matrix: `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
- RC evidence: `docs/implementation-path/25-EV-v1-single-node-rc-evidence.md`
- Observability minimums: `../ferrumgate-roadmap-v1/21-v1-single-node-observability-minimums.md`
- Operations runbook: `docs/ferrumgate-roadmap-v1/18-single-node-operations-runbook.md`
- Release paths (RC tag / production pilot / Phase 3 PostgreSQL): `docs/implementation-path/31-release-paths-todo.md`
- Adapter compensation evidence: `docs/implementation-path/56-adapter-compensation-evidence-matrix.md`
- Workload compensation drill plan: `docs/implementation-path/57-workload-compensation-drill-plan.md`
- Workload compensation drill evidence template: `docs/implementation-path/58-workload-compensation-drill-evidence-template.md`
- Pilot readiness evidence packet (G2.1–G2.8): `docs/implementation-path/59-pilot-readiness-evidence-packet.md`
- Bounded hardening examples: `docs/implementation-path/60-bounded-hardening-examples.md`
