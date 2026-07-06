> # ADR-015: Shared Nonce Cache for Agent Auth Replay Protection
>
> **Status:** Accepted
>
> **Date:** 2026-07-06
>
> ## Context
>
> Agent authentication uses a timestamped, signed request with a unique `X-Ferrum-Nonce`
> header to prevent replay attacks. The original implementation kept this nonce cache as an
> `Arc<Mutex<HashMap<String, Instant>>>` inside `ferrum-gateway`. This works for a single
> process, but any multi-process deployment (e.g., multiple `ferrumd` instances behind a
> load balancer) allows a nonce accepted by process A to be replayed against process B.
>
> A shared, persistent nonce cache is therefore required for multi-process deployments.
> The cache must remain small, fast, and fail-closed: any cache error must reject the
> request rather than allow a potential replay.
>
> ## Decision
>
> Introduce a `NonceCache` trait in `ferrum-store::repos` with two adapters:
>
> 1. `InMemoryNonceCache` — always available, preserves the existing process-local
>    behaviour (TTL prune + hard capacity bound).
> 2. `PostgresNonceCache` — compiled only with the `postgres` feature, backed by a
>    `nonce_cache` table.
>
> The trait deliberately stays out of `StoreFacade` so the gateway can construct it
> independently and so future adapters (e.g., Redis) do not require changes to the store
> facade.
>
> Configuration is additive:
>
> - `nonce_cache_backend`: `"auto"` (default), `"memory"`, or `"postgres"`.
> - `nonce_cache_ttl_secs`: explicit TTL; `0` derives from `agent_clock_skew_secs * 2`
>   with a minimum of 60 seconds.
> - `nonce_cache_max_entries`: in-memory capacity bound; default `10_000`.
>
> `auto` selects `PostgresNonceCache` when the store DSN is PostgreSQL and the
> `postgres` feature is enabled; otherwise it selects `InMemoryNonceCache`. Selecting
> `postgres` with a non-PostgreSQL store is a startup validation error.
>
> `NonceCache::check_and_insert` returns `Ok(true)` for a fresh nonce, `Ok(false)` for a
> replay, and `Err` on cache failure. The gateway treats `Err` as `401 Unauthorized`.
>
> `PostgresNonceCache` uses an atomic CAS upsert:
>
> ```sql
> INSERT INTO nonce_cache (nonce, expires_at)
> VALUES ($1, NOW() + MAKE_INTERVAL(secs => $2))
> ON CONFLICT (nonce) DO UPDATE
>     SET expires_at = EXCLUDED.expires_at
>     WHERE nonce_cache.expires_at <= NOW()
> RETURNING 1
> ```
>
> A row is inserted (or an expired row reclaimed) only when the returning query produces
> a row. An unexpired duplicate therefore returns `Ok(false)`. A periodic `vacuum` task
> deletes expired rows so the table does not grow without bound.
>
> Nonces are capped at 256 characters before the cache is consulted.
>
> ## Consequences
>
> - Single-process deployments keep the existing in-memory behaviour with no operational
>   change.
> - Multi-process PostgreSQL deployments can share replay protection by setting
>   `nonce_cache_backend = "auto"` or `"postgres"`.
> - Cache errors fail closed, preserving the security property even during database
>   outages.
> - SQLite deployments continue to use the in-memory cache; no SQLite nonce table is
>   introduced.
> - A small periodic reconciler is required for the Postgres backend to prevent
>   unbounded table growth.
