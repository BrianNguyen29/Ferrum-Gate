# Runtime Configuration Notes — FerrumGate Governance Gateway

## SQLite Configuration

### Connection Pool
- **Pool size**: 20 connections (default).
- **Write queue**: Enabled. All SQLite writes are funneled through a single in-process `mpsc` write queue to eliminate lock thrash and `SQLITE_BUSY` retry storms.
- **busy_timeout**: 5000ms (5s), now primarily a defensive fallback rather than the main contention-control mechanism.
- **WAL mode**: Enabled by default. Provides concurrent read access while writes are being serialized by the queue.
- **PRAGMA tuning**: `synchronous=NORMAL`, `wal_autocheckpoint=1000`, `cache_size=-64000`, `busy_timeout=5000`.

### Write Concurrency Limits
SQLite still has a **single-writer** storage model, but the gateway now serializes writes explicitly with a write queue instead of allowing many handlers to contend on the database lock.

**Operational guidance:**
- Multi-step pipelines at **5 workers** are now stable with **0% errors**.
- Pure concurrent write ingestion at **50 workers** is now stable with **0% errors** and acceptable latency for stress conditions.
- Throughput is now limited mostly by queue drain rate and payload size, not by lock contention/retry storms.
- PostgreSQL can be selected for higher write throughput or multi-process deployments when configured.

### FK Constraint Chain
The database schema has cascading foreign keys:
```
intents → proposals → capabilities → executions → rollback_contracts
```
All FK parent inserts (compile_intent, evaluate_proposal) are **synchronous** to guarantee FK integrity. The gateway handler returns 200 only after the record is persisted.

### Persistence Strategy
| Handler              | DB Write Mode                  | Rationale                                         |
|---------------------|--------------------------------|---------------------------------------------------|
| compile_intent       | Synchronous via write queue    | FK parent for proposals, capabilities             |
| evaluate_proposal    | Synchronous via write queue    | FK parent for capabilities, executions            |
| mint_capability      | Synchronous via write queue    | FK to intent + proposal; retries no longer needed |
| authorize_execution  | Synchronous via write queue    | FK to capability; retries no longer needed        |
| revoke_capability    | Synchronous via write queue     | Persisted state; durable fallback after in-memory loss |
| ingest_provenance    | Synchronous via write queue    | Provenance events remain immediately queryable    |

## Stress Test Baseline — Pre-Write-Queue (release binary, SQLite file-backed)

| #  | Scenario                         | Workers | Throughput       | p50 Latency | Error Rate |
|----|---------------------------------|---------|------------------|-------------|------------|
| S1 | health (GET /v1/healthz)         | 50      | ~39,000 req/s    | 1.1ms       | 0%         |
| S2 | auth (GET /v1/approvals)          | 50      | ~21,000 req/s    | 2.5ms       | 0%         |
| S3 | provenance-query (POST)          | 50      | ~20,000 req/s    | 2.3ms       | 0%         |
| S4 | intent-compile (POST, sync write) | 5       | ~6.5 req/s       | ~140ms      | ~10%       |
| S5 | execution-pipeline (6 steps)     | 5       | ~1.1 pipelines/s | ~958ms      | ~73%       |
| S6 | capability (mint→revoke cycle)   | 5       | ~1.5 req/s       | varies      | ~33%       |
| S7 | sqlite-contention (ingest writes) | 50      | ~8.1 req/s       | ~3.26s      | 0%*        |
| S8 | rate-limit (burst detection)     | 50      | ~52,000 req/s    | 0.9ms       | 0%         |
| S9 | mixed workload                   | 5       | ~16 req/s        | ~1.8ms      | ~2.5%      |

*Pre-write-queue S7 showed 0% errors because requests queued behind SQLite's writer lock, but latency was extremely high due to lock serialization.

## Stress Test Results — Post-Write-Queue

Release build, full `ferrum-stress` suite after WriteQueue + PRAGMA tuning + retry cleanup:

| #  | Scenario                         | Workers | Throughput        | p50 Latency | Error Rate |
|----|---------------------------------|---------|-------------------|-------------|------------|
| S1 | health (GET /v1/healthz)         | 50      | 33,126.1 req/s    | 1.28ms      | 0%         |
| S2 | auth (GET /v1/approvals)         | 50      | 13,646.5 req/s    | 3.03ms      | 0%         |
| S3 | provenance-query (POST)          | 50      | 16,311.5 req/s    | 2.76ms      | 0%         |
| S4 | intent-compile                   | 5       | 305.5 req/s       | 2.25ms      | 0%         |
| S5 | execution-pipeline               | 5       | 57.6 req/s        | 16.0ms      | 0%         |
| S6 | capability (mint→revoke cycle)   | 5       | 42.0 req/s        | 0.30ms      | 0%         |
| S7 | sqlite-contention (ingest writes)| 50      | 289.4 req/s       | 29.9ms      | 0%         |
| S8 | rate-limit (burst detection)     | 50      | 28,813.3 req/s    | 1.47ms      | 0%         |
| S9 | mixed workload                   | 5       | 123.8 req/s       | 4.80ms      | 0%         |

### Before / After Highlights

- **S4**: `6.5 req/s → 305.5 req/s` (~47x), `~10% → 0%` errors
- **S5**: `1.1 req/s → 57.6 req/s` (~52x), `~73% → 0%` errors
- **S6**: `1.5 req/s → 42.0 req/s` (~28x), `~33% → 0%` errors
- **S7**: `8.1 req/s → 289.4 req/s` (~36x), `3.26s → 29.9ms` p50 latency
- **S9**: `16 req/s → 123.8 req/s` (~7.7x), `~2.5% → 0%` errors

*S8 was run without rate limiting enabled in the stress test server configuration, so no `429` responses were expected or observed.

## Performance Optimization Plan

See [`docs/operations/`](./operations/) for operational guides.

Optimization history:

| Item | Solution | Effort | Outcome |
|------|----------|--------|---------|
| **1** | Write-Queue (mpsc) + retry cleanup + PRAGMA tuning | Implemented | All errors 0%, S7 p50 29.9ms |
| **2** | Transaction batching for pipelines + direct UPDATE | - | Perf regression observed in benchmarking; revisit after optimization |
| **3** | PostgreSQL migration | - | 1000+ writes/s, 200+ pipelines/s |

## Scaling Beyond SQLite

PostgreSQL is recommended for deployments requiring materially higher sustained write throughput, cross-process or multi-node deployment, or stronger transactional flexibility (not currently implemented in `ferrum-store`).

## Authentication
- **Bearer token mode**: Set `auth_mode = "Bearer"` and `bearer_token` in config
- Tokens are validated with constant-time comparison (timing-attack resistant)
- Health/readiness endpoints are always unauthenticated

## Health and Readiness Endpoints

| Endpoint | Purpose | Store Check | Queue Check | Always Returns 200 |
|----------|---------|-------------|-------------|-------------------|
| `/v1/healthz` | Liveness probe | No | No | Yes |
| `/v1/readyz` | Readiness probe (shallow) | No | No | Yes |
| `/v1/readyz/deep` | **Functional readiness probe** | **Yes** | **Yes (threshold: 100)** | **No (200 when healthy, 503 when degraded)** |
| `/v1/metrics` | Prometheus-compatible metrics | Yes | Yes | Yes |

### `/v1/readyz/deep` Components
The deep readiness probe returns three components:
1. **store**: Database connectivity and health
2. **write_queue**: SQLite write queue backpressure (healthy when depth ≤ 100, unhealthy when depth > 100)
3. **pool**: Connection pool status. PostgreSQL stores report pool saturation and return `degraded` when `idle_connections == 0 && total_connections >= max_connections` for `max > 0`; non-pool stores report `not applicable` and remain healthy.

The `write_queue` component provides bounded backpressure detection only; it does not indicate full dependency health, ledger scan status, adapter health, rollback health, or schema integrity.
The `pool` component is emitted for all stores; SQLite/non-pool stores report `not applicable`.

**Load balancer / Kubernetes guidance**:
- Use **`/v1/readyz/deep`** for load balancer health checks and Kubernetes readiness probes.
  This endpoint returns HTTP 503 when the SQLite store is unreachable, unhealthy, or when the write queue depth exceeds 100,
  allowing load balancers to route traffic away from degraded instances.
- **`/v1/healthz`** and **`/v1/readyz`** always return HTTP 200 — do NOT use these
  for load balancer or Kubernetes readiness probes. They do not check store health.
- **`/v1/metrics`** (`GET /v1/metrics`) returns Prometheus text format with request counters,
  `ferrumgate_store_health_up` gauge, `ferrumgate_write_queue_depth` gauge,
  `ferrumgate_governance_errors_total` per route, and `ferrumgate_governance_success_total` per route.
  It does not cause 503 on store failure.

**Metrics** (`/v1/metrics`) include **bounded latency histograms** (`ferrumgate_request_duration_seconds`)
for public endpoints (`/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`) with labels
`route`, `method`, `status`, `le` (bucket boundary), emitting `_bucket`, `_sum`, `_count` lines.
For PostgreSQL stores, pool metrics (`ferrumgate_store_pg_pool_size`, `ferrumgate_store_pg_pool_idle`,
`ferrumgate_store_pg_pool_max`) and acquire-timeout counters (`ferrumgate_store_pg_acquire_timeouts_total`)
are also emitted. SQLite stores omit these metrics.


## Logging

**Default format**: Human-readable text (compact style). This is the default when `log_format` is
not specified.

**JSON format**: Structured JSON logs for centralized log aggregation systems (ELK, Loki, etc.).
Enable via CLI (`--log-format json`), environment variable (`FERRUMD_LOG_FORMAT=json`), or config
file (`log_format = "json"` under `[server]`).

Config precedence: CLI > env > config file > defaults.

```toml
# Example: enable JSON logging via config file
[server]
log_format = "json"
```

```bash
# Example: enable JSON logging via environment variable
FERRUMD_LOG_FORMAT=json ferrumd --config /path/to/deployed.toml
```

**Log fields** (JSON format): `timestamp`, `level`, `message`, `target`, and `spans` (if any).
Text format includes: `timestamp`, `level`, `target`, `message`.

## Rate Limiting
- Built-in via `tower_governor`: 2 req/s sustained, burst of 50
- Applied per-IP using `GovernorLayer`
- Periodic cleanup of rate limiter entries (every 60s)

## Capability TTL
- Maximum TTL: **300 seconds** (5 minutes, hardcoded in `ferrum-cap` service)
- Default TTL: Configured per-request via `requested_ttl_secs`
- Expired capabilities return `CapabilityExpired` error

## Rate-limit configuration

- Default `2/50`: high 429 under sustained load (expected)
- Tuned `20/500`: also high 429 under spike load (expected)
- Max-valid `1000/10000`: passes validation workload with 0% 429
- Operator must tune based on real traffic and IP distribution. See `docs/operations/rate-limit-tuning-guide.md`.

## HA Configuration

- **Single-node PostgreSQL**: Default. SQLite also supported.
- **Same-VM PostgreSQL replication**: Streaming replication with watchdog failover has been validated on a single VM.
- **Multi-host clustering**: Operator must design their own clustering, load balancing, and failover if required.

## Related docs

- [`docs/guides/`](./guides/) — Product/user guide scaffolds.
