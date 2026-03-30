# 14 — API and contracts map

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
- `POST /v1/capabilities/{capability_id}/revoke` - Revoke capability

### Executions
- `POST /v1/executions/authorize` - Authorize execution
- `POST /v1/executions/{execution_id}/prepare` - Prepare rollback/preflight
- `POST /v1/executions/{execution_id}/execute` - Execute the prepared operation
- `POST /v1/executions/{execution_id}/verify` - Verify execution result against intent and policy
- `POST /v1/executions/{execution_id}/compensate` - Compensate execution (may be noop-backed)
- `POST /v1/executions/{execution_id}/cancel` - Cancel execution in pre-execute state (Proposed, Authorized, Prepared)
- `POST /v1/executions/{execution_id}/pause` - Pause execution in running state (Running, AwaitingVerification)
- `POST /v1/executions/{execution_id}/resume` - Resume paused execution
- `GET /v1/executions/{execution_id}` - Get execution record

### Approvals
- `GET /v1/approvals` - List pending approvals
- `GET /v1/approvals/{approval_id}` - Get specific approval
- `POST /v1/approvals/{approval_id}/resolve` - Resolve a pending approval (approve or deny)

### Provenance
- `POST /v1/provenance/query` - Query provenance events
- `GET /v1/provenance/lineage/{execution_id}` - Get lineage for execution
- `POST /v1/provenance/lineage` - Multi-hop lineage query from seed event (supports ancestors, descendants, both directions with bounded depth)

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
