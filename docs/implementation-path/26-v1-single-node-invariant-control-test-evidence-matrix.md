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
| 1 | Intent envelope validity (`allowed_outcomes.length >= 1`, `expires_at > created_at`) | `06-constraints-and-invariants.md:6-7` | `crates/ferrum-gateway/src/server.rs:197-202,225-227` | None direct | Indirect via `23-production-readiness-assessment.md:40` | INFERRED | |
| 2 | Critical risk requires approval (`risk_tier = Critical` => `approval_mode != None`) | `06-constraints-and-invariants.md:8` | `crates/ferrum-gateway/src/server.rs:175,206` | None direct | None explicit | INFERRED | Control gap |
| 3 | Capability TTL max 300s | `06-constraints-and-invariants.md:13` | `crates/ferrum-cap/src/service.rs:51-53,71` | None direct | None explicit | PARTIAL | |
| 4 | Capability single-use | `00-project-canon.md:89`, `06-constraints-and-invariants.md:14` | `crates/ferrum-cap/src/service.rs:101-122` | `integration_gateway_flow.rs:93-137` | `16-release-checklist.md:19`, `25-v1-single-node-rc-evidence.md:29`, `23-production-readiness-assessment.md:28` | PARTIAL | Gateway authorize path does not call `mark_used`; see `server.rs:356-365` |
| 5 | Scope cannot expand beyond intent (`resource_bindings subset_of intent.resource_scope`) | `00-project-canon.md:93`, `06-constraints-and-invariants.md:15` | `crates/ferrum-pdp/src/engine.rs:31-46` | `integration_gateway_flow.rs:1145-1221,1225-1294` | `16-release-checklist.md:18`, `25-v1-single-node-rc-evidence.md:85-93`, `23-production-readiness-assessment.md:42` | PARTIAL | |
| 6 | Approval binding matches action digest | `06-constraints-and-invariants.md:16` | `crates/ferrum-cap/src/service.rs:64`, `crates/ferrum-store/src/sqlite/approvals.rs:26-35` | None direct | None explicit | INFERRED | |
| 7 | High taint blocks risky mutation (`taint_score >= 70` blocks non-R0) | `06-constraints-and-invariants.md:19-20` | `crates/ferrum-pdp/src/engine.rs:48-61` | `integration_gateway_flow.rs:565-647,658-1135` | `16-release-checklist.md:22`, `25-v1-single-node-rc-evidence.md:33,39-45`, `23-production-readiness-assessment.md:96-97` | PARTIAL | |
| 8 | R3 requires approval and no auto-commit | `00-project-canon.md:90`, `06-constraints-and-invariants.md:24` | `crates/ferrum-pdp/src/engine.rs:63-74`, `crates/ferrum-rollback/src/service.rs:93-112` | `integration_gateway_flow.rs:147-212,905-983` | `16-release-checklist.md:20`, `25-v1-single-node-rc-evidence.md:30,74`, `23-production-readiness-assessment.md:27,45` | PARTIAL | Gateway prepare currently hardcodes R0; see `server.rs:422-427` |
| 9 | R2 has compensation plan | `06-constraints-and-invariants.md:23` | `crates/ferrum-rollback/src/service.rs:110` | None direct | None explicit | INFERRED | |
| 10 | Provenance lineage queryability/completeness | `00-project-canon.md:81`, `06-constraints-and-invariants.md:28-29` | `crates/ferrum-gateway/src/server.rs:68-73,547-593,598-709`, `crates/ferrum-store/src/sqlite/provenance.rs:24-45,73-146` | `integration_lineage_chain.rs:58-316` (see caveat at `tests/integration_lineage_chain.rs:4-6`) | `16-release-checklist.md:26`, `25-v1-single-node-rc-evidence.md:47-56,99-107`, `23-production-readiness-assessment.md:43,100-101` | PARTIAL | |
| 11 | Output sanitization | `06-constraints-and-invariants.md:32-34`, `00-project-canon.md:92` | `crates/ferrum-firewall/src/lib.rs:16-18,25-27` | None direct | None explicit | INFERRED | Current firewall control is passthrough/noop |
| 12 | Approval listing/query in supported scope | `00-project-canon.md:44` | `crates/ferrum-gateway/src/server.rs:734-809,981-990,1008-1017`, `crates/ferrum-store/src/sqlite/approvals.rs:93-127` | `integration_gateway_flow.rs:1388-1561,1569-1807` | `16-release-checklist.md:24-25`, `25-v1-single-node-rc-evidence.md:36-38,157-159`, `23-production-readiness-assessment.md:59` | VERIFIED | |

---

## Weak Spots

The following are known limitations of the current v1 single-node implementation.

### Weak Spot 1 — Commit/rollback claim mismatch

The gateway's commit and rollback handlers are described in the supported flows, but the
gateway prepare handler at `crates/ferrum-gateway/src/server.rs:422-427` hardcodes `R0`
before transitioning to the PREPARED state. This means the R3 `auto_commit = false` control
is bypassed at the prepare step. A caller could reach a PREPARED state with `rollback_class = R0`
even when the intent requires R3. **Impact**: R3 invariant (Invariant 8) is partially
undermined at prepare. **Mitigation**: Caller must ensure the correct `rollback_class`
is persisted by the intent creation service before the execution flow begins.

### Weak Spot 2 — Draft-only-at-prepare claim mismatch

`06-constraints-and-invariants.md` states draft-only intents are blocked at prepare,
but the check exists only in the PDP evaluate path (`crates/ferrum-pdp/src/engine.rs:76-85`).
The `POST /v1/executions/{id}/prepare` handler does not re-validate `intent.is_draft_only`
before transitioning to PREPARED. If a draft-only intent bypasses evaluate (e.g.,
re-hydrated from a stale execution record), prepare may not re-check. **Impact**: Low
in single-node v1 since all flows go through evaluate first.

### Weak Spot 3 — Single-use not wired end-to-end

The capability service `mark_used` function exists (`crates/ferrum-cap/src/service.rs:101-122`)
but the gateway authorize path at `crates/ferrum-gateway/src/server.rs:356-365` fetches the
capability without calling `mark_used`. **Impact**: A caller could reuse a single-use
capability by bypassing execute, or mint multiple single-use capabilities for one intent.
**Mitigation**: Caller is expected to track `intent_id` -> single capability mapping;
not enforced server-side at authorize.

### Weak Spot 4 — Provenance completeness only indirectly evidenced

Lineage events are emitted at each gateway step, but there is no end-to-end
test that queries the lineage endpoint for a full execution
(evaluate -> mint -> authorize -> prepare -> execute -> verify -> commit) and
confirms every step appears in the chain. The existing tests in
`tests/integration_lineage_chain.rs` cover endpoint behavior (empty, 404,
invalid UUID, max_hops) but not happy-path chain completeness. **Impact**: A silent
gap in event emission would not be caught by the current test suite.
**Mitigation**: Manual trace or ad-hoc integration run required to confirm
full chain completeness.

---

## Summary

| Status | Count |
|---|---|
| VERIFIED | 1 |
| PARTIAL | 6 (Invariants 3, 4, 5, 7, 8, 10) |
| INFERRED | 5 (Invariants 1, 2, 6, 9, 11) |

---

## References

- Invariants: `docs/06-constraints-and-invariants.md`
- Hard rules: `docs/00-project-canon.md` section 6
- RC evidence: `docs/implementation-path/25-v1-single-node-rc-evidence.md`
- Production readiness: `docs/implementation-path/23-production-readiness-assessment.md`
- Source: `crates/ferrum-gateway/src/`, `crates/ferrum-pdp/src/`, `crates/ferrum-store/src/`
- Tests: `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`, `tests/integration_lineage_chain.rs`
