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
- `GET /v1/executions/{execution_id}` - Get execution record

### Approvals
- `GET /v1/approvals` - List pending approvals
- `GET /v1/approvals/{approval_id}` - Get specific approval

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

## Upgrade-Track Routes (NOT in v1 single-node support contract)

The following routes are live in the router (`server.rs`) but are **NOT** in the v1
single-node support contract. They correspond to U1-U4 upgrade tracks documented in
`19-v1-single-node-support-contract.md §2.4` and are not production-verified for
single-node deployment.

### Provenance Ingest
- `POST /v1/provenance/ingest` — Ingest external provenance events (U3 Cross-runtime Provenance Fabric)

### Outcome Evaluation
- `POST /v1/executions/{execution_id}/evaluate-outcome` — Outcome-aware governance evaluation (U1)

### Runtime Bridges
- `GET /v1/bridges` — List available runtime bridges (U4 MCP/local/NemoClaw integrations)
- `GET /v1/bridges/{bridge_id}/tools` — List tools for a bridge (U4 MCP/local/NemoClaw integrations)

> **Execute/Verify boundary**: `POST /v1/executions/{id}/execute` and `POST /v1/executions/{id}/verify` **exist as actual HTTP routes** in `server.rs` for the fs-first FileWrite slice. They are **NOT** in the v1 support contract. See `docs/implementation-path/32-feature-completeness-audit.md` for full reconciliation.

### Policy Bundle Administration
- `POST /v1/policy-bundles` — Create policy bundle
- `GET /v1/policy-bundles` — List policy bundles
- `GET /v1/policy-bundles/{bundle_id}` — Get policy bundle
- `PUT /v1/policy-bundles/{bundle_id}` — Update policy bundle
- `DELETE /v1/policy-bundles/{bundle_id}` — Delete policy bundle
- `PUT /v1/policy-bundles/{bundle_id}/active` — Activate/deactivate policy bundle

> **Note**: Policy bundle endpoints are experimental/internal governance admin surface. See `docs/implementation-path/32-feature-completeness-audit.md` for classification details.

---

## Khi nào phải cập nhật đồng thời
Nếu thay:
- field names
- object semantics
- enum values
- API payload shapes
- invariant logic

thì phải sync lại giữa:
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

---

## Cross-References

- `docs/implementation-path/32-feature-completeness-audit.md` — Canonical route/API reconciliation with v1 support contract boundary classification
- `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` — v1 single-node support contract
- `docs/ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/04-api-roadmap.md` — API roadmap with post-v1 scope
