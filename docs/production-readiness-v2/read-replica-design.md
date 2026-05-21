# HA-3 — Read Replica Behavior Design

> **Status**: PLANNING ARTIFACT — design doc drafted; no implementation; no replica deployed.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Parent**: [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md)
> **Scope**: [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md)
> **ADR**: [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md)

---

## 1. Purpose

This document designs how FerrumGate can use PostgreSQL read replicas for read scaling without implementing automated write failover. It is a **planning artifact** that explores routing strategies, consistency boundaries, observability, and failure modes. No read replica code or infrastructure exists today.

---

## 2. Current state

| Layer | State |
|-------|-------|
| Store backend | One DSN per ferrumd process (`FERRUMD_STORE_DSN`) |
| Connection pool | Single `sqlx::PgPool` initialized at startup |
| Read routing | None — all queries go to the single DSN |
| Replica awareness | None — ferrumd cannot distinguish primary from replica |
| SQLite replication | Not applicable — SQLite has no native replication path |

> **Scope boundary**: This design applies only to PostgreSQL deployments. Single-node SQLite remains the supported pilot runtime.

---

## 3. Design goals

| Goal | Priority | Rationale |
|------|----------|-----------|
| Offload read traffic from primary | High | Reduces load on the write path and improves latency for reporting/audit queries. |
| Never serve stale policy or approval state from a replica | Critical | Inconsistent policy/approval reads could cause incorrect governance decisions. |
| Minimal code change to ferrumd | High | The ADR ranks "ferrumd code changes" as a medium-weight driver; simplicity reduces risk. |
| Operator-visible replica lag | Medium | Operators must know when reads are stale and whether to trust replica results. |
| No automated failover required | High | Read replicas are Step 2 alongside manual failover; automation is Step 3. |

---

## 4. Candidate strategies

### Strategy A — External proxy (PgBouncer / HAProxy)

**Description**: Place a connection proxy between ferrumd and PostgreSQL. The proxy routes read-only connections to replicas and write connections to the primary. ferrumd continues to use a single DSN pointing at the proxy.

**Pros**:
- **Zero ferrumd code changes** for routing logic.
- Proxy can be upgraded or replaced independently.
- Existing operator tooling (PgBouncer, HAProxy) is mature and well-documented.

**Cons**:
- **Extra infrastructure** to operate (proxy process, config, health checks).
- Proxy becomes a new single point of failure unless itself made HA.
- Query-level routing may be complex if a connection interleaves reads and writes (e.g., within a transaction).
- Replica lag is invisible to ferrumd unless the proxy surfaces it via metadata or a side channel.

**Operational notes**:
- PgBouncer in `transaction` pooling mode can route per-transaction, but this requires `pool_mode` tuning and careful session feature avoidance.
- HAProxy can perform TCP-level health checks but cannot inspect SQL.

### Strategy B — Dual DSN in ferrumd

**Description**: Add an optional `FERRUMD_STORE_READ_DSN` config field. ferrumd maintains two pools: `write_pool` (primary) and `read_pool` (replica). Endpoints explicitly choose which pool to use.

**Pros**:
- **Explicit control** — engineering decides exactly which queries go to which pool.
- **Replica lag can be exposed directly** via metrics and readiness probes.
- No extra proxy infrastructure; operator only needs to configure the second DSN.

**Cons**:
- **Code change required** in `StoreFacade`, config parsing, and every endpoint.
- **Endpoint audit burden** — every handler must be classified as read-only or read-write.
- If the read DSN is misconfigured (points to primary by mistake), there is no safety benefit but also no harm.

### Comparison

| Dimension | Strategy A (Proxy) | Strategy B (Dual DSN) |
|-----------|-------------------|----------------------|
| ferrumd code change | None | Low–medium (pool split + endpoint audit) |
| New infrastructure | Proxy layer | None (just another DSN) |
| Operator complexity | Proxy ops | Config + understanding of read/write split |
| Replica lag visibility | Requires side channel | Direct metric in ferrumd |
| Transaction safety | Proxy-dependent | Explicit in code |
| Rollback ease | Revert proxy config | Revert config + restart |

**Preliminary direction**: The HA ADR (§4.3) notes that read replica support is **deferred to Step 2 design** and a follow-up ADR is required before implementation. This document does not select a winner; it provides the analysis for that follow-up ADR.

---

## 5. Read / write routing rules

If Strategy B (dual DSN) is chosen, the following routing rules apply.

### 5.1 Always use primary (write pool)

Any operation that:
- Creates, updates, or deletes an intent, approval, policy bundle, token, or audit log entry.
- Reads policy or approval state that could influence a mutating decision.
- Checks token revocation status during auth.
- Runs schema migrations.

**Rationale**: These paths require strongly consistent, up-to-date data. Stale reads here could cause security or correctness bugs.

### 5.2 May use replica (read pool)

Any operation that:
- Lists historical provenance events (read-only audit trail).
- Serves `GET` requests for public metadata (e.g., `/v1/tools/list`, health/readiness probes that do not need deep store checks).
- Generates read-only reports or metrics that tolerate seconds of lag.
- Executes `ferrumctl` read-only commands (e.g., `ferrumctl admin audit list` with no filter mutation).

**Rationale**: These paths are naturally read-only and do not affect governance decisions.

### 5.3 Gray area — operator discretion required

- **Approval queue listing (`GET /v1/approvals`)**: Listing is read-only, but an operator may immediately act on the result. If the replica lags, a recently submitted approval may be missing from the list. Recommendation: **use primary** for approval queue reads, or document that the list may be stale.
- **Execution lineage (`GET /v1/executions/{id}`)**: Historical reads are safe; however, if queried immediately after creation, a replica may not yet have the row. Recommendation: **use primary** for recently created executions, or accept eventual consistency.

---

## 6. Lag and consistency semantics

### 6.1 Replication model assumptions

This design assumes **asynchronous streaming replication** (the default for most operator-managed PostgreSQL). Synchronous replication is possible but imposes write latency; if the operator chooses sync replication, lag is near-zero and the stale-read risk is minimal.

### 6.2 Stale read boundaries

| Data category | Acceptable lag | Pool |
|---------------|----------------|------|
| Health / liveness probes | N/A (no DB hit) | N/A |
| Metrics / reporting | ≤ 60 s | Read pool |
| Provenance history | ≤ 60 s | Read pool |
| Policy state | 0 s (strong consistency) | Write pool |
| Approval state | 0 s (strong consistency) | Write pool |
| Token revocation | 0 s (strong consistency) | Write pool |

### 6.3 Critical invariant

> **Policy/approval state must never be read from a lagging replica if it could cause inconsistent decisions.**

This is explicit in the HA ADR (§5.3). The routing rules above enforce it by sending policy/approval reads to the write pool.

---

## 7. Observability and readiness

### 7.1 Replica lag metric

If Strategy B is implemented, expose:

```
ferrumgate_store_replica_lag_seconds{replica="<host:port>"}  # float, seconds
```

**Collection method**: A background task runs `SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp()))` on the read pool periodically (e.g., every 15 s).

**Edge cases**:
- If `pg_last_xact_replay_timestamp()` is `NULL` (no WAL replayed yet), emit `0` or `NaN` and log a warning.
- If the replica is unreachable, the metric is absent and the readiness probe degrades.

### 7.2 Readiness probe extension

Extend `/v1/readyz/deep` with a **readiness gate** for replica lag:

| Condition | Behavior |
|-----------|----------|
| No read DSN configured | Skip replica check; return primary health only. |
| Read DSN configured and lag ≤ threshold | Include `"replica":"ready"` in response. |
| Read DSN configured and lag > threshold | Degrade readiness (return 503) **only if** the operator configured `enforce_replica_readiness = true`. Default: `false` (allow serving stale reads with warning). |
| Read DSN configured but replica unreachable | Degrade readiness if `enforce_replica_readiness = true`; otherwise warn. |

**Rationale**: Read replicas are for scaling, not for availability. A lagging replica should not take down the gateway unless the operator explicitly opts into strict consistency.

### 7.3 Alert rule template

```yaml
- alert: FerrumGateReplicaLagHigh
  expr: ferrumgate_store_replica_lag_seconds > 30
  for: 2m
  labels:
    severity: warning
  annotations:
    summary: "Read replica lag exceeds 30s"
```

> This is a template only; live Prometheus validation is operator-environment-dependent.

---

## 8. Failure modes

### 8.1 Replica lag exceeds threshold

- **Symptom**: `ferrumgate_store_replica_lag_seconds` spikes.
- **Impact**: Read pool serves stale data.
- **Mitigation**: Route gray-area reads back to primary; alert operator; investigate replication bottleneck (network, disk I/O, WAL retention).

### 8.2 Replica becomes unavailable

- **Symptom**: Read pool connections fail; metric disappears.
- **Impact**: Read-only endpoints may 503 if they exclusively use the read pool.
- **Mitigation**: Fallback to primary for all reads. In Strategy B, this requires code-level fallback (try read pool, on failure use write pool) or operator quickly unsetting `FERRUMD_STORE_READ_DSN` and restarting.

### 8.3 Primary fails while replica is lagging

- **Symptom**: Primary unreachable; replica has not replayed latest WAL.
- **Impact**: Data loss bounded by lag (RPO = lag duration).
- **Mitigation**: Follow the manual failover runbook. Promote the lagging standby, accepting the data loss window. Do not attempt to recover the old primary if it risks split-brain.

### 8.4 Misconfiguration — read DSN points to primary

- **Symptom**: No performance benefit; all traffic still hits primary.
- **Impact**: No harm, but no scaling benefit.
- **Mitigation**: Operator validates DSNs during setup; `ferrumctl admin status` could report pool targets.

---

## 9. RPO / RTO relationship

Read replicas affect RPO but not RTO (unless they are promoted during failover).

| Scenario | RPO impact | RTO impact |
|----------|------------|------------|
| Async read replica, primary fails | RPO = replication lag (seconds to minutes of WAL not yet replayed). | None — failover is still manual per HA-2 runbook. |
| Sync read replica, primary fails | RPO ≈ 0 (all commits confirmed on standby). | None — manual failover still required. |
| Replica used for reads only | No write-path RPO change. | None — replica is not a failover target unless promoted. |

> **Key point**: Read replicas are for scaling, not for reducing RTO. RTO improvement requires standby promotion (HA-2) or automated failover (HA-4, deferred).

---

## 10. Schema migration handling

Per HA ADR §5.1:
- Migrations run on the **primary** only.
- After a schema change, replicas must be recreated or allowed to re-sync.
- During the re-sync window, replica lag may spike or reads may fail if the replica schema is behind.
- **Recommendation**: Pause read routing to the replica during schema migrations, or accept degraded readiness until lag stabilizes.

---

## 11. Operator validation plan

Before any implementation, the operator should validate the following in a non-production environment:

1. **Replication baseline**: Confirm streaming replication is healthy (`pg_stat_replication` shows `state = streaming`).
2. **Lag measurement**: Run `SELECT pg_last_xact_replay_timestamp();` on the replica while generating write load on the primary. Verify lag stays within the target bound (e.g., < 5 s under normal load).
3. **Failover smoke**: Promote the replica manually, update ferrumd DSN, and confirm reads and writes resume. Measure RPO from the promotion moment.
4. **Misconfiguration test**: Point `FERRUMD_STORE_READ_DSN` at the primary and confirm ferrumd starts without error (graceful degradation to no-read-replica mode).
5. **Replica disconnect test**: Stop the replica process while ferrumd is running. Confirm read-only endpoints either fallback to primary or fail gracefully (no panics, no data corruption).

> These validation steps are **not yet executed**. They are part of the future operator runbook for when read replica support is implemented.

---

## 12. Non-claims

- **NOT implemented**: No read replica code exists. ferrumd uses a single DSN and a single pool.
- **NOT deployed**: No PostgreSQL read replica is running for FerrumGate.
- **NOT production-ready**: This design doc does not make FerrumGate production-ready.
- **NOT a committed architecture**: The choice between Strategy A (proxy) and Strategy B (dual DSN) is not finalized. A follow-up ADR is required before implementation.
- **NOT automated failover**: Read replicas do not provide automated failover. Standby promotion remains manual per HA-2.
- **NOT closing BLK-A-DOM**: Real owned domain remains an external operator blocker.
- **NOT SQLite support**: SQLite has no native replication path; read replicas apply only to PostgreSQL.

---

## Related docs

- [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md) — HA architecture decisions, phased strategy, and Step 2 impact analysis.
- [`docs/production-readiness-v2/manual-failover-runbook.md`](./manual-failover-runbook.md) — Manual failover procedure if a replica must be promoted.
- [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md) — HA roadmap and task tracking.
- [`docs/guides/operator.md`](../../guides/operator.md) — General operator procedures and monitoring guidance.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PostgreSQL hardening and schema migration handling.

---

*End of HA-3 Read Replica Behavior Design — planning artifact only (2026-05-21).*
