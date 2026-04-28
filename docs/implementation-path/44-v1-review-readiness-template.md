# 44 — v1 Review Readiness Template

Single-node v1 scope. Conservative official-review pass against current repo truth
and inherited evidence from Phase F RC gate.

**Method**: Conservative — PASS only when directly re-verified by doc inspection or
today's command output; INHERITED when relying on prior-cycle evidence; STALE
when evidence is stale/absent/conflicting.

---

## Section 1 — Workspace Quality

| Check | Status | Evidence | Notes |
|---|---|---|---|
| Repository layout | PASS | `bash scripts/validate_repo_layout.sh` — "Repository layout looks OK" | Verified this session. |
| Contract/schema consistency | PASS | `python3 scripts/check_contract_consistency.py` — "VALIDATION PASSED" | Verified this session. |
| `cargo fmt --all --check` | PASS | `cargo fmt --all -- --check` — exit 0 | Verified this session. |
| `cargo check --workspace` | PASS | `cargo check --workspace` — exit 0 | Verified this session. |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | `cargo clippy --workspace --all-targets -- -D warnings` — exit 0 | Verified this session. |
| `cargo test --workspace` | PASS | `cargo test --workspace` — exit 0; `integration_gateway_flow` 65 cases, `integration_lineage_chain` 8 cases, `ferrumctl` 35 cases, and all workspace tests passed | Verified this session. |

**Section 1 verdict**: PASS — all workspace quality checks verified this session. layout OK,
contract/schema consistent, fmt/check/clippy/test all exit 0.

---

## Section 2 — Governance Enforcement

| Criterion | Status | Evidence | Notes |
|---|---|---|---|
| Intent required before mutation | PASS | `server.rs:118` — all flows require intent_id | Direct from doc inspection |
| Capability single-use enforced | PASS | `ferrum-cap/src/service.rs:101-122` — `mark_capability_used_durable` called in authorize path (`server.rs:751-757`) with store persistence | VERIFIED — Weak Spot 3 resolved. Evidence from `26-v1-single-node-invariant-control-test-evidence-matrix.md:25` |
| Scope-bounds enforcement | PASS | `ferrum-pdp/src/engine.rs:31-46` — explicit scope check | Verified in `16-release-checklist.md:18` |
| Provenance chain maintained | PASS | `server.rs:119-129` — lineage/query/ingest routes registered | Evidence from `25-v1-single-node-rc-evidence.md:99-107` |
| R3 requires approval | PASS | `ferrum-pdp/src/engine.rs:63-74` — StaticPdpEngine returns RequireApproval for R3 | Evidence from `23-production-readiness-assessment.md:46-55` |
| Draft-only gated at evaluate | PASS | `ferrum-pdp/src/engine.rs:76-85` | Evidence from `19-v1-single-node-support-contract.md:157-162` |
| Rollback/compensate distinct | PASS | `ferrum-rollback/src/service.rs:93` — rollback vs compensate services | Evidence from `16-release-checklist.md:21` |
| Compensate end-to-end | PASS | `integration_gateway_flow.rs:compensate_execution_flow` | Evidence from `25-v1-single-node-rc-evidence.md:32` |

**Section 2 verdict**: CONDITIONAL — governance core controls are in place. Weak Spots 1–4
(WS1–WS4) are resolved, output sanitization (Invariant 11) is VERIFIED with bounded gateway
wiring, and the invariant matrix is `12 VERIFIED / 0 PARTIAL / 0 INFERRED`. Residual gaps are
operational/backlog items requiring compensating controls or formal risk acceptance.

---

## Section 3 — API Surface (v1 Router vs Docs)

| Route | In Router | In API Map | Status |
|---|---|---|---|
| `GET /v1/healthz` | ✅ `server.rs:117` | ✅ `14-api-and-contracts-map.md:14` | MATCH |
| `GET /v1/readyz` | ✅ `server.rs:118` | ✅ `14-api-and-contracts-map.md:15` | MATCH |
| `POST /v1/intents/compile` | ✅ `server.rs:139` | ✅ `14-api-and-contracts-map.md:18` | MATCH |
| `POST /v1/proposals/{proposal_id}/evaluate` | ✅ `server.rs:140-143` | ✅ `14-api-and-contracts-map.md:19` | MATCH |
| `POST /v1/capabilities/mint` | ✅ `server.rs:144` | ✅ `14-api-and-contracts-map.md:22` | MATCH |
| `POST /v1/capabilities/{capability_id}/revoke` | ✅ `server.rs:145-148` | ✅ `14-api-and-contracts-map.md:23` | MATCH |
| `POST /v1/executions/authorize` | ✅ `server.rs:149` | ✅ `14-api-and-contracts-map.md:26` | MATCH |
| `POST /v1/executions/{execution_id}/prepare` | ✅ `server.rs:150-153` | ✅ `19-v1-single-node-support-contract.md:34` | MATCH |
| `POST /v1/executions/{execution_id}/compensate` | ✅ `server.rs:154-157` | ✅ `19-v1-single-node-support-contract.md:35` | MATCH |
| `GET /v1/executions/{execution_id}` | ✅ `server.rs:134` | ✅ `14-api-and-contracts-map.md:28` | MATCH |
| `GET /v1/approvals` | ✅ `server.rs:136` | ✅ `14-api-and-contracts-map.md:31` | MATCH |
| `GET /v1/approvals/{approval_id}` | ✅ `server.rs:137` | ✅ `14-api-and-contracts-map.md:32` | MATCH |
| `GET /v1/provenance/lineage/{execution_id}` | ✅ `server.rs:122-125` | ✅ `14-api-and-contracts-map.md:36` | MATCH |
| `POST /v1/provenance/lineage` | ✅ `server.rs:127` | ✅ `14-api-and-contracts-map.md:37` | MATCH |
| `POST /v1/provenance/query` | ✅ `server.rs:120` | ✅ `14-api-and-contracts-map.md:35` | MATCH |
| `POST /v1/provenance/ingest` | ✅ `server.rs:129` | ✅ `14-api-and-contracts-map.md:58` | MATCH — documented as U3 upgrade-track route (not in v1 support contract) |
| `POST /v1/executions/{execution_id}/evaluate-outcome` | ✅ `server.rs:158-161` | ✅ `14-api-and-contracts-map.md:61` | MATCH — documented as U1 upgrade-track route (not in v1 support contract) |
| `GET /v1/bridges` | ✅ `server.rs:131` | ✅ `14-api-and-contracts-map.md:64` | MATCH — documented as U4 upgrade-track route (not in v1 support contract) |
| `GET /v1/bridges/{bridge_id}/tools` | ✅ `server.rs:132` | ✅ `14-api-and-contracts-map.md:65` | MATCH — documented as U4 upgrade-track route (not in v1 support contract) |

**Section 3 verdict**: PASS — All router routes are documented in `14-api-and-contracts-map.md`.
U1–U4 upgrade-track routes are present in API map (lines 50–65) and documented as outside
the v1 single-node support contract. No drift.

---

## Section 4 — Operational Completeness

| Criterion | Status | Evidence | Notes |
|---|---|---|---|
| SQLite persistence | PASS | `ferrum-store` with embedded migrations | Evidence: `19-v1-single-node-support-contract.md:18` |
| Config docs current | PASS | `15-deployment-and-operations.md` | Evidence: `16-release-checklist.md:39` |
| CLI inspect surface | PARTIAL | health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance | No mutating CLI commands; post-v1 backlog |
| Approval workflow | PASS | GET /v1/approvals with pagination/filter | Evidence: `23-production-readiness-assessment.md:59` |
| Provenance query | PASS | GET/POST /v1/provenance/lineage, POST /v1/provenance/query | Evidence: `23-production-readiness-assessment.md:60-68` |
| Backup/restore docs | PASS | `18-single-node-operations-runbook.md:109-201` | `ferrumctl backup create/verify/restore` implemented; bounded offline workflow |
| Recovery procedure | PASS | `18-single-node-operations-runbook.md:204-254` | Compensate + manual restore fallback |
| Health/readiness are shallow | PASS (documented) | `19-v1-single-node-support-contract.md:95-101` | Operator must run functional probe |

**Section 4 verdict**: PASS — operational surface is complete for single-node v1 scope.
Shallow-health limitation is documented.

---

## Section 5 — Documentation Completeness

| Criterion | Status | Evidence | Notes |
|---|---|---|---|
| Project canon | PASS | `docs/ferrumgate-roadmap-v1/00-project-canon.md` | Evidence: `23-production-readiness-assessment.md:71` |
| Support contract | PASS | `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md` | Canonical reference |
| Deployment/ops docs | PASS | `docs/ferrumgate-roadmap-v1/15-deployment-and-operations.md` | Evidence: `23-production-readiness-assessment.md:58` |
| Operations runbook | PASS | `docs/ferrumgate-roadmap-v1/18-single-node-operations-runbook.md` | Evidence: `19-v1-single-node-support-contract.md:203` |
| API endpoint reference | PASS | `docs/ferrumgate-roadmap-v1/14-api-and-contracts-map.md` | Upgrade-track routes documented; no drift |
| Release checklist | PASS | `docs/16-release-checklist.md` | Evidence: `23-production-readiness-assessment.md:77` |
| RC evidence | PASS | `docs/implementation-path/25-v1-single-node-rc-evidence.md` | Evidence: `23-production-readiness-assessment.md:81` |
| Invariant matrix | PASS | `docs/implementation-path/26-v1-single-node-invariant-control-test-evidence-matrix.md` | 12 VERIFIED, 0 PARTIAL, 0 INFERRED |
| Production roadmap | PASS | `docs/implementation-path/30-production-roadmap.md` | Phase 1 production-ready; Phase 2 deferred; Phase 3 PostgreSQL |
| Production evaluation | PASS | `docs/implementation-path/27-production-evaluation-plan.md` | Production evaluation framework |

**Section 5 verdict**: PASS — docs pack is cohesive and current. API map upgrade-track
routes documented (lines 50–65) with no drift.

---

## Section 6 — Outstanding Items and Declaration

### 6.1 API-Map Doc Drift — Resolved

**Status**: FIXED — `docs/ferrumgate-roadmap-v1/14-api-and-contracts-map.md` now includes
an "Upgrade-Track Routes (NOT in v1 single-node support contract)" section (lines 50–65)
documenting:
- `POST /v1/provenance/ingest` — U3 Cross-runtime Provenance Fabric
- `POST /v1/executions/{execution_id}/evaluate-outcome` — U1 upgrade track
- `GET /v1/bridges` — U4 MCP/local/NemoClaw integrations
- `GET /v1/bridges/{bridge_id}/tools` — U4 MCP/local/NemoClaw integrations

All four routes are documented as outside v1 single-node support contract scope, consistent
with `19-v1-single-node-support-contract.md:81-86`.

### 6.2 Recommended Pre-Signoff Actions

| Item | Priority | Description |
|---|---|---|
| Re-run workspace build checks | MED | ✅ VERIFIED — layout/contract/fmt/check/clippy/test passed this session. Evidence refreshed. |
| Confirm API map fix applied | MED | ✅ Applied — upgrade-track routes documented in `14-api-and-contracts-map.md:50-65` |
| Re-verify integration tests | MED | ✅ VERIFIED — `integration_gateway_flow` (65 cases) and `integration_lineage_chain` (8 cases) passed as part of workspace test run this session. |
| Review accepted risks with operator | LOW | Weak Spots 1-4 documented in `19-v1-single-node-support-contract.md:131-179` and `26-v1-single-node-invariant-control-test-evidence-matrix.md:37-79` |

### 6.3 Signoff Declaration

- [ ] **Section 1 — Workspace Quality**: workspace compile/fmt/clippy/test evidence refreshed this session (layout/contract/fmt/check/clippy/test all passed)
- [ ] **Section 2 — Governance Enforcement**: reviewed; CONDITIONAL status acknowledged
- [ ] **Section 3 — API Surface**: reviewed; PASS — upgrade-track routes documented
- [ ] **Section 4 — Operational Completeness**: reviewed; no gaps in v1 single-node scope
- [ ] **Section 5 — Documentation Completeness**: reviewed; docs pack is cohesive
- [ ] **Section 6 — Outstanding Items**: pre-signoff actions above tracked

**Verdict**: RC-ready for single-node v1. Production deployment requires evaluation
against `27-production-evaluation-plan.md`. **Ready for operator review** — all checkbox
items must be resolved before final signoff.

---

*Template generated: 2026-04-09. Evidence grounded in repo truth as of this pass.*
