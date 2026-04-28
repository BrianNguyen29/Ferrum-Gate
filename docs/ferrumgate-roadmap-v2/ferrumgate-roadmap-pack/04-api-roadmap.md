# 04 — API roadmap

## Execution Pack — Q1–Q2 API sequencing

This document (`04`) tracks API surface evolution for the execution pack.

### Q1 API dependency gates
| Gate | Route / behavior | Dependency |
|---|---|---|
| Q1-G1 | Evaluate endpoint naming reconciled | Proto shapes stable (G1 in `03`) |
| Q1-G2 | Lineage minimum-chain events confirmed | Store provenance append-only (G2 in `03`) |
| Q1-G3 | Single-use enforcement visible in authorize response | Cap mark_used closed (G3 in `03`) |

### Q2 API dependency gates
| Gate | Route / behavior | Dependency |
|---|---|---|
| Q2-G1 | Adapter artifact summary in execution inspect | Store adapter persistence ready (G4 in `03`) |
| Q2-G2 | fs/git/db mutation summaries in inspect payload | Adapter real implementations complete (G5 in `03`) |

### Evidence expectations
For each API change: record the OpenAPI diff or route table diff in
`docs/artifacts/<date>/` showing the before/after state.

---

## API principles

- API surface phải phản ánh đúng support scope, không overclaim
- mọi thay đổi field names/object semantics/enum values/API payload shapes/invariant logic phải sync code + docs + contracts + schemas + openapi
- trừ health/ready, production mode phải đi qua bearer auth

## Current supported baseline

> **V1 boundary**: The baseline below reflects the routes that are in the v1 single-node
> support contract (`19-v1-single-node-support-contract.md`). This is the **only
> authoritative list** of what is supported in v1. Routes listed in this document
> that are **not** in the v1 support contract are post-v1 scope, regardless of
> whether they appear as router entries in `ferrum-gateway/src/server.rs`.

### Health
- `GET /v1/healthz`
- `GET /v1/readyz`

### Intent / Policy
- `POST /v1/proposals/{proposal_id}/evaluate`

### Capability
- `POST /v1/capabilities/mint`

### Execution
- `POST /v1/executions/authorize`
- `POST /v1/executions/{execution_id}/prepare`
- `POST /v1/executions/{execution_id}/compensate`
- `GET /v1/executions/{execution_id}`

### Approvals
- `GET /v1/approvals`
- `GET /v1/approvals/{approval_id}`

### Provenance
- `POST /v1/provenance/query`
- `GET /v1/provenance/lineage/{execution_id}`
- `POST /v1/provenance/lineage`

### Routes that exist in the router but are post-v1 scope

The following routes exist in `ferrum-gateway/src/server.rs` but are **not** in the
v1 single-node support contract. They are listed here for clarity; do not claim
they are v1-supported:

- `POST /v1/intents/compile` — not in v1 support contract
- `POST /v1/capabilities/{capability_id}/revoke` — not in v1 support contract
- `POST /v1/executions/{execution_id}/evaluate-outcome` — not in v1 support contract
- `POST /v1/provenance/ingest` — not in v1 support contract
- `GET /v1/bridges` — not in v1 support contract
- `GET /v1/bridges/{bridge_id}/tools` — not in v1 support contract

The v1 router does **not** expose:
- `POST /v1/executions/{id}/commit` — not exposed in v1
- `POST /v1/executions/{id}/rollback` — not exposed in v1

## Q1 API tasks

### Canonicalization
- [ ] reconcile route naming inconsistency for evaluate endpoint
- [ ] publish canonical route table
- [ ] ensure OpenAPI and docs match runtime

### Execution integrity
- [ ] verify authorize/prepare flow returns enough state for debugging single-use and rollback class enforcement
- [ ] ensure prepare response exposes correct rollback metadata

### Provenance completeness
- [ ] define exact minimum lineage events in API docs
- [ ] assert endpoint returns all required terminal-path events

### Approval clarity
- [ ] document digest binding fields clearly in approval payload

## Q2 API additions/expansions

### Execution detail enrichment
- [ ] include adapter artifact summary in execution inspect (safe subset only)
- [ ] include verify status / compensate status / last error fields

### Recovery
- [ ] if compensate endpoint already exists, enrich response with verify-after-compensate result or explicit unknown state
- [ ] do not expose commit/rollback if not implemented

> **Execute/Verify HTTP surface status (2026-04-11):** `POST /v1/executions/{id}/execute` and
> `POST /v1/executions/{id}/verify` **exist** for the fs-first FileWrite slice in the gateway router (server.rs:155–162) per `11-gateway-execute-verify-surface-design-note.md`. The fs-first FileWrite slice exercises prepare → persist → execute → verify → compensate/restore at HTTP level. Git and sqlite adapter execute/verify endpoints are not yet implemented. This is a fs-first slice only; do not claim full Q2 adapter scope is complete.
>
> **Reconciliation**: See `docs/implementation-path/32-feature-completeness-audit.md` for canonical route classification (v1-supported vs post-v1 vs experimental/internal).

## Q3 API additions/expansions

### Operator plane APIs
- [ ] richer approvals filtering
- [ ] incident/quarantine listing endpoint or query mode
- [ ] evidence bundle export endpoint or signed artifact generation path

### Deployment/ops
- [ ] deeper readiness or diagnostic endpoint only if semantics are honest
- [ ] configuration redaction-safe diagnostics if needed

## Q4 API additions/expansions

### MCP/runtime governance
- [ ] tool execution metadata in proposal/evaluation payloads
- [ ] tool/resource constraint schemas
- [ ] runtime trust/taint propagation fields

### Evidence plane
- [ ] export evidence bundle endpoint
- [ ] signed approval retrieval
- [ ] tamper-evidence metadata retrieval

## API do-not-do list

- [ ] do not expose routes not backed by real semantics
- [ ] do not expose raw internal control data to user plane
- [ ] do not overpromise rollback if adapter is noop-backed
- [ ] do not add unstable fields without schema and docs update

---

## Cross-References

- `docs/implementation-path/32-feature-completeness-audit.md` — Canonical route/API reconciliation with v1 support contract boundary
- `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` — v1 single-node support contract
- `docs/ferrumgate-roadmap-v1/14-api-and-contracts-map.md` — API contracts map
