# 14 — API and contracts map

> **Role**: External API route / contract mapping. Enumerates REST endpoints, OpenAPI specs, contract files, and JSON schemas. For gateway component structure, see [`03-architecture.md`](./03-architecture.md). For per-adapter contract details (HTTP binding enforcement, auth, rollback), see [`13-adapter-contracts.md`](./13-adapter-contracts.md). For the supported v1 scope and what is/is not included, see [`00-project-canon.md`](./00-project-canon.md).

## Contracts
- `contracts/ferrumgate-agent-contract.v1.yaml`
- `contracts/ferrumgate-integrator-contract.v1.yaml`
- `contracts/policy-bundle.example.yaml`

## OpenAPI
- `openapi/ferrumgate-control-api.v1.yaml`

## Implemented REST Endpoints

### Unauthenticated (health/readiness)
- `GET /v1/healthz` - Basic health check
- `GET /v1/readyz` - Shallow readiness check

### Policy and Intents
- `POST /v1/intents/compile` - Compile an IntentEnvelope
- `POST /v1/proposals/{proposal_id}/evaluate` - Evaluate proposal via policy

### Capabilities
- `POST /v1/capabilities/mint` - Mint a capability lease
- `GET /v1/capabilities/{capability_id}` - Inspect a capability lease
- `POST /v1/capabilities/{capability_id}/revoke` - Revoke capability

### Executions
- `POST /v1/executions/authorize` - Authorize execution
- `POST /v1/executions/{execution_id}/prepare` - Prepare rollback/preflight
- `POST /v1/executions/{execution_id}/execute` - Execute the prepared operation
- `POST /v1/executions/{execution_id}/verify` - Verify execution result against intent and policy
- `POST /v1/executions/{execution_id}/commit` - Commit a verified execution
- `POST /v1/executions/{execution_id}/compensate` - Compensate execution (may be noop-backed)
- `POST /v1/executions/{execution_id}/rollback` - Rollback/compensate via rollback contract
- `POST /v1/executions/{execution_id}/cancel` - Cancel execution in pre-execute state (Proposed, Authorized, Prepared)
- `POST /v1/executions/{execution_id}/pause` - Pause execution in running state (Running, AwaitingVerification)
- `POST /v1/executions/{execution_id}/resume` - Resume paused execution
- `GET /v1/executions/{execution_id}` - Get execution record

> **HTTP adapter**: HTTP binding enforcement, request digest, auth handling, and conservative rollback no-op are defined in [`13-adapter-contracts.md`](./13-adapter-contracts.md), not in this document.

### Approvals
- `GET /v1/approvals` - List pending approvals
- `GET /v1/approvals/{approval_id}` - Get specific approval
- `POST /v1/approvals/{approval_id}/resolve` - Resolve a pending approval (approve or deny)

### Provenance
- `POST /v1/provenance/query` - Query provenance events
- `GET /v1/provenance/lineage/{execution_id}` - Get lineage for execution
- `POST /v1/provenance/lineage` - Multi-hop lineage query from seed event (supports ancestors, descendants, both directions with bounded depth)

### Additional implemented routes (outside the v1 single-node support contract)

These routes exist in the live router but are not part of the v1 single-node T1
support surface described in `19-v1-single-node-support-contract.md`.

- `GET /metrics` - metrics scrape endpoint
- `GET /v1/provenance/events/{event_id}` - inspect a single provenance event
- `POST /v1/provenance/replay` - replay/export-oriented provenance operation
- `POST /v1/provenance/export` - export provenance data
- `POST /v1/provenance/stats` - provenance statistics/diagnostics
- `POST /v1/provenance/events/external` - external provenance ingest surface
- `GET /v1/sync/leader/tip` - leader-side sync probe
- `GET /v1/sync/leader/tip/proof` - leader-side sync proof probe
- `GET /v1/ledger/verify` - ledger verification diagnostics

> **CLI Export**: `ferrumctl server inspect-lineage <execution_id>` supports `--format text|json|dot` and `--output <path>` for exporting lineage as Graphviz DOT format without any API changes.

## Schemas
- `schemas/jsonschema/common.json`
- `schemas/jsonschema/intent-envelope.json`
- `schemas/jsonschema/action-proposal.json`
- `schemas/jsonschema/capability-lease.json`
- `schemas/jsonschema/rollback-contract.json`
- `schemas/jsonschema/provenance-event.json`
- `schemas/jsonschema/approval-request.json`

## Khi nao phai cap nhat dong thoi
Neu thay:
- field names
- object semantics
- enum values
- API payload shapes
- invariant logic

thi phai sync lai giua:
- code
- docs
- contracts
- schemas
- openapi

## Authentication

When `auth_mode = "bearer"`, all endpoints except `/v1/healthz` and `/v1/readyz` require:
```
Authorization: Bearer <token>
```

## TLS

This API does not terminate TLS. Deploy behind a TLS-terminating proxy (e.g., nginx, cloud load balancer) for production use.
