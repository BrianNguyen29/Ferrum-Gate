# 26 — v1 Single-Node Invariant Control Test Evidence Matrix

Single-node v1 scope. Maps each of the 12 invariants from `06-constraints-and-invariants.md`
to its control point, test coverage, evidence artifact, and verification status.

---

## Legend

| Symbol | Meaning |
|---|---|
| VERIFIED | Control implemented, test exists, artifact confirms |
| PARTIAL | Control or test exists but coverage is incomplete |
| INFERRED | Control implied by code structure; no direct test artifact |

---

## Invariant Matrix

| # | Invariant | Source | Control | Tests | Evidence | Status | Notes |
|---|---|---|---|---|---|---|---|
| 1 | Intent envelope validity (`allowed_outcomes.length >= 1`, `expires_at > created_at`) | `06-constraints-and-invariants.md:6-7` | `crates/ferrum-gateway/src/server.rs:compile_intent` I1 validation; `crates/ferrum-proto/src/intent.rs:IntentEnvelope::validate` | Direct unit tests in `crates/ferrum-proto/src/intent.rs:tests` for empty outcomes, expires_at==created_at, expires_at<created_at | `crates/ferrum-proto/src/intent.rs` validate impl + unit tests | VERIFIED | |
| 2 | Critical risk requires approval (`risk_tier = Critical` => `approval_mode != None`) | `06-constraints-and-invariants.md:8` | `crates/ferrum-pdp/src/engine.rs:178-192` | `crates/ferrum-pdp/src/engine.rs:847-920` | Direct unit tests for Critical+None, Critical+approval, ordering | VERIFIED | Control implemented in PDP engine evaluate() |
| 3 | Capability TTL max 300s | `06-constraints-and-invariants.md:13` | `crates/ferrum-cap/src/service.rs:51-53,71` | Direct TTL unit tests in `crates/ferrum-cap/src/service.rs` | Direct unit tests for TTL enforcement | VERIFIED | TTL enforcement verified by `ferrum-cap` service unit tests |
| 4 | Capability single-use | `00-project-canon.md:89`, `06-constraints-and-invariants.md:14` | `crates/ferrum-cap/src/service.rs:101-122` | `integration_gateway_flow.rs:93-137` | `16-release-checklist.md:19`, `25-v1-single-node-rc-evidence.md:29`, `23-production-readiness-assessment.md:28` | VERIFIED | `mark_capability_used_durable` called in authorize path (`server.rs:751-757`) with store persistence |
| 5 | Scope cannot expand beyond intent (`resource_bindings subset_of intent.resource_scope`) | `00-project-canon.md:93`, `06-constraints-and-invariants.md:15` | `crates/ferrum-gateway/src/server.rs:validate_resource_bindings_subset_of_scope`; invoked in `authorize_execution` | 16 unit tests + 2 integration test cases (`integration_gateway_flow.rs`: `test_i5_scope_validation_resource_bindings_exceed_intent_scope`, `test_i5_scope_validation_resource_bindings_within_intent_scope`) | `16-release-checklist.md:18`, `25-v1-single-node-rc-evidence.md:85-93`, `23-production-readiness-assessment.md:42` | VERIFIED | `validate_resource_bindings_subset_of_scope` enforces subset check at authorize; conservative prefix matching used (note: superset scope with prefix overlap is rejected — not a blocker for v1) |
| 6 | Approval binding matches action digest | `06-constraints-and-invariants.md:16` | `crates/ferrum-gateway/src/server.rs:validate_approval_binding_digest` | 8 integration tests: None skip, valid binding, pending denial, digest mismatch (proposal vs binding), expired binding, approval not found, chain-broken (binding vs approval digest mismatch), single-use with valid binding | `integration_gateway_flow.rs:3881-4886` | VERIFIED | |
| 7 | High taint blocks risky mutation (`taint_score >= 70` blocks non-R0) | `06-constraints-and-invariants.md:19-20` | `crates/ferrum-pdp/src/engine.rs:202-215`; `crates/ferrum-gateway/src/server.rs:377-393` (firewall taint scoring wired to PDP) | `crates/ferrum-pdp/src/engine.rs:457-482` (unit test); `integration_gateway_flow.rs:2308-2839` (InjectablePdpEngine-based poisoned context tests); `integration_gateway_flow.rs:test_i7_e2e_static_pdp_quarantine_on_high_taint` (real StaticPdpEngine + TaintScoringFirewall E2E) | `16-release-checklist.md:22`, `25-v1-single-node-rc-evidence.md:33,39-45`, `23-production-readiness-assessment.md:96-97` | VERIFIED | Full pipeline verified: `evaluate_proposal` → `build_firewall_context` → `TaintScoringFirewall.compute_taint_score` → `TrustContextSummary` → `StaticPdpEngine.evaluate` → `Decision::Quarantine`; new E2E test uses real `StaticPdpEngine` (not `InjectablePdpEngine`) and confirms taint threshold triggers quarantine |
| 8 | R3 requires approval and no auto-commit | `00-project-canon.md:90`, `06-constraints-and-invariants.md:24` | `crates/ferrum-pdp/src/engine.rs:63-74`, `crates/ferrum-rollback/src/service.rs:93-112` | `integration_gateway_flow.rs:147-212,905-983` | `16-release-checklist.md:20`, `25-v1-single-node-rc-evidence.md:30,74`, `23-production-readiness-assessment.md:27,45` | VERIFIED | Gateway prepare loads `rollback_class` from proposal (`server.rs:856-872`); R3 `auto_commit=false` correctly applied |
| 9 | R2 has compensation plan | `06-constraints-and-invariants.md:23` | `crates/ferrum-rollback/src/service.rs:79-86` | `crates/ferrum-rollback/src/service.rs:412-519` | Direct unit tests for empty plan rejection and planner success | VERIFIED | R2 without compensation plan now explicitly rejected |
| 10 | Provenance lineage queryability/completeness | `00-project-canon.md:81`, `06-constraints-and-invariants.md:28-29` | `crates/ferrum-gateway/src/server.rs:68-73,547-593,598-709`, `crates/ferrum-store/src/sqlite/provenance.rs:24-45,73-146` | `integration_lineage_chain.rs:643-945` (`test_lineage_chain_full_provenance_events`) | `16-release-checklist.md:26`, `25-v1-single-node-rc-evidence.md:47-56,99-107`, `23-production-readiness-assessment.md:43,100-101` | VERIFIED | Full 6-event chain asserted end-to-end by integration test |
| 11 | Output sanitization | `06-constraints-and-invariants.md:32-34`, `00-project-canon.md:92` | `crates/ferrum-firewall/src/lib.rs:16-18,279-294`; `crates/ferrum-gateway/src/server.rs` I11 wired to 7 targeted endpoints | `crates/ferrum-firewall/src/lib.rs:576-650`; `integration_gateway_flow.rs:test_i11_sanitizes_execution_response_with_control_characters`, `test_i11_sanitizes_error_response_for_invalid_bundle_id` | Direct unit tests for control chars; 2 integration tests proving gateway sanitization wired | VERIFIED | Trait-level sanitize_output implemented in firewall; bounded wiring to targeted endpoints per design note 48 (revoke_capability, delete_policy_bundle, set_policy_bundle_active, get_execution, get_execution_lineage, query_lineage, list_bridge_tools); integration tests pass |
| 12 | Approval listing/query in supported scope | `00-project-canon.md:44` | `crates/ferrum-gateway/src/server.rs:734-809,981-990,1008-1017`, `crates/ferrum-store/src/sqlite/approvals.rs:93-127` | `integration_gateway_flow.rs:1388-1561,1569-1807` | `16-release-checklist.md:24-25`, `25-v1-single-node-rc-evidence.md:36-38,157-159`, `23-production-readiness-assessment.md:59` | VERIFIED | |

---

## Weak Spots

The following have been resolved in code. Documented here for historical record
and audit trail.

### Weak Spot 1 — Rollback class at prepare (RESOLVED)

Gateway prepare now loads `rollback_class` from `proposal.requested_rollback_class`
(`server.rs:856-872`). The R3 `auto_commit=false` control is correctly applied
at prepare. The intent creation service (caller) remains responsible for setting
the correct `rollback_class` at intent creation.

### Weak Spot 2 — Draft-only at prepare (RESOLVED)

Prepare handler now revalidates draft-only status (`server.rs:874-898`), rejecting
with HTTP 403 if `intent.approval_mode == DraftOnly`. This prevents a draft-only
intent from bypassing evaluate and reaching prepare.

### Weak Spot 3 — Single-use capability end-to-end (RESOLVED)

The `mark_capability_used_durable` helper (`server.rs:685-732`) is called in the
authorize path (`server.rs:751-757`), persisting Used status to store. The
in-memory service and store are both checked for AlreadyUsed/Revoked/Expired
status.

### Weak Spot 4 — Provenance completeness (RESOLVED)

`test_lineage_chain_full_provenance_events` (`integration_lineage_chain.rs:643-945`)
exercises the full authorize → prepare → execute → verify chain and asserts all
6 event kinds appear in the lineage query, linked to execution_id.

---

## Summary

| Status | Count |
|---|---|
| VERIFIED | 12 (Invariants 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12) |
| PARTIAL | 0 |
| INFERRED | 0 |

---

## References

- Invariants: `docs/ferrumgate-roadmap-v1/06-constraints-and-invariants.md`
- Hard rules: `docs/ferrumgate-roadmap-v1/00-project-canon.md` section 6
- RC evidence: `docs/implementation-path/25-v1-single-node-rc-evidence.md`
- Production readiness: `docs/implementation-path/23-production-readiness-assessment.md`
- Source: `crates/ferrum-gateway/src/`, `crates/ferrum-pdp/src/`, `crates/ferrum-store/src/`
- Tests: `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`, `tests/integration_lineage_chain.rs`
