# 19 — v1 Single-Node Support Contract

FerrumGate v1 single-node production support contract. This document is the
canonical reference for what is and is not supported in the FerrumGate v1
single-node release. All other docs should link here rather than restating
support scope.

**Scope**: single-node, SQLite-backed, v1 only.
**Last updated**: 2026-04-03.

---

## 0. Support Tier Summary — Production Boundary at a Glance

This table is the compact published support matrix for FerrumGate v1 single-node.
It defines what is fully supported, partially supported, and out-of-scope.

| Tier | Category | Status | Notes |
|------|----------|--------|-------|
| **T1 — Supported** | Single-node governance core + SQLite persistence | ✅ SUPPORTED | Core loop, evaluate/mint/authorize/prepare/execute/verify/compensate |
| **T1 — Supported** | Defined REST route surface (Section 1.2) | ✅ SUPPORTED | All listed endpoints; commit route exposed but optional for single-node |
| **T1 — Supported** | `ferrumctl` operator surface (Section 1.3) | ✅ SUPPORTED | High-use read/control flows CLI-covered |
| **T1 — Supported** | Provenance / lineage / approvals | ✅ SUPPORTED | Full pagination and filter support |
| **T2 — Partial** | Adapter-backed integrations | ⚠️ PARTIAL | Bounded local implementations only; broader hardening post-v1 |
| **T2 — Partial** | U1 core capability | ⚠️ PARTIAL | Materially mature for current scope; richer expressiveness post-v1 |
| **T3 — Out of Scope** | Multi-node / HA / read-replica | ❌ NOT SUPPORTED | Post-v1 backlog |
| **T3 — Out of Scope** | Upgrade tracks U2 / U3 / U4 | ❌ NOT SUPPORTED | Post-v1 backlog |
| **T3 — Out of Scope** | SLA-backed availability guarantees | ❌ NOT SUPPORTED | No HA; RPO owned by operator |
| **T3 — Out of Scope** | Automated backup / restore | ❌ NOT SUPPORTED | Manual SQLite file-level only |

**Bottom line**: FerrumGate v1 single-node is RC-ready. It is production-supported for single-node SQLite-backed deployments with the T1 surface. It is NOT production-ready for multi-node, HA, or broadly-hardened adapter integrations.

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
| POST | /v1/proposals/{proposal_id}/evaluate | Evaluate proposal via policy |
| POST | /v1/capabilities/mint | Mint a capability lease |
| GET | /v1/capabilities/{capability_id} | Inspect a capability lease |
| POST | /v1/executions/authorize | Authorize execution |
| POST | /v1/executions/{execution_id}/prepare | Prepare rollback/preflight |
| POST | /v1/executions/{execution_id}/execute | Execute the prepared operation |
| POST | /v1/executions/{execution_id}/verify | Verify execution result against intent and policy |
| POST | /v1/executions/{execution_id}/cancel | Cancel execution in pre-execute state (Proposed, Authorized, Prepared) |
| POST | /v1/executions/{execution_id}/pause | Pause execution in running state (Running, AwaitingVerification) |
| POST | /v1/executions/{execution_id}/resume | Resume paused execution |
| POST | /v1/executions/{execution_id}/compensate | Compensate execution (may be noop-backed) |
| POST | /v1/executions/{execution_id}/rollback | Rollback/compensate via rollback contract |
| GET | /v1/executions/{execution_id} | Inspect execution record |
| GET | /v1/approvals | List pending approvals (pagination, filter by proposal_id) |
| GET | /v1/approvals/{approval_id} | Get specific approval |
| POST | /v1/approvals/{approval_id}/resolve | Resolve a pending approval (approve or deny) |
| GET | /v1/provenance/lineage/{execution_id} | Get lineage for execution |
| POST | /v1/provenance/lineage | Multi-hop lineage query (ancestors/descendants/both, bounded depth) |
| POST | /v1/provenance/query | Query provenance events (intent_id, execution_id, capability_id, event_kind, time range) |

### 1.3 CLI surface

The following `ferrumctl` commands are in the v1 single-node support contract:

**Read-only:**
- `ferrumctl server health` — shallow health probe
- `ferrumctl server ready` — shallow readiness probe
- `ferrumctl server inspect-capability <capability_id>` — fetch a capability record
- `ferrumctl server inspect-execution <execution_id>` — fetch execution record
- `ferrumctl server inspect-approvals` — list approvals with CLI pagination/filtering (`--limit`, `--cursor`, `--proposal-id`, `--execution-id`)
- `ferrumctl server inspect-approval <approval_id>` — fetch single approval
- `ferrumctl server inspect-lineage <execution_id>` — fetch lineage for execution
- `ferrumctl server inspect-provenance` — query provenance events with CLI filters for intent/proposal/execution/capability/event kind/time range, plus pagination and export-all support
- `ferrumctl server watch-execution <execution_id>` — bounded polling for execution terminal state
- `ferrumctl server watch-approvals` — bounded polling for approval changes
- `ferrumctl server inspect-lineage-query` — multi-hop lineage query via `--ancestry`/`--descendants` flags

**Mutating:**
- `ferrumctl server revoke-capability <capability_id>` — revoke a capability
- `ferrumctl server resolve-approval <approval_id> --approve|--deny` — resolve a pending approval
- `ferrumctl server cancel-execution <execution_id>` — cancel an execution in pre-execute state
- `ferrumctl server pause-execution <execution_id>` — pause an execution in running state
- `ferrumctl server resume-execution <execution_id>` — resume a paused execution
- `ferrumctl server prepare-execution <execution_id>` — prepare an execution for execution
- `ferrumctl server execute-execution <execution_id>` — execute a prepared execution
- `ferrumctl server compensate-execution <execution_id>` — trigger compensation on an execution
- `ferrumctl server rollback-execution <execution_id>` — trigger rollback on an execution

---

## 2. Not Supported

The following are explicitly out of scope for FerrumGate v1 single-node:

### 2.1 Deployment

- Multi-node deployments of any kind.
- High-availability (HA) configurations.
- Read-replica configurations.
- Any deployment model other than single-node SQLite.

### 2.2 Adapter-backed integrations

- `ferrum-adapter-fs`, `ferrum-adapter-sqlite`, `ferrum-adapter-maildraft`, `ferrum-adapter-git`, `ferrum-adapter-http` have bounded local implementations.
  Broader production hardening, remote/external integration depth, and verified side-effect undo are post-v1.
- Guaranteed external undo via adapter. Compensate may be noop-backed depending on adapter and rollback class.

### 2.3 Routes with different visibility

- `POST /v1/executions/{id}/commit` — **exposed** in the v1 router and OpenAPI spec (`server.rs:145`).
  Commit finalizes a verified execution. The gateway flow uses compensate as the primary recovery endpoint; commit is available but typically not needed for single-node operation.

### 2.4 Upgrade tracks

- U1 — Outcome-aware Governance: materially mature for current single-node scope; remaining post-v1 backlog includes richer outcome clause expressiveness and operator ergonomics/authoring tooling.
- U2 — Reversible Execution Planner.
- U3 — Cross-runtime Provenance Fabric.
- U4 — MCP/local/NemoClaw runtime integrations.

---

## 3. Known Limitations

These limitations are intrinsic to the v1 single-node design and are not
expected to be resolved in v1:

### 3.1 healthz and readyz are shallow

`GET /v1/healthz` and `GET /v1/readyz` confirm the server process is alive
and the HTTP endpoint is reachable. They do **not** validate that the store,
migrations, or governance loop are fully functional. A functional probe
(e.g., `ferrumctl server inspect-execution` or `GET /v1/approvals`) is
required after startup to confirm end-to-end readiness.

### 3.2 Compensate may be noop-backed

`POST /v1/executions/{execution_id}/compensate` is the only provided recovery
endpoint in v1. Depending on the adapter implementation and rollback class
(R0/R1/R2/R3), compensate() may return HTTP 200 without performing any
external undo action. Always verify resource state manually after compensate.

### 3.3 Backup and restore are manual SQLite procedures

There is no built-in backup command. The operator must perform manual
file-level backup by copying the SQLite database file. There is no
incremental backup, no automated scheduling, and no backup retention
policy built into FerrumGate.

**RPO ownership**: RPO is owned entirely by the operator. The operator
must define an acceptable data-loss window, schedule manual backups
at intervals consistent with that RPO, and periodically verify
restore capability through drills. Support cannot backstop an RPO
the operator has not defined or enforced.

**Recommended minimum cadence**: one backup every 24 hours, plus a
backup before any major operator-initiated action, plus a backup after
any unplanned outage. Adjust based on operational risk tolerance.
Retain at least three daily backups and one weekly backup.

### 3.4 Restore causes data loss after backup timestamp

Restoring from a backup overwrites the entire store. Any executions,
intents, approvals, or provenance events created after the backup
timestamp are permanently lost. There is no incremental restore in v1.

### 3.5 Mutating CLI commands exist alongside REST API

`ferrumctl` provides both read/inspect commands and mutating execution-control
commands. All mutating operations are also available via the REST API. The CLI
wrappers (cancel-execution, pause-execution, resume-execution, prepare-execution,
execute-execution, compensate-execution, rollback-execution, resolve-approval)
invoke the same underlying gateway endpoints as their REST counterparts.

---

## 4. Accepted Risks

These risks are acknowledged based on current implementation and test evidence.
They are documented in `26-v1-single-node-invariant-control-test-evidence-matrix.md`
and the runbook.

### 4.1 Restore causes data loss after backup timestamp

Any state created after a backup's timestamp is lost when restoring.
There is no incremental or point-in-time restore in v1. **Mitigation**:
Operator must coordinate backup scheduling with acceptable recovery
point objective (RPO).

---

## 5. Verification References

Evidence that the above scope is implemented and tested:

| Check | Evidence |
|---|---|
| Workspace compiles | `cargo check --workspace` PASS (2026-04-02) |
| fmt pass | `cargo fmt --all -- --check` PASS (2026-04-02) |
| clippy pass | `cargo clippy --workspace -- -D warnings` PASS (2026-04-02). Historical pass: `docs/artifacts/2026-03-30/03-cargo-clippy.txt` |
| cargo test pass | `cargo test --workspace` PASS (2026-04-02). Historical pass: `docs/artifacts/2026-03-30/04-cargo-test.txt` |
| Contract consistency | `python3 scripts/check_contract_consistency.py` PASS (2026-04-02) |
| RC evidence script | `python3 scripts/generate_rc_evidence.py` verdict: ALL GATES PASSED (2026-04-02). Historical pass: `docs/artifacts/2026-03-30/07-rc-evidence-script.txt` |
| Scope-mismatch deny | `crates/ferrum-pdp/src/engine.rs:31-46`; `16-release-checklist.md:18` |
| Single-use capability | `crates/ferrum-cap/src/service.rs:101-122`; `16-release-checklist.md:19` |
| R3 no auto-commit | `crates/ferrum-rollback/src/service.rs:93-112`; `16-release-checklist.md:20` |
| Compensate end-to-end | `integration_gateway_flow.rs:compensate_execution_flow`; `16-release-checklist.md:23` |
| Provenance endpoint shape | `integration_lineage_chain.rs`; `16-release-checklist.md:26` |
| Approvals pagination/filter | `integration_gateway_flow.rs`; `16-release-checklist.md:24-25` |

Source docs:
- `docs/00-project-canon.md` — project scope and hard rules
- `docs/18-single-node-operations-runbook.md` — operator guide
- `docs/14-api-and-contracts-map.md` — API endpoint reference
- `docs/implementation-path/23-production-readiness-assessment.md` — RC verdict
- `docs/implementation-path/25-v1-single-node-rc-evidence.md` — evidence record
- `docs/implementation-path/26-v1-single-node-invariant-control-test-evidence-matrix.md` — weak spots
- `docs/implementation-path/11-remaining-tasks.md` — post-v1 backlog

---

## 7. SLA Surface

This section defines the availability, recovery, and response boundaries for
FerrumGate v1 single-node, distinguishing operator-owned obligations from
FerrumGate-supported behavior. It is conservative: it makes no promise that
is not backed by current implementation or explicit contract language.

### 7.1 Availability

| Boundary | Owner | Detail |
|---|---|---|
| Process uptime | Operator | No built-in process supervisor or auto-restart. Operator must provide external supervision (systemd, container restart policy, etc.). |
| Node availability | Operator | Single-node only. No HA, no multi-node failover. If the node goes down, the operator must restart it manually. |
| Network reachability | Operator | Operator controls network exposure, firewall, and bind address. |
| Health/readiness probes | FerrumGate | `GET /v1/healthz` and `GET /v1/readyz` return 200 when the HTTP server is reachable. These are shallow checks; they do not validate store or governance loop state. |
| Functional readiness | Operator | After startup, operator must run a functional probe (e.g., `GET /v1/approvals?limit=1`) to confirm end-to-end readiness. Shallow probes alone are insufficient. |

**What is not guaranteed:**
- No uptime SLO or availability percentage commitment for FerrumGate v1 single-node.
- No HA, no automatic failover, no read-replica.
- healthz/readyz do not guarantee store connectivity or governance loop health.

### 7.2 Recovery

| Boundary | Owner | Detail |
|---|---|---|
| Recovery Point Objective (RPO) | **Operator** | RPO is owned entirely by the operator. FerrumGate provides no automatic backup, no incremental backup, no backup scheduler, and no point-in-time restore. The operator must define a data-loss tolerance, schedule manual SQLite file backups at intervals consistent with that tolerance, and periodically verify restore capability. |
| Recovery Time Objective (RTO) | **Operator** | RTO is determined by operator's backup cadence, restore drill frequency, and manual restore procedure execution time. FerrumGate provides no automated recovery path. |
| Compensate endpoint | FerrumGate | `POST /v1/executions/{id}/compensate` is the primary recovery endpoint in v1. It returns HTTP 200 on success; however, depending on the adapter and rollback class (R0/R1/R2/R3), compensate may be a no-op. Always verify external resource state after compensate. |
| Rollback contract | FerrumGate | Rollback contracts (R0/R1/R2/R3) with `auto_commit=false` for R3 are enforced. Compensate triggers the adapter's rollback handler if one is registered. |
| Manual restore | **Operator** | Restore is a manual SQLite file-level copy. Restoring overwrites the entire store; any state created after the backup timestamp is permanently lost. |
| Restore drill | **Operator** | Operator must periodically verify (at minimum quarterly and after any backup infrastructure change) that backups are restorable. See runbook Section 6.4. |

**What is not guaranteed:**
- Compensate is not guaranteed to produce an external undo action. It may return 200 with no observable side effect depending on adapter implementation.
- No automated backup, no incremental restore, no built-in backup retention policy.
- No RTO commitment — recovery time depends entirely on operator-defined backup cadence and manual restore speed.

### 7.3 Response

| Boundary | Owner | Detail |
|---|---|---|
| Bug response | FerrumGate | FerrumGate supports the T1 surface (Section 1). Bugs in implemented behavior will be addressed per the support contract. |
| Scope boundary | Operator | Issues arising from deployment outside the supported scope (multi-node, HA, non-SQLite stores, adapter configurations beyond bounded local implementations) are operator-owned. |
| Adapter external side effects | **Operator** | Adapters (fs, sqlite, git, http, maildraft) have bounded local implementations in v1. Broader production hardening, remote/external integration, and verified external undo are post-v1 backlog. The operator is responsible for verifying adapter behavior in their target environment. |
| Upgrade path | FerrumGate | Upgrade tracks U2/U3/U4 are post-v1. No in-place upgrade mechanism exists in v1. |

**What is not guaranteed:**
- No response-time SLO for issue resolution.
- No advisory SLA for multi-node or HA configurations (they are out of scope for v1).
- No guaranteed external undo via adapter (compensate may be noop-backed; external undo verification is operator responsibility).

### 7.4 Summary

| Category | FerrumGate supports | Operator owns |
|---|---|---|
| Availability | HTTP probe endpoints (shallow); single-node process lifecycle | Node supervision, process restart, failover, network |
| Recovery | Compensate endpoint (best-effort; may be noop); rollback contracts | RPO definition, manual backup cadence, restore procedure, restore drills |
| Response | T1 surface bug response; scope boundary enforcement | Adapter behavior in target environment, upgrade paths, backup integrity verification |

---

## 8. Change Control

This document is the canonical support contract for FerrumGate v1 single-node.
Any doc that describes FerrumGate v1 single-node support scope should link to
this document rather than restating the support boundaries. Changes to
supported routes, known limitations, or accepted risks must be reflected
here first and then propagated to the linked docs.

---

## 9. EOL / Deprecation Policy

This section defines how the v1 single-node support contract is deprecated,
how supported surface (routes, CLI commands) is retired, and how changes to
the support boundary are announced. It is intentionally conservative: no
support window duration or version schedule is implied beyond what is
explicitly stated elsewhere in this document.

### 9.1 Scope

This policy applies to:
- **Supported routes** listed in Section 1.2
- **Supported CLI commands** listed in Section 1.3
- **Deployment model** (single-node, SQLite-backed, v1)
- **Support tier assignments** (T1/T2/T3) and their boundaries

This policy does **not** apply to multi-node, HA, or post-v1 upgrade tracks;
those are out-of-scope for v1 and have no support commitment.

### 9.2 Change classification

| Class | Description | Examples |
|-------|-------------|----------|
| **Material scope change** | Removes or reclassifies a previously supported route, CLI command, or deployment model; tightens T1/T2 boundary in a way that may break existing workflows | Removing a supported route from the contract; changing single-node to multi-node-only; promoting a T2 adapter to T1 |
| **Clarifying change** | Fixes typos, updates evidence links, refines descriptions without changing supported surface or operator obligations | Correcting a CLI command description; adding a missing pagination flag to a CLI command; updating evidence file paths |

### 9.3 Deprecation announcement process (material scope changes)

For any material scope change:

1. **Advance notice**: A deprecation notice is published in the project's
   release notes or equivalent public record **before** the change takes
   effect. The notice describes what is changing, why, and the effective date.
2. **Minimum notice period**: The change does not take effect sooner than
   **30 days** after the deprecation notice is published. This gives operators
   time to assess impact and plan migration.
3. **Contract update**: The support contract (this document) is updated to
   reflect the deprecation at the time of announcement, with the effective
   date noted. The deprecated item remains in the contract with a
   `DEPRECATED` marker until the effective date.
4. **No automatic migration**: FerrumGate does not provide automated migration
   tooling for deprecated surface. Operators are responsible for their own
   migration planning.

**What this policy does not require:**
- A fixed support window (e.g., "12 months of support after deprecation").
  Such commitments are not made in this document and would require a separate
  explicit agreement.
- A semantic-versioning policy. No semantic-versioning policy is defined for
  FerrumGate v1 in this document.

### 9.4 Route and CLI removal

A supported route or CLI command is **never removed** without:
1. First being marked `DEPRECATED` in this document with an effective date.
2. The deprecation notice process in Section 9.3 being followed.

After the effective date, the deprecated route or CLI command may be removed
from the implementation without further notice.

### 9.5 Supported-surface additions

New routes, CLI commands, or deployment models may be added to the support
contract at any time. Additions do not require a deprecation period but do
require this document to be updated first (per Section 8).

### 9.6 EOL of v1 single-node support

"FerrumGate v1 single-node end-of-life" means the point at which all support
for the v1 single-node scope (Section 0) ceases. This document does **not**
define an EOL date for v1 single-node. An EOL date, if set, will be
announced with the same process as a material scope change (Section 9.3).

Until an EOL date is formally announced, the v1 single-node support contract
remains in effect. Operators should monitor release notes for announcements.

### 9.7 Relationship to upgrade tracks

U2, U3, U4, and multi-node topologies are out-of-scope for v1 single-node
support (Section 2.4). The availability of a successor version or upgrade
track does not, by itself, constitute EOL of v1 single-node. An explicit
EOL announcement is required to retire v1 single-node support.
