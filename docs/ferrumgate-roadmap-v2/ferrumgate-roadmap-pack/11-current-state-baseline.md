# 11 — Current-state baseline

> **⚠️ Historical / Snapshot**: This document was a P6/P7-era snapshot reference. Some content may now be stale (e.g., adapter "skeleton/partial" descriptions, test counts). For current state, see `docs/implementation-path/01-current-state.md` which is maintained with the latest phase status and test coverage matrix.
>
> **For current support scope**: Always refer to `19-v1-single-node-support-contract.md` — the only authoritative v1 boundary document.

## Purpose

This document captured facts about the current state of the FerrumGate repository
as of the time of writing. It is a **snapshot reference** for roadmap consumers
and AI agents. It does not create new commitments or change support scope.
For support scope, see `19-v1-single-node-support-contract.md` — the only
authoritative v1 boundary document.

---

## 1. Repository structure

### Workspace crates (from `Cargo.toml`)

```
bins/ferrumd            — server binary
bins/ferrumctl           — CLI binary
bins/ferrum-stress       — stress test binary
crates/ferrum-proto      — protocol types and domain objects
crates/ferrum-pdp        — policy decision point
crates/ferrum-cap        — capability service
crates/ferrum-rollback   — rollback and compensation service
crates/ferrum-gateway    — HTTP gateway and router
crates/ferrum-firewall   — trust and taint enforcement
crates/ferrum-store      — persistence layer (SQLite)
crates/ferrum-graph      — provenance and lineage query helpers
crates/ferrum-ledger    — append-only audit trail
crates/ferrum-adapter-fs       — filesystem adapter (skeleton/partial)
crates/ferrum-adapter-git      — git adapter (skeleton/partial)
crates/ferrum-adapter-sqlite   — SQLite adapter (skeleton/partial)
crates/ferrum-adapter-http     — HTTP adapter (skeleton/partial)
crates/ferrum-adapter-maildraft — maildraft adapter (skeleton/partial)
crates/ferrum-testkit    — test utilities
crates/ferrum-integration-tests — integration test suite
crates/ferrum-sync       — synchronization utilities
```

Workspace edition: **2024**
Rust edition 2024 became stable in 2024; the workspace uses it.

### Adapter crates note

All five `ferrum-adapter-*` crates exist in the workspace. Their presence
in `Cargo.toml` does **not** mean they are v1-supported. Per the v1 support
contract, all adapter implementations are out of v1 scope. The adapter
crates exist as skeleton or partial implementations.

---

## 2. Gateway router — current route table

The router is defined in `crates/ferrum-gateway/src/server.rs` (`build_router_core`).

### Routes present in the router (as of last inspection)

| Method | Route | Note |
|---|---|---|
| GET | `/v1/healthz` | In v1 support contract |
| GET | `/v1/readyz` | In v1 support contract |
| POST | `/v1/provenance/query` | In v1 support contract |
| GET | `/v1/provenance/lineage/{execution_id}` | In v1 support contract |
| POST | `/v1/provenance/lineage` | In v1 support contract |
| POST | `/v1/provenance/ingest` | **Not** in v1 support contract |
| GET | `/v1/bridges` | **Not** in v1 support contract |
| GET | `/v1/bridges/{bridge_id}/tools` | **Not** in v1 support contract |
| GET | `/v1/executions/{execution_id}` | In v1 support contract |
| GET | `/v1/approvals` | In v1 support contract |
| GET | `/v1/approvals/{approval_id}` | In v1 support contract |
| POST | `/v1/intents/compile` | **Not** in v1 support contract |
| POST | `/v1/proposals/{proposal_id}/evaluate` | In v1 support contract |
| POST | `/v1/capabilities/mint` | In v1 support contract |
| POST | `/v1/capabilities/{capability_id}/revoke` | **Not** in v1 support contract |
| POST | `/v1/executions/authorize` | In v1 support contract |
| POST | `/v1/executions/{execution_id}/prepare` | In v1 support contract |
| POST | `/v1/executions/{execution_id}/compensate` | In v1 support contract (may be noop-backed) |
| POST | `/v1/executions/{execution_id}/evaluate-outcome` | **Not** in v1 support contract |

### Routes explicitly absent from v1 router

The following routes are **not** exposed in the v1 router and are out of v1 scope:
- `POST /v1/executions/{id}/commit`
- `POST /v1/executions/{id}/rollback`

The v1 gateway flow terminates at `compensate` as the recovery endpoint.

---

## 3. CLI surface

### v1-supported CLI commands (from `19-v1-single-node-support-contract.md`)

```
ferrumctl server health                    — shallow health probe
ferrumctl server inspect-execution <id>   — fetch execution record
ferrumctl server inspect-approvals         — list approvals
ferrumctl server inspect-approval <id>    — fetch single approval
ferrumctl server inspect-lineage <id>      — fetch lineage for execution
ferrumctl server inspect-provenance --intent-id <id> — query provenance events
```

All v1 CLI commands are **inspect-only**. Mutating `ferrumctl` commands are
post-v1 scope.

---

## 4. Adapter implementations — current state

| Adapter | Status | Notes |
|---|---|---|
| `ferrum-adapter-fs` | Skeleton/partial | Out of v1 scope per support contract |
| `ferrum-adapter-git` | Skeleton/partial | Out of v1 scope per support contract |
| `ferrum-adapter-sqlite` | Skeleton/partial | Out of v1 scope per support contract |
| `ferrum-adapter-http` | Skeleton/partial | Out of v1 scope per support contract |
| `ferrum-adapter-maildraft` | Skeleton/partial | Out of v1 scope per support contract |

The existence of adapter crate code in the repo does not expand v1 scope.
The v1 support contract is the only authoritative boundary.

---

## 5. Storage — current state

- Default store: **SQLite** via `ferrum-store`
- SQLite used for governance core persistence (executions, capabilities, approvals, provenance)
- SQLite migrations are embedded in `ferrum-store`
- No built-in backup command; manual file-level backup is the only path
- No incremental backup, no automated scheduling, opt-in CLI retention pruning (`--retention-days N`)

Postgres support (via `ferrum-store` generalization or separate adapter) is
post-v1 scope.

---

## 6. Known limitations (from v1 support contract)

These are intrinsic v1 limitations, not defects to be fixed in the roadmap:

1. **healthz and readyz are shallow** — they confirm the process is alive, not that store or governance loop is functional. A functional probe is required after startup.
2. **Compensate may be noop-backed** — the compensate endpoint exists but may return HTTP 200 without performing external undo. Always verify resource state manually after compensate.
3. **Backup/restore is bounded SQLite-only** — `ferrumctl backup create/verify/restore` exists for offline/local workflows with opt-in retention pruning (`--retention-days N`). Restoring overwrites the entire store; any state created after the backup timestamp is permanently lost. There is no built-in scheduling, encryption, or incremental backup.
4. **CLI is inspect-only** — no mutating CLI commands (no intent create, no capability revoke via CLI).
5. **Accepted risks — status as of Q1/v1.1 gate (2026-04-09)** (from v1 support contract weak spots 1–4):
   - WS1 (prepare-step rollback_class bypass): addressed — rollback_class now sourced from proposal at prepare; adversarial regression test confirms `auto_commit=false` propagation — evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
   - WS2 (single-use capability reuse): addressed — `mark_used` called at authorize; adversarial regression test confirms authorize can only be called once — evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
   - WS3 (draft-only bypass): addressed — draft-only revalidated at prepare/evaluate gateway path; adversarial regression test confirms draft-only intent cannot reach prepare via bypass — evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
   - WS4 (lineage partial flow): addressed — full provenance minimum-chain integration test confirms terminal-path events emitted on existing surface; adversarial regression test confirms lineage adversarial partial execution — evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
   All four weak spots are addressed for Q1/v1.1 gate scope. The v1 support contract remains the authoritative boundary.

---

## 7. Upgrade tracks — explicit out-of-v1-scope items

The v1 support contract explicitly lists these as **not supported** in v1:

| Track | Description |
|---|---|
| U1 | Outcome-aware Governance |
| U2 | Reversible Execution Planner |
| U3 | Cross-runtime Provenance Fabric |
| U4 | MCP/local/NemoClaw runtime integrations |

These are not v1 defects; they are future work tracks.

---

## 8. Repo facts relevant to roadmap use

- **No multi-node in v1** — HA, read-replica, multi-node are out of v1 scope per support contract
- **No operator UI in v1** — operator UI is post-v1 scope
- **No postgres in v1** — postgres support is Q3 post-v1 scope
- **No commit/rollback routes** — v1 router terminates at compensate; commit/rollback routes not exposed
- **All adapters are skeleton/partial** — real adapter implementations are post-v1 scope; compensate may be noop-backed
- **Mutating CLI commands are post-v1** — all `ferrumctl` mutating operations are out of v1 scope
- **The repo may contain code beyond the v1 support baseline** — adapter crate shapes, non-v1 routes, CLI commands marked post-v1. This code does not expand the v1 support contract.

---

## 9. Using this baseline with the roadmap

When reading `01-quarterly-plan.md` through `10-master-checklist.md`:

- Every item labeled "Q1" relates to v1 kernel hardening — **Q1 exit gate (Q1-P7/v1.1) is passed as of 2026-04-09** per `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`; all four weak spots (WS1–WS4) are addressed or risk-accepted within the existing v1 support contract, not a scope expansion
- Every item labeled "Q2" through "Q4" is **post-v1 scope**
- Every adapter item is **post-v1 scope** regardless of whether adapter code exists in the repo
- Every deployment/storage item that mentions postgres, multi-node, HA, or operator UI is **post-v1 scope**
- For any claim about what "exists" in the codebase, this document and `19-v1-single-node-support-contract.md` are the references — not the roadmap plan docs

This baseline document is a **facts snapshot** at time of writing. It does not
update in real time. For current support scope, always refer to
`19-v1-single-node-support-contract.md`.
