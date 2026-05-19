# FerrumGate API Guide

> **Status**: Expanded scaffold. All listed endpoints exist in `crates/ferrum-gateway/src/server.rs`. OpenAPI spec is not yet generated.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Base URL and versions

All endpoints are prefixed with `/v1`. The current API version is v1. There is no v0 or v2 yet.

Example base URLs:

```
http://127.0.0.1:18080/v1   # local dev
https://ferrumgate.example.com/v1   # hosted (when configured)
```

## Authentication

Two auth modes are supported:

| Mode | Behavior | Use case |
|------|----------|----------|
| `Disabled` | No auth required on any endpoint | Local development only |
| `Bearer` | `Authorization: Bearer <token>` required on all endpoints except health/metrics | Production and staging |

### Health and metrics endpoints (always unauthenticated)

- `GET /v1/healthz`
- `GET /v1/readyz`
- `GET /v1/readyz/deep`
- `GET /v1/metrics`

### Bearer token format

Generate a token:

```bash
openssl rand -hex 32
```

Set via config or env var `FERRUMD_BEARER_TOKEN`. Tokens are compared with constant-time equality.

### Auth errors

| Status | Code | Meaning |
|--------|------|---------|
| 401 | `Unauthorized` | Missing or invalid bearer token |

---

## Endpoint inventory

### Monitoring

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/healthz` | `GET` | none | Liveness probe |
| `/v1/readyz` | `GET` | none | Readiness probe |
| `/v1/readyz/deep` | `GET` | none | Deep readiness (store health, queue depth, pool saturation) |
| `/v1/metrics` | `GET` | none | Prometheus-compatible metrics |

### Intent and policy

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/intents/compile` | `POST` | bearer | Compile a new intent |
| `/v1/intents` | `GET` | bearer | List compiled intents |
| `/v1/proposals/{proposal_id}/evaluate` | `POST` | bearer | Evaluate a proposal against active policy |

### Capabilities

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/capabilities/mint` | `POST` | bearer | Mint a capability for an allowed proposal |
| `/v1/capabilities/{capability_id}/revoke` | `POST` | bearer | Revoke a capability before use |

### Execution lifecycle

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/executions/authorize` | `POST` | bearer | Authorize execution with a capability |
| `/v1/executions/{execution_id}/prepare` | `POST` | bearer | Prepare side effect (snapshot, validation) |
| `/v1/executions/{execution_id}/execute` | `POST` | bearer | Execute the prepared action |
| `/v1/executions/{execution_id}/verify` | `POST` | bearer | Verify the outcome |
| `/v1/executions/{execution_id}/evaluate-outcome` | `POST` | bearer | Evaluate whether outcome aligns with intent |
| `/v1/executions/{execution_id}/compensate` | `POST` | bearer | Trigger compensation/rollback |
| `/v1/executions/{execution_id}/cancel` | `POST` | bearer | Cancel execution before commit |
| `/v1/executions/{execution_id}` | `GET` | bearer | Get execution record |

### Approvals

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/approvals` | `GET` | bearer | List pending approvals |
| `/v1/approvals/{approval_id}` | `GET` | bearer | Get approval details |
| `/v1/approvals/{approval_id}/resolve` | `POST` | bearer | Approve or reject |

### Provenance and lineage

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/provenance/query` | `POST` | bearer | Query provenance events |
| `/v1/provenance/lineage/{execution_id}` | `GET` | bearer | Get lineage for an execution |
| `/v1/provenance/lineage` | `POST` | bearer | Multi-hop lineage query |
| `/v1/provenance/ingest` | `POST` | bearer | Ingest external provenance events |

### Bridges and tools

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/bridges` | `GET` | bearer | List registered bridges |
| `/v1/bridges/{bridge_id}/tools` | `GET` | bearer | List tools exposed by a bridge |

### Policy bundles

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/policy-bundles` | `POST` | bearer | Create a policy bundle |
| `/v1/policy-bundles` | `GET` | bearer | List policy bundles |
| `/v1/policy-bundles/{bundle_id}` | `GET` | bearer | Get a policy bundle |
| `/v1/policy-bundles/{bundle_id}` | `PUT` | bearer | Update a policy bundle |
| `/v1/policy-bundles/{bundle_id}` | `DELETE` | bearer | Delete a policy bundle |
| `/v1/policy-bundles/{bundle_id}/active` | `PUT` | bearer | Set bundle as active |

---

## Execution lifecycle example

The following curl sequence demonstrates the full lifecycle validated locally:

```bash
# 1. Health check
curl http://127.0.0.1:18080/v1/healthz

# 2. Compile intent
curl -X POST http://127.0.0.1:18080/v1/intents/compile \
  -H "Content-Type: application/json" \
  -d '{"action":"fs.FileWrite","target":"/tmp/demo.txt","parameters":{"content":"hello"}}'

# 3. Evaluate proposal
curl -X POST http://127.0.0.1:18080/v1/proposals/{proposal_id}/evaluate \
  -H "Content-Type: application/json" \
  -d '{}'

# 4. Mint capability
curl -X POST http://127.0.0.1:18080/v1/capabilities/mint \
  -H "Content-Type: application/json" \
  -d '{"proposal_id":"...","ttl_secs":120}'

# 5. Authorize execution
curl -X POST http://127.0.0.1:18080/v1/executions/authorize \
  -H "Content-Type: application/json" \
  -d '{"capability_id":"..."}'

# 6. Prepare
curl -X POST http://127.0.0.1:18080/v1/executions/{execution_id}/prepare \
  -H "Content-Type: application/json" \
  -d '{}'

# 7. Execute
curl -X POST http://127.0.0.1:18080/v1/executions/{execution_id}/execute \
  -H "Content-Type: application/json" \
  -d '{}'

# 8. Verify
curl -X POST http://127.0.0.1:18080/v1/executions/{execution_id}/verify \
  -H "Content-Type: application/json" \
  -d '{}'

# 9. Evaluate outcome
curl -X POST http://127.0.0.1:18080/v1/executions/{execution_id}/evaluate-outcome \
  -H "Content-Type: application/json" \
  -d '{}'

# 10. Query lineage
curl http://127.0.0.1:18080/v1/provenance/lineage/{execution_id}
```

> **Note**: Replace placeholder IDs with actual values returned by each step. This flow assumes `auth_mode=disabled` (local dev).

---

## Error format

All errors share a uniform JSON structure:

```json
{
  "code": "Unauthorized",
  "message": "missing or invalid bearer token",
  "correlation_id": "550e8400-e29b-41d4-a716-446655440000",
  "retriable": false,
  "details": {}
}
```

| Field | Meaning |
|-------|---------|
| `code` | Machine-readable error code |
| `message` | Human-readable description |
| `correlation_id` | UUID for tracing this error through logs |
| `retriable` | Whether retrying the same request may succeed |
| `details` | Additional structured context (varies by endpoint) |

Common HTTP status codes:

| Status | Typical cause |
|--------|---------------|
| 200 | Success |
| 400 | Bad request (malformed JSON, missing field) |
| 401 | Unauthorized (bearer token missing/invalid) |
| 404 | Resource not found |
| 409 | Conflict (capability already used, execution already in terminal state) |
| 422 | Unprocessable (policy denied, validation failed) |
| 429 | Rate limited (governor limit exceeded) |
| 500 | Internal server error |

---

## Rate limiting

The workload router is protected by a governor layer. Default limits are configurable:

```toml
rate_limit_per_second = 2
rate_limit_burst = 50
```

When rate limited, the API returns HTTP 429. Health and metrics endpoints are not rate limited.

---

## Status caveat

> **production-ready = NO**. This endpoint inventory reflects the current implementation in `crates/ferrum-gateway/src/server.rs`. Not all endpoints have full integration tests against every adapter. See [`docs/ROADMAP.md`](../../ROADMAP.md) for gaps.

## Related docs

- [`quickstart.md`](./quickstart.md) — Complete curl walkthrough with timing.
- [`concepts.md`](./concepts.md) — Intent, capability, provenance, and lineage explained.
- [`operator.md`](./operator.md) — Config, auth modes, and deployment.
