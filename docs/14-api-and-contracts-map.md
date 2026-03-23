# 14 — API and contracts map

## Contracts
- `contracts/ferrumgate-agent-contract.v1.yaml`
- `contracts/ferrumgate-integrator-contract.v1.yaml`
- `contracts/policy-bundle.example.yaml`

## OpenAPI
- `openapi/ferrumgate-control-api.v1.yaml`

## Schemas
- `schemas/jsonschema/common.json`
- `schemas/jsonschema/intent-envelope.json`
- `schemas/jsonschema/action-proposal.json`
- `schemas/jsonschema/capability-lease.json`
- `schemas/jsonschema/rollback-contract.json`
- `schemas/jsonschema/provenance-event.json`
- `schemas/jsonschema/approval-request.json`

## Core control-plane endpoints (operator quick-ref)

| Lifecycle step | Purpose |
|---------------|---------|
| `POST /v1/intents/compile` | Parse and scope intent from agent |
| `POST /v1/proposals/{proposal_id}/evaluate` | Evaluate proposal against policy bundle |
| `POST /v1/capabilities/mint` | Issue a limited-capability lease |
| `POST /v1/executions/authorize` | Gateway-level capability check before execution |
| `POST /v1/executions/{execution_id}/prepare` | Prepare rollback contract for the operation |
| `POST /v1/executions/{execution_id}/execute` | Run the tool/adapter (fs, git, sqlite, http, maildraft) |
| `POST /v1/executions/{execution_id}/verify` | Verify result against intent and policy |
| `POST /v1/executions/{execution_id}/commit` | Finalize and commit the action |
| `POST /v1/executions/{execution_id}/compensate` | Trigger compensation when a recovery path exists |
| `POST /v1/executions/{execution_id}/rollback` | Trigger rollback via prepared adapter |
| `GET /v1/executions/{execution_id}` | Inspect the stored execution record |
| `GET /v1/approvals` | List pending approvals |
| `GET /v1/approvals/{approval_id}` | Inspect a specific approval |
| `GET /v1/provenance/lineage/{execution_id}` | Inspect the execution lineage chain |
| `GET /v1/provenance/events/{event_id}` | Inspect a single provenance event, optionally with `?ancestry=true` and/or `?descendants=true` |
| `POST /v1/provenance/query` | Query provenance events by `intent_id`, `proposal_id`, `execution_id`, `capability_id`, `event_kind`, time window, or `terminal_only` |

HTTP adapter rollback is a **no-op by design** today; see `15-deployment-and-operations.md` for caveats.

When `ferrumd` runs with `auth.mode = "bearer"`, all non-health control-plane routes require `Authorization: Bearer <token>`.

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
