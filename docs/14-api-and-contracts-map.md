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
