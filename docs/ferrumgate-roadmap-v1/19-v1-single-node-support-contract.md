# 19 — v1 Single-Node Support Contract

FerrumGate v1 single-node production support contract. This document is the
canonical reference for what is and is not supported in the FerrumGate v1
single-node release. All other docs should link here rather than restating
support scope.

**Scope**: single-node, SQLite-backed, v1 only.
**Last updated**: 2026-04-27.

---

## 1. Supported

### 1.1 Deployment model

- Single-node governance core with SQLite-backed persistence.
- SQLite store via `ferrum-store` with embedded migrations.
- Config file, environment variable, and CLI argument configuration.
- Bearer-token authentication mode.
- Manual file-level SQLite backup and restore.

### 1.2 Supported routes

The following REST endpoints are in the v1 single-node support contract:

| Method | Route | Description |
|---|---|---|
| GET | /v1/healthz | Shallow health check |
| GET | /v1/readyz | Shallow readiness check |
| GET | /v1/readyz/deep | Deep readiness check with store probe (SQLite `SELECT 1`) |
| POST | /v1/proposals/{proposal_id}/evaluate | Evaluate proposal via policy |
| POST | /v1/capabilities/mint | Mint a capability lease |
| POST | /v1/executions/authorize | Authorize execution |
| POST | /v1/executions/{execution_id}/prepare | Prepare rollback/preflight |
| POST | /v1/executions/{execution_id}/compensate | Compensate (may be noop-backed) |
| GET | /v1/executions/{execution_id} | Inspect execution record |
| GET | /v1/approvals | List pending approvals (pagination, filter by proposal_id) |
| GET | /v1/approvals/{approval_id} | Get specific approval |
| GET | /v1/provenance/lineage/{execution_id} | Get lineage for execution |
| POST | /v1/provenance/lineage | Multi-hop lineage query (ancestors/descendants/both, bounded depth) |
| POST | /v1/provenance/query | Query provenance events (intent_id, execution_id, capability_id, event_kind, time range) |

### 1.3 CLI surface (inspect-only)

The following `ferrumctl` commands are in the v1 single-node support contract:

- `ferrumctl server health` — shallow health probe
- `ferrumctl server inspect-execution <execution_id>` — fetch execution record
- `ferrumctl server inspect-approvals` — list approvals (pagination/filtering available via REST API at `/v1/approvals`)
- `ferrumctl server inspect-approval <approval_id>` — fetch single approval
- `ferrumctl server inspect-lineage <execution_id>` — fetch lineage for execution
- `ferrumctl server inspect-provenance --intent-id <intent_id>` — query provenance events (intent-id-only via CLI; richer filtering via POST /v1/provenance/query)

Mutating `ferrumctl` commands are post-v1 scope.

---

## 2. Not Supported

> **v1 Lock Statement**: FerrumGate v1 is locked to single-node SQLite-backed
> deployment. The support contract in this document is the definitive boundary.
> U1–U4 upgrade tracks (Outcome-aware Governance, Reversible Execution Planner,
> Cross-runtime Provenance Fabric, MCP/local/NemoClaw runtime integrations)
> are implemented work outside the v1 support baseline; they are post-v1 scope
> and are not covered by this support contract.

The following are explicitly out of scope for FerrumGate v1 single-node:

### 2.1 Deployment

- Multi-node deployments of any kind.
- High-availability (HA) configurations.
- Read-replica configurations.
- Any deployment model other than single-node SQLite.

### 2.2 Adapter-backed integrations

- Real adapter implementations: `ferrum-adapter-fs`, `ferrum-adapter-sqlite`, `ferrum-adapter-maildraft`, `ferrum-adapter-git`, `ferrum-adapter-http`.
  These are skeleton crate/API shapes only; no production-verified side-effect integrations exist in v1.
- Guaranteed external undo via adapter. Compensate may be noop-backed.

### 2.3 Routes not in v1 router

- `POST /v1/executions/{id}/commit` — **not exposed** in the v1 router.
- `POST /v1/executions/{id}/rollback` — **not exposed** in the v1 router.
  The gateway flow terminates at compensate as the recovery endpoint.

### 2.4 Routes implemented but outside v1 support contract

The following routes are **implemented** in the gateway but are **not** part of the v1
single-node support contract. They are classified as experimental/internal or post-v1
scope and must not be claimed as v1 production-supported:

#### Policy bundle CRUD + activate (experimental/internal governance admin surface)

| Method | Route | Description |
|---|---|---|
| POST | `/v1/policy-bundles` | Create a policy bundle |
| GET | `/v1/policy-bundles` | List all policy bundles |
| GET | `/v1/policy-bundles/{bundle_id}` | Get a specific policy bundle |
| PUT | `/v1/policy-bundles/{bundle_id}` | Update a policy bundle |
| DELETE | `/v1/policy-bundles/{bundle_id}` | Delete a policy bundle |
| PUT | `/v1/policy-bundles/{bundle_id}/active` | Set a policy bundle as active/inactive |

These routes are experimental/internal governance admin surface. They are **not**
v1 production-supported. Policy bundle evaluation is used by `POST /v1/proposals/{proposal_id}/evaluate`
(which **is** in the v1 contract), but the bundle management surface is not.

#### Bridge list/tools (post-v1 U4 scope)

| Method | Route | Description |
|---|---|---|
| GET | `/v1/bridges` | List registered runtime bridges |
| GET | `/v1/bridges/{bridge_id}/tools` | List tools available through a bridge |

These routes are post-v1 (U4 — MCP/local/NemoClaw runtime integrations) and are
**not** v1 production-supported. Bridge registration, tool discovery, and event
submission are U4 scope.

### 2.5 Upgrade tracks

- U1 — Outcome-aware Governance.
- U2 — Reversible Execution Planner.
- U3 — Cross-runtime Provenance Fabric.
- U4 — MCP/local/NemoClaw runtime integrations.

---

## 3. Known Limitations

These limitations are intrinsic to the v1 single-node design and are not
expected to be resolved in v1:

### 3.1 healthz and readyz are shallow; readyz/deep provides bounded store probe

`GET /v1/healthz` and `GET /v1/readyz` confirm the server process is alive
and the HTTP endpoint is reachable. They do **not** validate that the store,
migrations, or governance loop are fully functional.

`GET /v1/readyz/deep` provides a bounded deep readiness probe that verifies
the SQLite store is reachable via a `SELECT 1` query. It returns HTTP 200
when the store is healthy and HTTP 503 when the store check fails. This is
an additive opt-in endpoint; existing `healthz` and `readyz` behavior is
preserved. A full functional probe (e.g., `ferrumctl server inspect-execution`
or `GET /v1/approvals`) is still recommended after startup to confirm
end-to-end readiness.

### 3.2 Compensate may be noop-backed

`POST /v1/executions/{execution_id}/compensate` is the only provided recovery
endpoint in v1. Depending on the adapter implementation and rollback class
(R0/R1/R2/R3), compensate() may return HTTP 200 without performing any
external undo action. Always verify resource state manually after compensate.

### 3.3 Backup and restore are bounded SQLite procedures

FerrumGate provides bounded offline/local SQLite backup commands via
`ferrumctl backup create`, `ferrumctl backup verify`, and
`ferrumctl backup restore --confirm`. Restore requires the server to be
stopped and remains a full-store replacement. There is no incremental backup,
no automated scheduling, and opt-in retention pruning (`--retention-days N`).
Full retention policy management (scheduling, offsite, encryption) remains
operator-owned.

### 3.4 Restore causes data loss after backup timestamp

Restoring from a backup overwrites the entire store. Any executions,
intents, approvals, or provenance events created after the backup
timestamp are permanently lost. There is no incremental restore in v1.

### 3.5 CLI is inspect-only

`ferrumctl` provides read/inspect commands only. There are no mutating
commands (e.g., no `ferrumctl intent create`, no `ferrumctl capability revoke`
via CLI). Mutating operations must be performed via the REST API.

---

## 4. Accepted Risks

These risks are acknowledged based on current implementation and test evidence.
They are documented in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
Weak Spots 1-4 and the runbook.

### 4.1 prepare-step rollback-class handling

The gateway prepare handler loads `rollback_class` from the proposal's
`requested_rollback_class` field (`server.rs:856-872`). The R3
`auto_commit = false` control is correctly applied at prepare. The
intent creation service (caller) is still responsible for setting the
correct `rollback_class` at intent creation time.

### 4.2 Single-use capability enforced at authorize

The `mark_capability_used_durable` helper (`server.rs:685-732`) is called
by the authorize path (`server.rs:751-757`), persisting the Used status
to store. The in-memory service and store are checked for AlreadyUsed,
Revoked, or Expired status before proceeding. First authorize succeeds;
subsequent attempts return `AlreadyUsed`.

### 4.3 Draft-only revalidated at prepare

The prepare handler (`server.rs:874-898`) looks up the intent and
rejects with HTTP 403 if `approval_mode == DraftOnly`. This prevents a
draft-only intent from bypassing evaluate and reaching prepare.

### 4.4 Provenance completeness asserted by integration test

The `test_lineage_chain_full_provenance_events` integration test
(`integration_lineage_chain.rs:643-945`) exercises the full
authorize → prepare → execute → verify chain and asserts all 6 event
kinds appear in the lineage query response, linked to the execution
id. This provides end-to-end coverage for event emission completeness.

### 4.5 Restore causes data loss after backup timestamp

Any state created after a backup's timestamp is lost when restoring.
There is no incremental or point-in-time restore in v1. **Mitigation**:
Operator must coordinate backup scheduling with acceptable recovery
point objective (RPO).

---

## 5. Verification References

Evidence that the above scope is implemented and tested:

| Check | Evidence |
|---|---|
| Workspace compiles | `docs/artifacts/2026-03-30/01-cargo-check.txt` |
| fmt pass | `docs/artifacts/2026-03-30/02-cargo-fmt.txt` |
| clippy pass | `docs/artifacts/2026-03-30/03-cargo-clippy.txt` |
| cargo test pass | `docs/artifacts/2026-03-30/04-cargo-test.txt` |
| Contract consistency | `docs/artifacts/2026-03-30/05-contract-consistency.txt` |
| RC evidence script | `docs/artifacts/2026-03-30/07-rc-evidence-script.txt` |
| Scope-mismatch deny | `crates/ferrum-pdp/src/engine.rs:31-46`; `16-release-checklist.md:18` |
| Single-use capability | `crates/ferrum-cap/src/service.rs:101-122`; `16-release-checklist.md:19` |
| R3 no auto-commit | `crates/ferrum-rollback/src/service.rs:93-112`; `16-release-checklist.md:20` |
| Compensate end-to-end | `integration_gateway_flow.rs:compensate_execution_flow`; `16-release-checklist.md:23` |
| Provenance endpoint shape | `integration_lineage_chain.rs`; `16-release-checklist.md:26` |
| Approvals pagination/filter | `integration_gateway_flow.rs`; `16-release-checklist.md:24-25` |

Source docs:
- `docs/ferrumgate-roadmap-v1/00-project-canon.md` — project scope and hard rules
- `docs/ferrumgate-roadmap-v1/18-single-node-operations-runbook.md` — operator guide
- `docs/ferrumgate-roadmap-v1/14-api-and-contracts-map.md` — API endpoint reference
- `docs/implementation-path/23-production-readiness-assessment.md` — RC verdict
- `docs/implementation-path/25-EV-v1-single-node-rc-evidence.md` — evidence record
- `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` — weak spots
- `docs/implementation-path/11-remaining-tasks.md` — post-v1 backlog

---

## 6. Change Control

This document is the canonical support contract for FerrumGate v1 single-node.
Any doc that describes FerrumGate v1 single-node support scope should link to
this document rather than restating the support boundaries. Changes to
supported routes, known limitations, or accepted risks must be reflected
here first and then propagated to the linked docs.
