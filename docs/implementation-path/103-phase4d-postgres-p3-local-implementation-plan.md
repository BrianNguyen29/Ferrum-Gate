# 103 — PostgreSQL P3 Local Implementation Plan

> **Status**: P3 — Local Docker Implementation Planning Only
> **Date**: 2026-05-10
> **Scope**: Documentation artifact. No Rust implementation, no database creation, no GCP, no commit.
> **Constraint**: Do NOT claim PostgreSQL runtime YES. Do NOT claim production-ready/HA/full posture. No secrets.
> **Target**: Local Docker PostgreSQL first; production PostgreSQL deferred until after P3 complete.

---

## 1. Context and Evidence

### Current State (P2 Skeleton)

- `PostgresStore` skeleton in `crates/ferrum-store/src/postgres/mod.rs` exists (P2 done)
- All 9 skeleton repos return `StoreError::Other("PostgreSQL P2 skeleton only; runtime support not implemented")`
- `docker-compose.postgres.yml` provides a local PostgreSQL 16 container for development/testing
- `sqlx::postgres` feature is non-default and compile-time only — does not enable runtime functionality
- ADR-50 Phase P3 lists 9 repos to implement with ~1500-2000 LOC estimated
- `StoreFacade` trait is DB-agnostic; `SqliteStore` is the working implementation

### Evidence References

- `crates/ferrum-store/src/postgres/mod.rs` — PostgresStore skeleton
- `crates/ferrum-store/src/postgres/{intents,proposals,capabilities,executions,rollback,approvals,provenance,ledger,policy_bundles}.rs` — 9 skeleton repos
- `docker-compose.postgres.yml` — local PostgreSQL 16 container definition
- `crates/ferrum-store/src/sqlite/mod.rs` — working SQLite store (reference implementation)
- `crates/ferrum-store/src/sqlite/{intents,proposals,capabilities,executions,rollback,approvals,provenance,ledger,policy_bundles}.rs` — SQLite repo implementations (reference patterns)
- `crates/ferrum-store/src/repos.rs` — StoreFacade trait definition
- `ADR-50 (docs/implementation-path/50-p4-postgres-store-facade-adr.md)` — phased plan with P3 deferred

### Gap

P3 must deliver a working `PostgresStore` where all 9 repos have functional implementations backed by a local Docker PostgreSQL instance. This plan provides the actionable implementation checklist, schema strategy, test gates, and invariant gates.

**Post-P3 claim boundaries**: P3 does NOT deliver production PostgreSQL, HA, multi-node, or full production posture. These are P4/P5 scope.

---

## 2. Scope

### IN

- Add Phase 4D/P3 PostgreSQL local implementation plan
- Include repo-by-repo checklist
- Schema/migration strategy
- Test gates
- Invariant gates
- Local Docker target
- Claim boundaries
- Update README index

### OUT

- No Rust implementation (this is a planning artifact only)
- No database creation
- No GCP
- No commit
- No production PostgreSQL deployment
- No HA/multi-node

---

## 3. Repo-by-Repo Implementation Checklist

Each repo must be implemented in order. Skeleton error replaced with real `sqlx::query` + `sqlx::Row` deserialization.

### 3.1 `PostgresIntentRepo` — `crates/ferrum-store/src/postgres/intents.rs`

**Reference**: `crates/ferrum-store/src/sqlite/intents.rs`

**Required methods**:
- [ ] `insert(intent: &IntentEnvelope) -> Result<()>`
- [ ] `get(intent_id: IntentId) -> Result<Option<IntentEnvelope>>`
- [ ] `update(intent: &IntentEnvelope) -> Result<()>`
- [ ] `update_status(intent_id: IntentId, status: IntentStatus) -> Result<()>`
- [ ] `list_by_status(status: IntentStatus) -> Result<Vec<IntentEnvelope>>`
- [ ] `list_intents(intent_id, statuses, cursor, limit) -> Result<(Vec<IntentEnvelope>, Option<String>)>`
- [ ] `list_intents_with_exec_state(intent_id, statuses, cursor, limit) -> Result<(Vec<(IntentEnvelope, Option<String>)>, Option<String>)>`

**SQLite patterns to translate**:
- `$var` → `$1, $2, ...` (PostgreSQL positional params)
- `?1, ?2` → `$1, $2`
- `enum_text()` helper usage (same helper, same enum serialization)
- `to_json()` helper usage (same helper)
- `fetch_entity_by_id()`, `fetch_entities()` → PostgreSQL equivalents with `sqlx::query` and `sqlx::query_as`
- Note: No write queue in P3 — direct writes only

**Key differences from SQLite**:
- Uses `sqlx::query` + `sqlx::query_as` with `PgPool` instead of `SqlitePool`
- No `WriteQueue` (deferred to future write-queue architecture for PostgreSQL)
- JSON serialization via `serde_json` (same as SQLite via `to_json` helper)

---

### 3.2 `PostgresProposalRepo` — `crates/ferrum-store/src/postgres/proposals.rs`

**Reference**: `crates/ferrum-store/src/sqlite/proposals.rs`

**Required methods**:
- [ ] `insert(proposal: &ActionProposal) -> Result<()>`
- [ ] `get(proposal_id: ProposalId) -> Result<Option<ActionProposal>>`
- [ ] `list_by_intent(intent_id: IntentId) -> Result<Vec<ActionProposal>>`

---

### 3.3 `PostgresCapabilityRepo` — `crates/ferrum-store/src/postgres/capabilities.rs`

**Reference**: `crates/ferrum-store/src/sqlite/capabilities.rs`

**Required methods**:
- [ ] `insert(capability: &CapabilityLease) -> Result<()>`
- [ ] `get(capability_id: CapabilityId) -> Result<Option<CapabilityLease>>`
- [ ] `update(capability: &CapabilityLease) -> Result<()>`
- [ ] `list_by_intent(intent_id: IntentId) -> Result<Vec<CapabilityLease>>`

---

### 3.4 `PostgresExecutionRepo` — `crates/ferrum-store/src/postgres/executions.rs`

**Reference**: `crates/ferrum-store/src/sqlite/executions.rs`

**Required methods**:
- [ ] `insert(execution: &ExecutionRecord) -> Result<()>`
- [ ] `get(execution_id: ExecutionId) -> Result<Option<ExecutionRecord>>`
- [ ] `update(execution: &ExecutionRecord) -> Result<()>`
- [ ] `update_state(execution_id: ExecutionId, state: ExecutionState) -> Result<()>`
- [ ] `list_by_intent(intent_id: IntentId) -> Result<Vec<ExecutionRecord>>`

---

### 3.5 `PostgresRollbackRepo` — `crates/ferrum-store/src/postgres/rollback.rs`

**Reference**: `crates/ferrum-store/src/sqlite/rollback.rs`

**Required methods**:
- [ ] `insert(contract: &RollbackContract) -> Result<()>`
- [ ] `get(contract_id: RollbackContractId) -> Result<Option<RollbackContract>>`
- [ ] `update(contract: &RollbackContract) -> Result<()>`
- [ ] `list_by_execution(execution_id: ExecutionId) -> Result<Vec<RollbackContract>>`

---

### 3.6 `PostgresApprovalRepo` — `crates/ferrum-store/src/postgres/approvals.rs`

**Reference**: `crates/ferrum-store/src/sqlite/approvals.rs`

**Required methods**:
- [ ] `insert(approval: &ApprovalRequest) -> Result<()>`
- [ ] `get(approval_id: ApprovalId) -> Result<Option<ApprovalRequest>>`
- [ ] `update(approval: &ApprovalRequest) -> Result<()>`
- [ ] `resolve(approval_id: ApprovalId, state: ApprovalState) -> Result<()>`
- [ ] `list_pending() -> Result<Vec<ApprovalRequest>>`

---

### 3.7 `PostgresProvenanceRepo` — `crates/ferrum-store/src/postgres/provenance.rs`

**Reference**: `crates/ferrum-store/src/sqlite/provenance.rs`

**Required methods**:
- [ ] `append_event(event: &ProvenanceEvent) -> Result<()>`
- [ ] `get_event(event_id: EventId) -> Result<Option<ProvenanceEvent>>`
- [ ] `append_edges(to_event_id: EventId, edges: &[ProvenanceEdge]) -> Result<()>`
- [ ] `query(request: &ProvenanceQueryRequest) -> Result<Vec<ProvenanceEvent>>`
- [ ] `get_edges_to(to_event_id: EventId) -> Result<Vec<ProvenanceEdge>>`
- [ ] `get_edges_from(from_event_ids: &[EventId]) -> Result<Vec<ProvenanceEdge>>`

---

### 3.8 `PostgresLedgerRepo` — `crates/ferrum-store/src/postgres/ledger.rs`

**Reference**: `crates/ferrum-store/src/sqlite/ledger.rs`

**Required methods**:
- [ ] `append(entry: &LedgerEntry) -> Result<()>`
- [ ] `get_by_event(event_id: EventId) -> Result<Option<LedgerEntry>>`
- [ ] `list_recent(limit: u32) -> Result<Vec<LedgerEntry>>`
- [ ] `get_latest() -> Result<Option<LedgerEntry>>`
- [ ] `verify_chain() -> Result<()>`

---

### 3.9 `PostgresPolicyBundleRepo` — `crates/ferrum-store/src/postgres/policy_bundles.rs`

**Reference**: `crates/ferrum-store/src/sqlite/policy_bundles.rs`

**Required methods**:
- [ ] `insert(bundle: &PolicyBundle) -> Result<()>`
- [ ] `get(bundle_id: &str) -> Result<Option<PolicyBundle>>`
- [ ] `get_by_content_hash(content_hash: &str) -> Result<Option<PolicyBundle>>`
- [ ] `update(bundle: &PolicyBundle) -> Result<()>`
- [ ] `delete(bundle_id: &str) -> Result<()>`
- [ ] `list() -> Result<Vec<PolicyBundle>>`
- [ ] `list_active() -> Result<Vec<PolicyBundle>>`
- [ ] `set_active(bundle_id: &str, active: bool) -> Result<()>`

---

## 4. Schema and Migration Strategy

### 4.1 Schema Definition

PostgreSQL schema should mirror SQLite schema with PostgreSQL-specific types.

**Tables** (mirror SQLite `migrations.rs` schema):

```sql
-- intents table
CREATE TABLE IF NOT EXISTS intents (
    intent_id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    normalized_goal TEXT NOT NULL,
    status TEXT NOT NULL,
    risk_tier TEXT NOT NULL,
    approval_mode TEXT NOT NULL,
    default_rollback_class TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- proposals table
CREATE TABLE IF NOT EXISTS proposals (
    proposal_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    action TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- capabilities table
CREATE TABLE IF NOT EXISTS capabilities (
    capability_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    lease TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- executions table
CREATE TABLE IF NOT EXISTS executions (
    execution_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- rollback_contracts table
CREATE TABLE IF NOT EXISTS rollback_contracts (
    contract_id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    rollback_class TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- approvals table
CREATE TABLE IF NOT EXISTS approvals (
    approval_id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- provenance_events table
CREATE TABLE IF NOT EXISTS provenance_events (
    event_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- provenance_edges table
CREATE TABLE IF NOT EXISTS provenance_edges (
    from_event_id TEXT NOT NULL,
    to_event_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    PRIMARY KEY (from_event_id, to_event_id, edge_type)
);

-- ledger table
CREATE TABLE IF NOT EXISTS ledger (
    event_id TEXT PRIMARY KEY,
    prev_event_id TEXT,
    created_at TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

-- policy_bundles table
CREATE TABLE IF NOT EXISTS policy_bundles (
    bundle_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);
```

### 4.2 Migration Strategy

**Phase 1 — Schema Creation**:
- Create `crates/ferrum-store/src/postgres/migrations.rs` with inline schema SQL
- Add `migrate()` method to `PostgresStore::connect()`
- Use `sqlx::query` to execute `CREATE TABLE IF NOT EXISTS` statements

**Phase 2 — Data Migration**:
- Deferred to P4 (ADR-50)
- SQLite → PostgreSQL migration is out of P3 scope

### 4.3 Connection Pool

- Use `sqlx::PgPoolOptions` with `max_connections = 5` for local Docker
- Connection URL: `postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test`

---

## 5. Test Gates

### 5.1 Unit Test Gate (per repo)

Each repo implementation must pass:

```bash
# With postgres feature enabled
cargo test --package ferrum-store --features postgres -- postgres::[repo_name]
```

**Pass criteria**: All existing skeleton tests pass + new impl tests pass.

### 5.2 Integration Test Gate

**Docker Compose start**:
```bash
docker compose -f docker-compose.postgres.yml up -d postgres_p2
# Wait for health check: pg_isready -U ferrumgate_dev -d ferrumgate_p2_test
```

**Test execution**:
```bash
# Run all postgres feature tests
cargo test --package ferrum-store --features postgres
```

**Pass criteria**: All tests pass with live PostgreSQL.

### 5.3 Schema Validation Gate

```bash
# Verify all tables exist
psql -U ferrumgate_dev -d ferrumgate_p2_test -c '\dt'
```

**Expected**: 10 tables (intents, proposals, capabilities, executions, rollback_contracts, approvals, provenance_events, provenance_edges, ledger, policy_bundles).

### 5.4 Health Check Gate

`PostgresStore::health_check()` must return `Ok(())` when PostgreSQL is reachable.

---

## 6. Invariant Gates

These invariants must hold for P3 PostgreSQL local implementation:

### 6.1 Functional Invariants

- [ ] `PostgresStore` implements `StoreFacade` trait correctly
- [ ] All 9 repos return correct types matching trait signatures
- [ ] `health_check()` returns `Ok(())` when connected
- [ ] `write_queue_depth()` returns 0 (no write queue in P3)
- [ ] `Pool` uses `PgPool` not `SqlitePool`

### 6.2 Behavioral Invariants

- [ ] Each `IntentRepo` method behavior matches `SqliteIntentRepo` semantics
- [ ] Each `ProposalRepo` method behavior matches `SqliteProposalRepo` semantics
- [ ] Each `CapabilityRepo` method behavior matches `SqliteCapabilityRepo` semantics
- [ ] Each `ExecutionRepo` method behavior matches `SqliteExecutionRepo` semantics
- [ ] Each `RollbackRepo` method behavior matches `SqliteRollbackRepo` semantics
- [ ] Each `ApprovalRepo` method behavior matches `SqliteApprovalRepo` semantics
- [ ] Each `ProvenanceRepo` method behavior matches `SqliteProvenanceRepo` semantics
- [ ] Each `LedgerRepo` method behavior matches `SqliteLedgerRepo` semantics
- [ ] Each `PolicyBundleRepo` method behavior matches `SqlitePolicyBundleRepo` semantics

### 6.3 Non-Claims (Must Remain False)

- [ ] PostgreSQL is NOT production-ready (P3 = local Docker only)
- [ ] HA is NOT implemented (P5 scope)
- [ ] Multi-node is NOT implemented (P5 scope)
- [ ] Write queue for PostgreSQL is NOT implemented (deferred)
- [ ] No claim of parity with SQLite feature set beyond repo implementations

---

## 7. Local Docker Target

### 7.1 Docker Compose Service

Use existing `docker-compose.postgres.yml`:

```yaml
services:
  postgres_p2:
    image: postgres:16
    container_name: ferrumgate_postgres_p2
    environment:
      POSTGRES_USER: ferrumgate_dev
      POSTGRES_PASSWORD: ferrumgate_dev_password  # placeholder - not for production
      POSTGRES_DB: ferrumgate_p2_test
    ports:
      - "5432:5432"
    volumes:
      - postgres_p2_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ferrumgate_dev -d ferrumgate_p2_test"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: "no"
    deploy:
      resources:
        limits:
          memory: 512M
```

### 7.2 Startup Commands

```bash
# Start container
docker compose -f docker-compose.postgres.yml up -d postgres_p2

# Verify health
docker compose -f docker-compose.postgres.yml ps

# Stop container
docker compose -f docker-compose.postgres.yml down -v  # -v removes volumes
```

### 7.3 Connection String

```
postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test
```

**Note**: Credentials are development-only placeholders from `docker-compose.postgres.yml`. No real secrets.

---

## 8. Implementation Order

### Recommended Order (dependency-aware)

1. **`PostgresIntentRepo`** — most frequently used, good reference for patterns
2. **`PostgresProposalRepo`** — depends on IntentRepo via foreign key
3. **`PostgresExecutionRepo`** — depends on IntentRepo via foreign key
4. **`PostgresCapabilityRepo`** — depends on IntentRepo via foreign key
5. **`PostgresRollbackRepo`** — depends on ExecutionRepo via foreign key
6. **`PostgresApprovalRepo`** — depends on ProposalRepo via foreign key
7. **`PostgresProvenanceRepo`** — independent graph structure
8. **`PostgresLedgerRepo`** — independent append-only log
9. **`PostgresPolicyBundleRepo`** — independent key-value store

### Parallelization

- Steps 1-4 can proceed in parallel (all depend only on schema)
- Steps 5-9 can proceed in parallel after steps 1-4 complete

---

## 9. Claim Boundaries

### P3 Delivers (YES)

- Local Docker PostgreSQL `PostgresStore` with all 9 repos functional
- Schema creation via inline migrations
- Health check returning `Ok(())` when PostgreSQL is reachable
- Local development and testing capability

### P3 Does NOT Deliver (NO)

- **PostgreSQL runtime YES for production** — still NO
- **Production-ready** — still NO
- **HA/multi-node** — P5 scope
- **Write queue for PostgreSQL** — deferred
- **SQLite → PostgreSQL data migration** — P4 scope
- **GCP or cloud deployment** — out of scope
- **Parity with all SQLite features** (e.g., write queue, WAL tuning) — deferred

### Deferred to Future Phases

| Phase | Item |
|-------|------|
| P4 | Schema migration (SQLite → PostgreSQL) |
| P4 | Embedded migration runner |
| P4 | Integration tests with live postgres |
| P4 | Benchmark validation (1000+ writes/s target) |
| P5 | HA/clustering architecture |
| P5 | Connection pool tuning for production |
| P5 | Backup/restore for PostgreSQL |
| P5 | Multi-node deployment validation |

---

## 10. Risk Factors

| Risk | Mitigation |
|------|------------|
| `sqlx::postgres` async row deserialization differs from SQLite | Use `sqlx::query_as::<_, Type>` with explicit type annotations |
| PostgreSQL `BIGSERIAL` vs SQLite `INTEGER` auto-increment | Use `gen_random_uuid()` or `gen_uuid()` for IDs instead of auto-increment |
| JSONB vs JSON storage | Use `serde_json::Value` with `sqlx::types::Json` for PostgreSQL |
| Enum serialization | Same `enum_text()` helper works for both |
| Connection pool saturation | P3 uses fixed `max_connections = 5` for local dev only |

---

## 11. Summary

| Item | Status |
|------|--------|
| P3 plan created | ✅ This artifact |
| 9 repos to implement | ✅ Listed in §3 |
| Schema strategy | ✅ Inline SQL in `migrations.rs` |
| Migration strategy | ✅ Schema creation first; data migration deferred to P4 |
| Test gates | ✅ Unit, integration, schema, health check in §5 |
| Invariant gates | ✅ Functional, behavioral, non-claims in §6 |
| Local Docker target | ✅ Existing `docker-compose.postgres.yml` |
| Claim boundaries | ✅ Explicit in §9 |
| README index update | ☐ Pending (must be done when this plan is committed) |

**Total estimated LOC for P3**: ~1500-2000 LOC (9 repos × ~150-200 LOC each + connection pooling + migrations)

---

## 12. References

- [ADR-50 — PostgreSQL StoreFacade Phased Implementation Plan](./50-p4-postgres-store-facade-adr.md)
- [Production Readiness Roadmap](./67-production-readiness-roadmap.md) — P3.1 PostgreSQL deferred
- [docker-compose.postgres.yml](../../docker-compose.postgres.yml) — local PostgreSQL container
- `crates/ferrum-store/src/postgres/mod.rs` — PostgresStore skeleton
- `crates/ferrum-store/src/sqlite/mod.rs` — SqliteStore reference implementation
- `crates/ferrum-store/src/repos.rs` — StoreFacade trait definition
