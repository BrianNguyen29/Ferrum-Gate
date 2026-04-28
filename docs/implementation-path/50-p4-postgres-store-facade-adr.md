# ADR-50 — PostgreSQL StoreFacade: Phased Implementation Plan

> **Status**: Deferred — Not implemented; Phase 1 SQLite only
> **Date**: 2026-04-27
> **Deciders**: Oracle NO-GO verdict for full implementation
> **Estimated Effort**: ~2000-3000 LOC + migrations + container tests

---

## 1. Context and Evidence

### Current State

- `StoreFacade` trait in `crates/ferrum-store/src/repos.rs` is **DB-agnostic**
- `SqliteStore` fully implements `StoreFacade` in `crates/ferrum-store/src/sqlite/mod.rs`
- `ferrumd` currently connects via `SqliteStore::connect_with_tuning()` only
- No `PostgresStore` or `MySqlStore` implementation exists
- `sqlx` is configured for SQLite only (no `sqlx::postgres` feature enabled)
- Config files show `postgres://` and `mysql://` as examples but these are **not implemented**

### Evidence References

- `crates/ferrum-store/src/repos.rs:175-189` — `StoreFacade` trait
- `crates/ferrum-store/src/sqlite/mod.rs:312-375` — `SqliteStore` impl
- `bins/ferrumd/src/main.rs:236-240` — store connection
- `configs/ferrumgate.dev.toml:14` — postgres example comment
- `configs/ferrumgate.prod.toml:15` — postgres example comment
- `docs/implementation-path/45-current-feature-audit.md:324` — G7: PostgreSQL deferred

### Gap

The codebase references PostgreSQL in documentation and config comments but provides **no implementation path**. This creates a misleading impression that PostgreSQL support exists or is readily available. Oracle has issued a NO-GO verdict for full implementation at this time.

---

## 2. Decision

### Immediate Action: Explicit Rejection

Add DSN guardrails in `ServerConfig::validate()` that explicitly reject non-SQLite DSNs with clear error messages:

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

### Rationale

1. **No false claims**: SQLite-only status is unambiguous
2. **Clear error path**: Users get actionable guidance instead of cryptic connection failures
3. **Low risk**: Validation happens at startup before any resource acquisition
4. **No overclaim**: Does not add stub implementation that appears working

---

## 3. Phased Implementation Plan

> **Note**: Full implementation is deferred. This plan is for design-ready artifact purposes only.

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
- [ ] Enable `sqlx::postgres` feature flag
- [ ] Create `PostgresStore` skeleton with placeholder repo implementations
- [ ] Define migration strategy (SQLite → PostgreSQL compatibility layer?)
- [ ] Add container test infrastructure (Docker Compose for postgres)

**Estimated Effort**: ~500 LOC infrastructure, ~200 LOC for skeleton stores

**Deliverables**:
- `crates/ferrum-store/src/postgres/mod.rs` — module skeleton
- `crates/ferrum-store/src/postgres/intents.rs` — placeholder
- `docker-compose.yml` for local postgres testing

---

### Phase P3 — Repository Implementations (Post-P2)

**Goals**:
- [ ] Implement `PostgresIntentRepo`
- [ ] Implement `PostgresProposalRepo`
- [ ] Implement `PostgresCapabilityRepo`
- [ ] Implement `PostgresExecutionRepo`
- [ ] Implement `PostgresRollbackRepo`
- [ ] Implement `PostgresApprovalRepo`
- [ ] Implement `PostgresProvenanceRepo`
- [ ] Implement `PostgresLedgerRepo`
- [ ] Implement `PostgresPolicyBundleRepo`

**Estimated Effort**: ~1500-2000 LOC (9 repos × ~150-200 LOC each + connection pooling)

**Key Considerations**:
- Write queue architecture must be adapted for PostgreSQL (different concurrency model)
- Connection pooling via `sqlx::Pool<Postgres>`
- Batch INSERT patterns for write queue

---

### Phase P4 — Migrations and Testing (Post-P3)

**Goals**:
- [ ] Design SQLite → PostgreSQL migration path
- [ ] Implement embedded migration runner for postgres
- [ ] Add integration tests with live postgres
- [ ] Benchmark validation (target: 1000+ writes/s)

**Estimated Effort**: ~300-500 LOC migrations + tests

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
# Store DSN — SQLite only for v1
# Examples:
#   sqlite::memory: - in-memory database (default)
#   sqlite://ferrumgate.dev.db - file-based SQLite
# PostgreSQL and MySQL are not implemented.
# See ADR-50 for the phased implementation plan.
store_dsn = "sqlite::memory:"
```

---

## 6. Summary

| Phase | Status | Notes |
|-------|--------|-------|
| P1 Guardrails | ✅ **This artifact** | DSN validation + docs |
| P2 Infrastructure | Deferred | Skeleton + container tests |
| P3 Repo impl | Deferred | Full postgres store |
| P4 Migrations | Deferred | Schema + data migration |
| P5 Production | Deferred | HA/clustering |

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
