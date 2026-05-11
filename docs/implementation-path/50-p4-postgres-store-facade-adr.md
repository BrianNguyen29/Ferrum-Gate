# ADR-50 — PostgreSQL StoreFacade: Phased Implementation Plan

> **Status**: P3 Repository Implementations — Complete (local Docker). P4.1 Runtime DSN switching — Complete. P4.2 Migration infrastructure — Complete. P4.3 Benchmark validation — Complete (3853.2 writes/s local Docker release). P5 Production Readiness — Deferred.
> **Date**: 2026-05-11
> **Deciders**: Engineering implementation complete for local Docker/runtime; production/HA/multi-node posture remains NO.
> **Estimated Effort**: ~2000-3000 LOC + migrations + container tests

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
- [ ] Design SQLite → PostgreSQL data migration path
- [x] Implement embedded migration runner for postgres (schema creation)
- [x] Add integration tests with live postgres
- [x] Benchmark validation (target: 1000+ writes/s) — **achieved 3853.2 writes/s local Docker release**

**P4 Sub-phase Status**:
- P4.1 Runtime DSN switching — ✅ Complete
- P4.2 Migration infrastructure (schema runner) — ✅ Complete
- P4.3 Benchmark validation — ✅ Complete (3853.2 writes/s local Docker release)
- P4.4 Data migration (SQLite → PostgreSQL) — ☐ Deferred

**Estimated Effort**: ~300-500 LOC migrations + tests — **P4.1–P4.3 complete; P4.4 deferred**

---

### Phase P5 — Production Readiness (Post-P4)

**Goals**:
- [ ] HA/clustering architecture design
- [ ] Connection pool tuning for production
- [ ] Backup/restore for PostgreSQL
- [ ] Multi-node deployment validation

**Estimated Effort**: ~500+ LOC + significant testing

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
| P4.4 Data migration | ☐ Deferred | SQLite → PostgreSQL data migration |
| P5 Production | ☐ Deferred | HA/clustering, pool tuning, backup/restore, multi-node |

**Total estimated for full PostgreSQL**: ~3000-4000 LOC + significant testing infrastructure

---

## 7. References

- `crates/ferrum-store/src/repos.rs` — StoreFacade trait
- `crates/ferrum-store/src/sqlite/mod.rs` — SqliteStore implementation
- `crates/ferrum-gateway/src/state.rs` — ServerConfig validation
- `bins/ferrumd/src/main.rs` — daemon entry point
- `docs/implementation-path/45-current-feature-audit.md` — G7 gap record
- `docs/implementation-path/30-production-roadmap.md` — Phase 3 PostgreSQL
- `docs/implementation-path/23-production-readiness-assessment.md` — production readiness
