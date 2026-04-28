# Q1-P7 — Invariant Matrix Pass Evidence

**Date:** 2026-04-09
**Package:** Q1-P7
**Status:** SATISFIED (invariant matrix pass / Q1 exit gate)
**Evidence type:** Full workspace test pass + route parity confirmation + prior package pass chain

---

## Q1-P7 Package Objective

> Run the full test suite and produce an evidence summary confirming all Q1 invariants
> hold. This is the Q1 exit gate package (Q1-P7 = Gate C exit = v1.1 release gate).

**Scope of this note:** Grounded pass conditions for Q1/v1.1 gate only — limited to
the four already-established facts listed below. No expansion of v1 support scope.

---

## Grounded Pass Conditions

Q1-P7 is marked **SATISFIED** based on the following four facts only:

| # | Fact | Evidence |
|---|------|----------|
| 1 | `cargo test --workspace` passes | Full workspace test suite passes (all crates) |
| 2 | `cargo test -p ferrum-gateway` passes | Gateway crate tests pass |
| 3 | Runtime/OpenAPI route parity: 19/19 | All runtime routes have corresponding OpenAPI entries; no undocumented routes |
| 4 | Q1-P6 (adversarial first slices) passed | WS1/WS2/WS3/WS4 each have a passing adversarial regression test (`07-q1-p6-adversarial-first-slices-evidence.md`) |

No other facts are claimed in this note. Specifically:

- **No literal execute endpoint claimed** — gate pass does not require a dedicated `/v1/executions/execute` route; terminal events are emitted via the existing prepare path
- **No v1 scope expansion** — this note records defect closure within the existing v1 support contract
- **Q1-P5 conservative minimum chain preserved** — Q1-P5 remains documented as a conservative slice pass with no literal execute endpoint (`06-q1-p5-minimum-chain-evidence.md`)
- **Gate A partial wording preserved** — Gate A is still described as partial; Q1-P7 does not retroactively elevate Gate A to fully closed

---

## Code Evidence

### Full workspace test pass

```sh
cargo test --workspace
```

All crates in the workspace pass. No regressions observed.

### Gateway crate test pass

```sh
cargo test -p ferrum-gateway
```

Gateway-specific tests pass. Confirms route handlers are wired correctly.

### Route parity: 19/19

OpenAPI spec (`openapi/ferrumgate-control-api.v1.yaml`) has 19 routes defined.
Runtime router (`crates/ferrum-gateway/src/server.rs`) serves 19 routes.
Route parity confirmed: no undocumented routes, no orphan OpenAPI entries.

### Q1-P6 adversarial first slices chain

From `docs/artifacts/2026-04-09/07-q1-p6-adversarial-first-slices-evidence.md`:

| Weak Spot | Adversarial Test | Result |
|-----------|------------------|--------|
| WS1 (R3 rollback_class propagation) | `test_r3_contracts_have_auto_commit_false` | PASS |
| WS2 (capability reuse blocked) | `test_authorize_can_only_be_called_once` | PASS |
| WS3 (draft-only bypass blocked) | `test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate` | PASS |
| WS4 (lineage partial flow) | `test_lineage_adversarial_partial_execution_no_terminal` | PASS |

WS1–WS4 adversarial first slices: 4/4 tests pass. Q1-P6 is satisfied.

---

## Scope Limitation — Conservative Wording

**Q1-P7 (invariant matrix pass): SATISFIED — Q1 exit gate for v1.1 scope.**

Q1-P7 is satisfied for Q1/v1.1 gate scope based on the four grounded facts above.

**Q1-P7 does NOT claim:**
- Any post-v1 scope or v1.2+ features
- Full adversarial suite exhaustiveness (first slices only; further adversarial expansion is post-v1.1)
- Any new routes beyond the 19/19 confirmed routes
- Any adapter-backed behavior beyond the v1 kernel surface

**Q1-P7 scope:** Q1 exit gate / v1.1 release gate. Evidence chain: P1→P2→P3→P4→P5→P6→P7 is complete for the conservative Q1 hardening surface. Q2 entry gate is now satisfied.

---

## Relationship to v1.1 Release Gate

Q1-P7 satisfies the v1.1 release gate per `02-release-plan.md` — Release 1 scope:
- Weak Spot 1: rollback_class fix (WS1, confirmed in Q1-P6)
- Weak Spot 2: mark_used at authorize (WS2, confirmed in Q1-P6)
- Weak Spot 3: draft-only revalidation (WS3, confirmed in Q1-P6)
- Weak Spot 4: lineage minimum chain (WS4, confirmed in Q1-P6)
- Route table reconciled (19/19 route parity confirmed above)

Gate evidence recorded here: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`.

---

## Summary

| Criterion | Status | Evidence |
|-----------|--------|----------|
| `cargo test --workspace` pass | PASS | workspace-wide test suite passes |
| `cargo test -p ferrum-gateway` pass | PASS | gateway crate tests pass |
| Route parity 19/19 | PASS | OpenAPI vs runtime router confirmed |
| Q1-P6 adversarial chain (WS1–WS4) | PASS | `07-q1-p6-adversarial-first-slices-evidence.md` |
| Q1-P7 invariant matrix pass | SATISFIED | this note |
| Q1/v1.1 exit gate | PASS | chain of P1–P7 |

**Q1-P7: SATISFIED — Q1 exit gate passed for v1.1 scope. Q2 entry gate is now satisfied.**
