# 32 — Feature Completeness Audit: Route / API Reconciliation

> **Trạng thái**: Hoàn thành — 2026-04-28
> **Mục đích**: reconcile actual gateway routes với v1 support contract và API roadmap boundaries
> **Phạm vi**: single-node SQLite, v1 only, no production-ready claim

---

## 1. Mục đích

Tài liệu này reconcile các route thực tế trong `server.rs` với:
- v1 single-node support contract (`19-v1-single-node-support-contract.md`)
- API roadmap (`ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/04-api-roadmap.md`)
- API contracts map (`14-api-and-contracts-map.md`)

**Nguyên tắc quan trọng**: Implemented route ≠ v1-supported route. Các route được triển khai trong `server.rs` không tự động nằm trong v1 support contract. Chỉ các route được liệt kê rõ ràng trong `19-v1-single-node-support-contract.md` mới thuộc về v1 scope.

---

## 2. Reconciliation Table

### 2.1 Actual Routes in `server.rs` (as of 2026-04-28)

| Method | Route | Handler | v1-supported | Notes |
|--------|-------|---------|-------------|-------|
| GET | `/v1/healthz` | `healthz` | ✅ Yes | Unauthenticated |
| GET | `/v1/readyz` | `readyz` | ✅ Yes | Unauthenticated |
| GET | `/v1/readyz/deep` | `readyz_deep` | ✅ Yes | Unauthenticated; store probe |
| POST | `/v1/proposals/{proposal_id}/evaluate` | `evaluate_proposal` | ✅ Yes | PDP + policy bundle evaluation |
| POST | `/v1/intents/compile` | `compile_intent` | ❌ No | Not in v1 contract; U1 scope |
| POST | `/v1/capabilities/mint` | `mint_capability` | ✅ Yes | |
| POST | `/v1/capabilities/{capability_id}/revoke` | `revoke_capability` | ❌ No | Not in v1 contract; post-v1 |
| POST | `/v1/executions/authorize` | `authorize_execution` | ✅ Yes | |
| GET | `/v1/executions/{execution_id}` | `get_execution` | ✅ Yes | |
| POST | `/v1/executions/{execution_id}/prepare` | `prepare_execution` | ✅ Yes | |
| POST | `/v1/executions/{execution_id}/execute` | `execute_execution` | ❌ No | Not in v1 contract; fs-first slice only |
| POST | `/v1/executions/{execution_id}/verify` | `verify_execution` | ❌ No | Not in v1 contract; fs-first slice only |
| POST | `/v1/executions/{execution_id}/cancel` | `cancel_execution` | ❌ No | Implemented for ferrumctl compatibility; post-v1 support boundary |
| POST | `/v1/executions/{execution_id}/compensate` | `compensate_execution` | ✅ Yes | May be noop-backed |
| POST | `/v1/executions/{execution_id}/evaluate-outcome` | `evaluate_outcome` | ❌ No | Not in v1 contract; U1 scope |
| GET | `/v1/approvals` | `list_approvals` | ✅ Yes | |
| GET | `/v1/approvals/{approval_id}` | `get_approval` | ✅ Yes | |
| POST | `/v1/provenance/query` | `query_provenance` | ✅ Yes | Not in contract doc but implemented |
| GET | `/v1/provenance/lineage/{execution_id}` | `get_execution_lineage` | ✅ Yes | |
| POST | `/v1/provenance/lineage` | `query_lineage` | ✅ Yes | Multi-hop BFS traversal |
| POST | `/v1/provenance/ingest` | `ingest_provenance` | ❌ No | Not in v1 contract; U3 scope |
| GET | `/v1/bridges` | `list_bridges` | ❌ No | Not in v1 contract; U4 scope |
| GET | `/v1/bridges/{bridge_id}/tools` | `list_bridge_tools` | ❌ No | Not in v1 contract; U4 scope |
| POST | `/v1/policy-bundles` | `create_policy_bundle` | ❌ No | Not in v1 contract; governance admin |
| GET | `/v1/policy-bundles` | `list_policy_bundles` | ❌ No | Not in v1 contract |
| GET | `/v1/policy-bundles/{bundle_id}` | `get_policy_bundle` | ❌ No | Not in v1 contract |
| PUT | `/v1/policy-bundles/{bundle_id}` | `update_policy_bundle` | ❌ No | Not in v1 contract |
| DELETE | `/v1/policy-bundles/{bundle_id}` | `delete_policy_bundle` | ❌ No | Not in v1 contract |
| PUT | `/v1/policy-bundles/{bundle_id}/active` | `set_policy_bundle_active` | ❌ No | Not in v1 contract |

**Tổng cộng**: 29 routes trong server.rs
- **v1-supported**: 14 routes
- **Not in v1 contract**: 15 routes (post-v1/experimental/internal)

### 2.2 Route Classification Summary

| Classification | Count | Routes |
|----------------|-------|--------|
| **v1-supported** (in support contract) | 14 | healthz, readyz, readyz/deep, evaluate, mint, authorize, get_execution, prepare, compensate, list_approvals, get_approval, provenance/query, lineage/{id}, lineage (POST) |
| **Implemented post-v1** (U1-U4 / feature-completeness tracks) | 9 | intents/compile, capabilities/revoke, execute, verify, cancel, evaluate-outcome, provenance/ingest, bridges/* |
| **Ops/readiness** | 3 | healthz, readyz, readyz/deep |
| **Experimental/internal** | 6 | policy-bundle CRUD + activate |
| **Not exposed in v1 router** | 2 | commit, rollback (per contract §2.3) |

> **Note 1**: `revoke_capability` is implemented in server.rs but NOT listed in the v1 support contract's supported routes table. The contract only lists `POST /v1/capabilities/mint`. This is G-32B — requires an explicit support-contract change to promote to v1 scope; do not assume inclusion.

> **Note 2**: `POST /v1/provenance/query` is implemented and is not in the v1 support contract. This is G-32A — requires an explicit support-contract change to promote to v1 scope; do not assume inclusion.

> **Decision rule**: Implemented route ≠ v1-supported route. Routes not explicitly listed in [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) remain outside the v1 support contract until a documented support-contract change is approved.

### 2.3 execute/verify Status Correction

**Previous stale statement**: Some docs implied execute/verify are internal-only or not exposed.

**Actual status**: Both `POST /v1/executions/{execution_id}/execute` and `POST /v1/executions/{execution_id}/verify` **exist as actual HTTP routes** in `server.rs` (lines 196-202). They are part of the fs-first FileWrite slice and are tested in integration tests.

**Classification**: They are **NOT** in the v1 support contract. They are implemented-post-v1 (fs-first slice), not internal-only.

Source: `crates/ferrum-gateway/src/server.rs:196-202`; `04-api-roadmap.md:110-111`

---

## 3. Gap Analysis

### 3.1 Documentation Gaps vs Actual Implementation

| Gap | Severity | Description | Decision Note |
|-----|----------|-------------|---------------|
| G-32A | Medium | `POST /v1/provenance/query` implemented but not listed in v1 contract support table | **Requires explicit support-contract change to add.** Do not assume inclusion. |
| G-32B | Low | `POST /v1/capabilities/{capability_id}/revoke` implemented but not in v1 contract | **Requires explicit support-contract change to add.** revoke is a lifecycle operation; v1 contract lists only mint. |
| G-32E | Low | `POST /v1/executions/{execution_id}/cancel` implemented for ferrumctl compatibility but not in v1 contract | Post-v1 scope; ferrumctl internal use only |
| G-32C | Low | Policy bundle endpoints fully implemented but not in v1 contract | Experimental/internal governance admin surface; not in v1 contract |
| G-32D | Info | execute/verify documented as actual routes (corrected from stale "internal-only" claim) | Post-v1 (fs-first slice); not in v1 contract |

### 3.2 v1 Contract Route Naming — Verified Consistent

The v1 contract (`19-v1-single-node-support-contract.md:32`) and `server.rs:182` both use:
```
POST /v1/proposals/{proposal_id}/evaluate
```

The path pattern is consistent. The previous cosmetic mismatch (contract listing `{server_name}` vs server's `{proposal_id}`) has been verified as resolved. Both docs (`14-api-and-contracts-map.md`, `04-api-roadmap.md`) also use `{proposal_id}` — no action required.

---

## 4. execute/verify/revoke/compile/evaluate-outcome/provenance/bridges/policy-bundles — Boundary Status

| Route | Boundary Status | Notes |
|-------|-----------------|-------|
| `POST /v1/intents/compile` | **Post-v1 (U1)** | Intent envelope creation; not in v1 contract |
| `POST /v1/capabilities/{capability_id}/revoke` | **Post-v1** | Implemented but not in v1 contract |
| `POST /v1/executions/{execution_id}/execute` | **Post-v1 (fs-first)** | Exists as HTTP route; fs-first slice only; not in v1 contract |
| `POST /v1/executions/{execution_id}/verify` | **Post-v1 (fs-first)** | Exists as HTTP route; fs-first slice only; not in v1 contract |
| `POST /v1/executions/{execution_id}/evaluate-outcome` | **Post-v1 (U1)** | Outcome-aware governance; not in v1 contract |
| `POST /v1/provenance/ingest` | **Post-v1 (U3)** | Cross-runtime provenance fabric; not in v1 contract |
| `GET /v1/bridges` | **Post-v1 (U4)** | MCP/local/NemoClaw integrations; not in v1 contract |
| `GET /v1/bridges/{bridge_id}/tools` | **Post-v1 (U4)** | MCP/local/NemoClaw integrations; not in v1 contract |
| Policy bundle CRUD + activate | **Experimental/internal** | Not in v1 contract; governance admin surface |

---

## 5. Cross-Reference Matrix

| This Doc | Links To | Purpose |
|----------|----------|---------|
| `32-feature-completeness-audit.md` | `19-v1-single-node-support-contract.md` | Canonical v1 boundary reference |
| `32-feature-completeness-audit.md` | `14-api-and-contracts-map.md` | API contracts reference |
| `32-feature-completeness-audit.md` | `04-api-roadmap.md` | Post-v1 roadmap context |
| `32-feature-completeness-audit.md` | `45-current-feature-audit.md` | Phase 1 feature matrix source |
| `32-feature-completeness-audit.md` | `33-feature-completion-backlog.md` | Incomplete/partial feature backlog |
| `32-feature-completeness-audit.md` | `31-release-paths-todo.md` | Release path context |

---

## 6. No Production Overclaim

**IMPORTANT**: FerrumGate v1 is **NOT** production-ready. This audit documents what exists vs what is contracted.

- v1 support contract covers single-node SQLite only
- PostgreSQL/multi-node/HA not in v1 scope
- execute/verify are fs-first slices, not full Q2 adapter scope
- Compensate may be noop-backed depending on adapter
- Policy bundle endpoints are experimental/internal governance admin surface

---

## 7. References

- `crates/ferrum-gateway/src/server.rs` — actual route definitions
- `19-v1-single-node-support-contract.md` — v1 support contract
- `14-api-and-contracts-map.md` — API contracts map
- `04-api-roadmap.md` — API roadmap with post-v1 boundary clarification
- `45-current-feature-audit.md` — Phase 1 feature matrix
- `31-release-paths-todo.md` — release paths

---

*Document generated: 2026-04-28. Grounded in server.rs route inspection and v1 contract review.*
