# 15 — Revocation Durability Tradeoff Note

> **Status**: Planning artifact. Supports operator Q4 decision; does not choose for the operator. No code changes.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **Depends on**: [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md)

---

## Goal

Provide a concise, decision-ready comparison of token revocation durability strategies so the operator can answer Q4 in the [`11-blockers-and-unblock-plan.md`](11-blockers-and-unblock-plan.md) decision packet with confidence.

## Question

> **Q4**: Should token revocation be immediate (in-memory deny list) or durable (store-backed revocation table)?

## Option A — Immediate (in-memory deny list)

### How it works

- Revocation sets a flag in an in-memory `HashSet` or concurrent skip list.
- Token lookup checks the deny list before checking the store.
- On process restart, the deny list is empty; all previously revoked tokens must be reloaded from a persistent log or re-revoked manually.

### Pros

| Benefit | Detail |
|---------|--------|
| Zero latency | Revocation is `O(1)` in memory; no store round-trip |
| No schema change | No new table or column needed |
| Simple implementation | A single `Arc<DashSet<TokenId>>` or similar |
| Works without DB | Compatible with in-memory or file-backed deployments |

### Cons

| Risk | Detail |
|------|--------|
| **Durability gap** | Revocations are lost on restart unless a WAL or snapshot is maintained |
| **Replication gap** | In multi-node deployments, each node has its own deny list; revocations do not propagate without a distributed cache |
| **Audit gap** | No durable record of *when* or *why* a token was revoked |
| **Recovery complexity** | Requires a persistent revocation log or periodic snapshot to survive restart |

### Best for

- Single-node deployments where restart is rare and operator can tolerate re-revoking tokens after restart.
- Rapid prototyping and short-lived tokens (TTL < 24h).

---

## Option B — Durable (store-backed revocation table / column)

### How it works

- The token store table has a `revoked_at` timestamp column (already in the proposed schema).
- Revocation updates `revoked_at = now()` and optionally `revoked_reason`.
- Every token lookup queries the store (or a cached view) and checks `revoked_at IS NULL`.

### Pros

| Benefit | Detail |
|---------|--------|
| **Survives restart** | Revocations are durable by definition |
| **Audit trail** | `revoked_at` + `revoked_reason` provide a complete record |
| **Multi-node safe** | All nodes share the same store; revocation is immediately visible |
| **Rollback support** | Accidental revocation can be documented and reversed via a new token issuance (rotation) |
| **No extra log** | Uses the existing store; no separate WAL needed |

### Cons

| Cost | Detail |
|------|--------|
| **Store latency** | Every authenticated request incurs a store lookup (mitigated by connection pooling and caching) |
| **Schema change** | Requires `revoked_at` and `revoked_reason` columns in the token table |
| **Cache invalidation** | If a read-through cache is used, revocation must invalidate the cache entry |
| **Slightly more code** | Store update + read path vs. pure in-memory set |

### Best for

- Production deployments where restart, failover, or rolling updates are expected.
- Any deployment where token lifetime exceeds process lifetime.
- Compliance or audit scenarios where revocation must be provable.

---

## Option C — Hybrid (durable store + in-memory cache)

### How it works

- Revocation is written to the store first (durability).
- An in-memory cache (e.g., `moka` or `dashmap`) is populated lazily or eagerly.
- Lookups hit the cache first; cache misses fall back to the store.
- Cache TTL is short (e.g., 5–30 seconds) to balance latency and consistency.

### Pros

| Benefit | Detail |
|---------|--------|
| **Best of both** | Durability of Option B + near-zero latency of Option A for hot tokens |
| **Graceful degradation** | If cache is lost, behavior falls back to Option B |
| **Scalable** | Reduces store load under high request volume |

### Cons

| Cost | Detail |
|------|--------|
| **Complexity** | Two systems to maintain, test, and debug |
| **Consistency window** | A revoked token may be accepted for up to cache-TTL seconds on a cold cache node |
| **Operational burden** | Cache sizing, eviction, and monitoring become new concerns |

### Best for

- High-throughput production deployments where request latency is critical.
- Multi-node deployments with a shared store.

---

## Decision matrix

| Criterion | Option A (Immediate) | Option B (Durable) | Option C (Hybrid) |
|-----------|----------------------|--------------------|--------------------|
| Implementation complexity | Low | Medium | High |
| Revocation latency | Zero | Store round-trip | Near-zero (cached) |
| Survives restart | No (without WAL) | Yes | Yes |
| Multi-node safe | No | Yes | Yes (with caveats) |
| Audit trail | No | Yes | Yes |
| Suitable for pilot | Yes | Yes | Overkill |
| Suitable for production | Risky | Recommended | Future optimization |

## Engineering recommendation

**Default: Option B (Durable)** for the first implementation.

Rationale:
1. The schema already includes `revoked_at` in the proposed token model.
2. FerrumGate already depends on a store (SQLite or PostgreSQL) for all durable state.
3. The performance cost is negligible for pilot-tier request volumes.
4. It satisfies the SEC-4 acceptance criterion ("Revoked token returns 401") without edge cases around restart.
5. Option C can be added later as a performance optimization without breaking the contract.

**If the operator chooses Option A**, engineering can implement it as a stopgap, but must document that:
- Revocations are lost on restart.
- A manual re-revocation procedure is required after restart.
- The acceptance criteria for SEC-4 must include a restart-revocation test.

## Non-claims

- **NOT implemented**: This is a decision-support document. No code changes yet.
- **NOT a mandate**: The operator may choose Option A, B, or C. Engineering will implement the chosen option.
- **NOT performance-guaranteed**: Latency numbers are qualitative estimates, not measured benchmarks.
- **NOT production-ready**: This note supports a design decision, not a production claim.

## Related docs

- [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) — Source of the token model schema
- [`11-blockers-and-unblock-plan.md`](11-blockers-and-unblock-plan.md) — Operator decision packet (Q4)
- [`13-token-api-contract.md`](13-token-api-contract.md) — Token API contract

---

*End of file — Revocation Durability Tradeoff Note (planning artifact only).*
