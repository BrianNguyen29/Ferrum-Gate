# ADR-50 — PostgreSQL StoreFacade: Phased Implementation Plan

> **Status**: P3 Repository Implementations — Complete (local Docker). P4.1 Runtime DSN switching — Complete. P4.2 Migration infrastructure — Complete. P4.3 Benchmark validation — Complete (3853.2 writes/s local Docker release). P4.4 SQLite→PostgreSQL migration MVP — Complete. P5a Production Readiness Design — GO for ADR/design only. G3.5 operator D1–D3 signoff satisfied (Option A defaults). Eng.1 capacity confirmed and Eng.2 plan approved via chat authorization. P5b–P5e Implementation — Gated on G3.6 (Eng.1 and Eng.2 satisfied).
> **Date**: 2026-05-11
> **Deciders**: Engineering implementation complete for local Docker/runtime; production/HA/multi-node posture remains NO. P5a design phase authorized; P5b–P5e blocked on G3.6 pilot data and engineering planning.
> **Estimated Effort**: P1–P4.4 ~2000-3000 LOC + migrations + container tests; P5a design only; P5b–P5e ~200-400 LOC + testing for D1=A/D2=A/D3=A (gated on G3.6/Eng.1/Eng.2)
> **Next Step**: G3.6 pilot metrics collection per `106-g3-6-pilot-metrics-evidence-packet.md`. No P5b–P5e until G3.6 is satisfied (Eng.1 and Eng.2 are now satisfied).

---

## 1. Context and Evidence

### Current State

- `StoreFacade` trait in `crates/ferrum-store/src/repos.rs` is **DB-agnostic**
- `SqliteStore` fully implements `StoreFacade` in `crates/ferrum-store/src/sqlite/mod.rs`
- `ferrumd` selects `SqliteStore` by default and `PostgresStore` for `postgres://`/`postgresql://` DSNs when built with the non-default `postgres` feature
- `PostgresStore` implements all 9 repos for local Docker/runtime; `MySqlStore` not implemented
- `sqlx` is configured for SQLite by default; `sqlx::postgres` feature enabled for non-default builds
- Config files show `postgres://` as a working local Docker example; `mysql://` is **not implemented**

### Evidence References

- `crates/ferrum-store/src/repos.rs:175-189` — `StoreFacade` trait
- `crates/ferrum-store/src/sqlite/mod.rs:312-375` — `SqliteStore` impl
- `bins/ferrumd/src/main.rs:236-240` — store connection
- `configs/ferrumgate.dev.toml:14` — postgres example comment
- `configs/ferrumgate.prod.toml:15` — postgres example comment
- `docs/implementation-path/45-current-feature-audit.md` — PostgreSQL local runtime implemented; production deployment / multi-node / HA deferred

### Gap

The codebase now has a working `PostgresStore` for local Docker/runtime. PostgreSQL production/HA/multi-node posture remains explicitly deferred. Oracle NO-GO verdict for full production implementation still stands.

---

## 2. Decision

### Historical P1 Action: Explicit Rejection (Superseded by P4.1)

During P1, DSN guardrails in `ServerConfig::validate()` explicitly rejected non-SQLite DSNs:

```
store_dsn "postgres://...": PostgreSQL is not implemented.
  See ADR-50 for the phased implementation plan.
  Use sqlite:// or sqlite::memory: for local development.
```

```
store_dsn "mysql://...": MySQL is not implemented.
  See ADR-50 for the phased implementation plan.
  Use sqlite:// or sqlite::memory: for local development.
```

**P4.1 Update**: Runtime DSN switching now enables `postgres://` for local Docker/runtime.
MySQL remains rejected.

### Rationale (P1)

1. **No false claims**: SQLite-only status was unambiguous at v1 RC
2. **Clear error path**: Users got actionable guidance instead of cryptic connection failures
3. **Low risk**: Validation happened at startup before any resource acquisition
4. **No overclaim**: No stub implementation was added

---

## 3. Phased Implementation Plan

> **Note**: Production/HA/full posture remains deferred. This plan documents completed local Docker/runtime work.

### Phase P1 — Guardrails (This Artifact)

- [x] Add DSN validation rejecting postgres:// and mysql://
- [x] Update config file comments to clarify not implemented
- [x] Update ADR-45/ADR-30 if needed
- [x] Document phased plan in this ADR

**Status**: Guardrails only — no runtime store implementation

---

### Phase P2 — Infrastructure Preparation (Post-v1)

**Prerequisites**: v1 stable release, production evaluation complete

**Goals**:
- [x] Enable `sqlx::postgres` feature flag (non-default, compile-time only)
- [x] Create `PostgresStore` skeleton with placeholder repo implementations
- [ ] Define migration strategy (SQLite → PostgreSQL compatibility layer?)
- [x] Add container test infrastructure (Docker Compose for postgres)

**Estimated Effort**: ~500 LOC infrastructure, ~200 LOC for skeleton stores

**Deliverables**:
- `crates/ferrum-store/src/postgres/mod.rs` — module skeleton ✅
- `crates/ferrum-store/src/postgres/*.rs` — skeleton repos (9 total) ✅
- `docker-compose.yml` or `P2_INFRA.md` for local postgres testing ✅

**P2 Status (2026-05-11)**: Skeleton infrastructure complete. All 9 repos are now
fully implemented for local Docker/runtime. Runtime PostgreSQL support is
implemented for local Docker; production/HA/multi-node remains deferred.

---

### Phase P3 — Repository Implementations (Post-P2)

**Goals**:
- [x] Implement `PostgresIntentRepo`
- [x] Implement `PostgresProposalRepo`
- [x] Implement `PostgresCapabilityRepo`
- [x] Implement `PostgresExecutionRepo`
- [x] Implement `PostgresRollbackRepo`
- [x] Implement `PostgresApprovalRepo`
- [x] Implement `PostgresProvenanceRepo`
- [x] Implement `PostgresLedgerRepo`
- [x] Implement `PostgresPolicyBundleRepo`

**Estimated Effort**: ~1500-2000 LOC (9 repos × ~150-200 LOC each + connection pooling) — **actual: complete**

**Key Considerations**:
- Write queue architecture must be adapted for PostgreSQL (different concurrency model) — **deferred**
- Connection pooling via `sqlx::Pool<Postgres>` — **implemented**
- Batch INSERT patterns for write queue — **deferred**

---

### Phase P4 — Migrations and Testing (Post-P3)

**Goals**:
- [x] Design SQLite → PostgreSQL data migration path
- [x] Implement embedded migration runner for postgres (schema creation)
- [x] Add integration tests with live postgres
- [x] Benchmark validation (target: 1000+ writes/s) — **achieved 3853.2 writes/s local Docker release**
- [x] Implement P4.4 MVP migration CLI (dry-run default, --apply, empty-target safety, count + ID validation)

**P4 Sub-phase Status**:
- P4.1 Runtime DSN switching — ✅ Complete
- P4.2 Migration infrastructure (schema runner) — ✅ Complete
- P4.3 Benchmark validation — ✅ Complete (3853.2 writes/s local Docker release)
- P4.4 Data migration (SQLite → PostgreSQL) — ✅ Complete (MVP only)

**P4.4 MVP Scope**:
- Standalone CLI binary: `bins/ferrum-migrate`
- Feature-gated by `--features postgres`; does not alter default non-postgres posture
- Dry-run by default; explicit `--apply` required to write to target
- `--apply` requires empty target for MVP and fails fast otherwise
- Migrates core governance records in dependency-safe order: intents, proposals, capabilities, executions, rollback_contracts, approvals, provenance_events, provenance_edges, ledger_entries, policy_bundles
- Count + stable ID-set validation implemented and tested
- Human-readable default output and `--json` output supported
- **Non-goals**: production-grade migration, idempotent/upsert/resume/checkpointing, content hash validation, large dataset streaming

**Estimated Effort**: ~300-500 LOC migrations + tests — **P4.1–P4.4 complete; P4.4 is MVP only**

---

### Phase P5 — Production Readiness (Post-P4)

> **Oracle verdict**: P5 GO for design/ADR only (P5a). P5b–P5e implementation requires G2 pilot data, G3 gate refresh, and operator D1–D3 signoff. P5 completion does not claim production-ready; P6 assessment required afterward.

#### 3.5 P5a — Design / ADR Review (Approved for Design Only)

P5a is the only currently authorized P5 subphase. It produces a design document, risk register, operator decision framework, and verification gates for P5b–P5e. G3.4 is satisfied by the approval packet; no P5b–P5e implementation begins until G3.5–G3.6 are satisfied.

> **Approval workflow**: G3.4 approval is recorded in [`104-g3-4-p5a-adr-approval-packet.md`](./104-g3-4-p5a-adr-approval-packet.md).
> That packet contains the structured review checklist, signoff fields, and explicit non-claims.
> G3.4 approval authorizes P5a design/ADR only and does not authorize P5b–P5e implementation.

**P5a Deliverables**:
- [x] P5a design doc (this ADR §3.5 or standalone doc) with D1–D6 decisions
- [x] Risk register with P5-specific risks (pool exhaustion, failover gaps, backup inconsistency, migration divergence)
- [x] Verification gates defined for P5b–P5e with pass/fail criteria
- [x] Operator decision framework D1–D6 drafted and ready for signoff
- [x] Non-claims language reviewed and preserved

**Operator Decisions (D1–D6)**:

| Decision | Question | Options | Default | Signoff Required |
|---|---|---|---|---|
| D1 | Target topology | Single-node PostgreSQL / Read replica / Full HA cluster | Single-node PostgreSQL | Operator |
| D2 | Backup strategy | `pg_dump` logical / Streaming replication / External tool (e.g., pgBackRest) | `pg_dump` logical | Operator |
| D3 | Failover requirement | None (single-node) / Manual failover / Automated failover | None | Operator |
| D4 | Pool sizing model | Fixed min/max / Dynamic based on pilot data | Dynamic (needs G3.6) | Engineering |
| D5 | Migration grade | MVP (P4.4) / Production-grade (P5e) | MVP until P5e authorized | Engineering + Operator |
| D6 | Production claim timeline | P5e complete + P6 assessment / Deferred beyond P5 | Deferred beyond P5 | Operator + Engineering |

**P5a Risks**:

| Risk | Impact | Mitigation | Owner |
|---|---|---|---|
| Pool exhaustion under pilot load | Connection timeouts, request failures | Size pool from G2 pilot metrics; add circuit breaker | Engineering |
| Failover gap not modeled | Data loss during failover if replication lag unchecked | Define RPO for replication lag; operator accepts | Operator |
| Backup inconsistency (concurrent writes) | Backup captures inconsistent state | Use PostgreSQL consistent snapshot or stop writes | Operator |
| Migration divergence (content hash mismatch) | SQLite and PostgreSQL states not equivalent | Content-hash validation in P5e; operator accepts MVP risk | Engineering + Operator |
| Operator D1–D3 not signed | P5b–P5e cannot begin | Gate on G3.5; do not proceed without signoff | Engineering lead |

**P5a Verification Gates**:

| Gate | Criterion | Evidence |
|---|---|---|
| P5a.V1 | D1–D6 decision framework documented and reviewed | Design doc §Operator Decisions |
| P5a.V2 | Risk register contains at least 4 P5-specific risks | Design doc §P5a Risks |
| P5a.V3 | P5b–P5e verification gates defined with pass/fail criteria | Design doc §P5b–P5e Verification Gates |
| P5a.V4 | Non-claims language reviewed by second party | Design doc signoff or review comment |

#### 3.5.1 P5b — Connection Pool Tuning (Implementation Gated)

**Goals**:
- [ ] Pool size model validated against pilot workload data (G2 metrics)
- [ ] `max_connections`, `min_idle`, `acquire_timeout` tuned for target throughput
- [ ] Connection-leak detection and circuit-breaker behavior defined

**Blocked until**: G3.6 pilot data available; Eng.1 capacity confirmed; Eng.2 implementation plan approved

**Estimated Effort**: ~100-200 LOC + configuration changes

**Verification Gates**:

| Gate | Criterion | Evidence |
|---|---|---|
| P5b.V1 | Pool config validated in local Docker stress test | Benchmark ≥1000 writes/s with tuned pool |
| P5b.V2 | No connection leaks observed in 30-min stress test | `sqlx` pool metrics or custom leak detector |
| P5b.V3 | Circuit breaker triggers within 5s on pool exhaustion | Integration test or manual verification |

#### 3.5.2 P5c — Backup / Restore for PostgreSQL (Implementation Gated)

**Goals**:
- [ ] `pg_dump`/`pg_restore` or logical-replication backup strategy documented
- [ ] Backup automation design (external scheduler, not in-tree)
- [ ] Restore drill procedure for PostgreSQL defined
- [ ] RPO/RTO targets for PostgreSQL documented and operator-accepted

**Blocked until**: Eng.1 capacity confirmed; Eng.2 implementation plan approved; P5b design complete

**Estimated Effort**: ~50-100 LOC + documentation + operator runbook (D2=A pg_dump logical backup; lowest effort)

**Verification Gates**:

| Gate | Criterion | Evidence |
|---|---|---|
| P5c.V1 | Backup produces consistent snapshot | `pg_dump` with `--snapshot` or equivalent; integrity verified |
| P5c.V2 | Restore drill completes successfully | Operator drill log with restored DB verification |
| P5c.V3 | RPO/RTO operator-accepted for PostgreSQL | Signed operator acknowledgment |

#### 3.5.3 P5d — HA / Clustering Design (Implementation Gated)

**Goals**:
- [ ] HA topology reviewed (read replica, failover, partitioning)
- [ ] Multi-node deployment validated in staging (not production)
- [ ] StoreFacade concurrency model adapted for multi-node (if required)

**Blocked until**: D1=A and D3=A selected; P5d explicitly skipped/out of v1 scope unless operator revises D1/D3

**Estimated Effort**: ~0 LOC for D1=A/D3=A (skipped); ~200-300 LOC if D1/D3 revised later

**Verification Gates**:

| Gate | Criterion | Evidence |
|---|---|---|
| P5d.V1 | HA topology documented and operator-approved | Architecture diagram + operator signoff |
| P5d.V2 | Staging multi-node deployment passes integration tests | Test evidence from staging environment |
| P5d.V3 | Failover procedure tested in staging | Operator drill log |

#### 3.5.4 P5e — Migration Grade-Up (Implementation Gated)

**Goals**:
- [ ] SQLite → PostgreSQL migration upgraded from MVP to production-grade
- [ ] Idempotent/resumable migration with checkpointing
- [ ] Content-hash validation for lineage equivalence
- [ ] Large-dataset streaming and chunking

**Blocked until**: Eng.1 capacity confirmed; Eng.2 implementation plan approved; P5b–P5c design complete; P4.4 MVP migration baseline available

**Estimated Effort**: ~100-200 LOC + migration testing (upgrade from P4.4 MVP)

**Verification Gates**:

| Gate | Criterion | Evidence |
|---|---|---|
| P5e.V1 | Migration is idempotent (rerunnable without duplication) | Integration test with repeated runs |
| P5e.V2 | Content-hash validation passes for all migrated records | Hash comparison log |
| P5e.V3 | Large dataset (≥1M records) streams without OOM | Memory profile or benchmark evidence |

#### 3.5.5 P5 Non-Claims

- P5a approval does **NOT** authorize P5b–P5e implementation
- P5 completion (P5a–P5e) does **NOT** claim production-ready
- Full production-ready claim requires **P6 assessment** after P5e
- PostgreSQL production deployment remains **operator-owned** and gated
- HA/multi-node is **explicitly out of v1 scope** even if P5d is designed
- All P5b–P5e estimates are **rough planning figures**, not commitments

---

## 4. Rejected Approaches

### Fake/Stub PostgreSQL Support

**Rejected**: Adding a `PostgresStore` that panics or returns empty data "for future implementation"

**Reason**: Creates illusion of working PostgreSQL support that would need to be torn out later. Violates "no untested panicking production path" constraint.

### Full Implementation Now

**Rejected**: Implementing all repos and migrations in this phase

**Reason**: Oracle NO-GO verdict; estimated 2000-3000 LOC; requires container test infrastructure; distracts from v1 stability.

---

## 5. Configuration Impact

### Before (Misleading)

```toml
# store_dsn examples from config:
#   postgres://user:pass@localhost:5432/db - PostgreSQL
```

### After (Accurate)

```toml
# Store DSN — SQLite default; PostgreSQL local Docker/runtime supported
# Examples:
#   sqlite::memory: - in-memory database (default)
#   sqlite://ferrumgate.dev.db - file-based SQLite
#   postgres://user:pass@localhost:5432/db - PostgreSQL (local Docker/runtime only)
# MySQL is not implemented.
# See ADR-50 for the phased implementation plan.
store_dsn = "sqlite::memory:"
```

---

## 6. Summary

| Phase | Status | Notes |
|-------|--------|-------|
| P1 Guardrails | ✅ Complete | DSN validation + docs |
| P2 Infrastructure | ✅ Complete | Skeleton + container tests |
| P3 Repo impl | ✅ Complete | Local Docker/runtime; all 9 repos implemented |
| P4.1 DSN switching | ✅ Complete | Runtime `postgres://` DSN support |
| P4.2 Migration infra | ✅ Complete | Embedded schema migration runner |
| P4.3 Benchmark | ✅ Complete | 3853.2 writes/s local Docker release |
| P4.4 Data migration | ✅ Complete (MVP) | SQLite → PostgreSQL migration CLI; dry-run default, empty-target safety, count+ID validation |
| P5a Design/ADR | ☑ DONE | D1–D6 decisions, risk register, verification gates, non-claims — **G3.4 satisfied** |
| P5b Pool tuning | ☐ Deferred | Blocked on G3.6 pilot data (Eng.1/Eng.2 satisfied) |
| P5c Backup/restore | ☐ Deferred | Blocked on P5b design complete; D2=A pg_dump logical (lowest effort) |
| P5d HA/clustering | ☐ Skipped | D1=A/D3=A; explicitly out of v1 scope |
| P5e Migration grade-up | ☐ Deferred | Blocked on P5b–P5c design complete; P4.4 MVP baseline |

**Total estimated for full PostgreSQL production readiness**: ~3500-4500 LOC + significant testing infrastructure

> **P5 completion does not claim production-ready.** Even after P5a–P5e complete, a P6 assessment is required before any full production-ready claim.

---

## 7. References

- `crates/ferrum-store/src/repos.rs` — StoreFacade trait
- `crates/ferrum-store/src/sqlite/mod.rs` — SqliteStore implementation
- `crates/ferrum-gateway/src/state.rs` — ServerConfig validation
- `bins/ferrumd/src/main.rs` — daemon entry point
- `docs/implementation-path/45-current-feature-audit.md` — G7 gap record
- `docs/implementation-path/30-production-roadmap.md` — Phase 3 PostgreSQL
- `docs/implementation-path/23-production-readiness-assessment.md` — production readiness
- `docs/implementation-path/104-g3-4-p5a-adr-approval-packet.md` — G3.4 P5a approval packet (signed)
- `docs/implementation-path/105-g3-5-operator-d1-d3-signoff-packet.md` — G3.5 operator D1–D3 signoff packet (signed via chat authorization; Option A defaults)
- `docs/implementation-path/106-g3-6-pilot-metrics-evidence-packet.md` — G3.6 pilot metrics evidence packet (pending operator data)
- `docs/implementation-path/107-eng-1-capacity-confirmation-packet.md` — Eng.1 capacity confirmation packet (signed via chat authorization)
- `docs/implementation-path/108-eng-2-p5b-p5e-implementation-planning-packet.md` — Eng.2 P5b–P5e implementation planning packet (approved via chat authorization)
