# Q1-P3 — PDP Hard-Rules Audit Evidence

**Date:** 2026-04-09
**Package:** Q1-P3 (PDP Hard-Rules Audit)
**Evidence type:** Audit note / branch coverage summary

---

## Objective

Audit all PDP decision branches for scope/taint/R3/draft-only enforcement.
Ensure every rule is deterministic — no "maybe" branches.

Reference: `docs/artifacts/2026-04-09/manifest.txt`, `13-q1-work-packages.md:179`

---

## Branch Coverage Map

All branches in `ferrum-pdp/src/engine.rs` `StaticPdpEngine::evaluate()` are
deterministic with a single explicit outcome. No "maybe" paths exist.

| # | Branch | Condition | Decision | Rule ID |
|---|--------|-----------|----------|---------|
| 1 | Scope deny | `resource_scope` empty + non-R0 mutation | `Deny` | `scope.mismatch.empty.scope` |
| 2 | Taint quarantine | `taint_score >= 70` + non-R0 mutation | `Quarantine` | `quarantine.high.taint.mutation` |
| 3 | R3 approval required | `requested_rollback_class == R3IrreversibleHighConsequence` | `RequireApproval` | `approval.r3.required` |
| 4 | Draft-only intent | `approval_mode == DraftOnly` | `AllowDraftOnly` | `draft.only.intent` |
| 5 | Forbidden outcome | inferred effect matches `forbidden_outcomes` | `Deny` | `outcome.forbidden` |
| 6 | Advisory mismatch | inferred effect not in `allowed_outcomes` (non-empty) | `Allow` + warning | `outcome.advisory.mismatch` |
| 7 | Default allow | none of the above fire | `Allow` | `allow.default` |

**Note:** Branches 3 and 4 are mutually exclusive by construction — R3 fires on
`RollbackClass::R3IrreversibleHighConsequence` before the DraftOnly check can
fire. Verified by `test_evaluate_r3_before_draft_only` ordering test.

---

## Branch Code Evidence

Branches 1–7 are implemented in `ferrum-pdp/src/engine.rs` lines 174–257.

- **No `match` with unreachable or `_` catch-all that could含糊 (be ambiguous)**
- Every guard condition is explicit (`if`, `matches!`, discriminant comparison)
- Advisory mismatch (Branch 6) warns but does not deny — this is intentional
- All 7 branches have corresponding unit tests (`engine.rs:397–598`)

---

## Draft-Only Gateway Fix

**File:** `crates/ferrum-gateway/src/server.rs:275`
**Before (defect):** `approval_mode` defaulted to `ApprovalMode::None` ignoring
the request value.
**After (fix):** `approval_mode: req.approval_mode.unwrap_or(ApprovalMode::None)`
correctly propagates the caller's requested mode.

Integration test confirms correct propagation:
- `crates/ferrum-integration-tests/src/integration_gateway_flow.rs:1396`
  sets `approval_mode: Some(ferrum_proto::ApprovalMode::DraftOnly)` on compile
- `integration_gateway_flow.rs:1425` asserts the compiled intent has
  `DraftOnly` mode — PASS

---

## Gate A Status

**Gate A criterion (from 01-quarterly-plan.md:49):**
> PDP audit notes show scope/taint/R3/draft-only rules are deterministic; no
> "maybe" branches

**Q1-P3 slice verdict:** ✅ **SATISFIED** — all 7 branches are deterministic.

**Gate A is NOT fully closed** because Gate A (step 1.3 → 1.4) also requires:
- PDP rules stable before `mark_used` is wired (`ferrum-cap` cap closure not yet
  evidenced)

This bundle does not close Gate A in full. It only documents that the Q1-P3
PDP audit slice (scope/taint/R3/draft-only) is deterministic and that the
draft-only gateway propagation bug is fixed.

Full Gate A closeability requires `ferrum-cap` `mark_used` integration evidence
or an explicit risk-accepted note.

---

## Compilation Verification

- `cargo check --workspace` — PASS (no changes to PDP or gateway in this slice)
- `cargo test --package ferrum-pdp` — PASS (branch coverage tests pass)

---

## Summary

| Criterion | Status |
|-----------|--------|
| All 7 PDP branches deterministic (scope, taint, R3, draft-only, outcomes, advisory, default) | PASS |
| No "maybe" branches in hard-rules enforcement | PASS |
| Draft-only `approval_mode` correctly propagated at compile (server.rs:275) | PASS |
| Q1-P3/PDP audit slice evidence | RECORDED |

**Gate A overall: NOT FULLY CLOSED** — this bundle covers the Q1-P3/PDP audit
slice only. Full Gate A closeability requires additional `mark_used` evidence.
