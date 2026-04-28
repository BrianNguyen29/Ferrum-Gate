# 13 — Q1 Work Packages

## Purpose

This document provides **execution-ready work packages** for FerrumGate Q1 kernel hardening.
Each package is sized for one engineer or one agent to pick up directly.

**Q1 is v1 hardening work.** All packages in this document are scoped to closing
Weak Spots 1–4 from the v1 support contract (`19-v1-single-node-support-contract.md`).
Q1 packages do **not** expand the v1 support contract; they close or document
accepted risks within the existing v1 boundary.

**Q2 must not begin until the Q1 exit gate is passed.** Evidence of gate pass
is required before Q2 work is treated as committed.

---

## How to execute a package

Each package below is self-contained. Follow these three steps in order:

1. **Verify preconditions** — Check the `Blockers` field and confirm all dependent
   packages are done. Gate evidence lives in `docs/artifacts/<date>/`.
2. **Execute the package** — Start from the `Starting point` directory. Work
   until all `Done criteria` are met. Record evidence as described in
   `Evidence required`.
3. **Update pack docs on close** — Before declaring a package done, mark the
   items listed in `Pack docs to update on close` so the pack stays in sync.

**Quick-verify command convention** (for packages with a single primary crate):
```sh
cargo test --package <crate-name> -- --nocapture
```
For integration-level packages, see the per-package `Verification` field.

---

## Package map

| # | Package | Steps | Gate |
|---|---|---|---|
| Q1-P1 | Proto Shape Lock | 1.1 | G1 |
| Q1-P2 | Store Integrity | 1.2 | G2 |
| Q1-P3 | PDP Hard-Rules Audit | 1.3 | G3 |
| Q1-P4 | Cap mark_used + Rollback State Machine | 1.4, 1.5 | Gate A (G3→P4), Gate B (G2→P4) |
| Q1-P5 | Gateway Lineage Completeness | 1.6 | Gate C (P4→P5) |
| Q1-P6 | Testkit Adversarial Suite | 1.7 | Gate C (P5→P6) |
| Q1-P7 | Invariant Matrix Pass | 1.8 | Gate C (P6→P7 = exit gate) |

---

## Q1-P1 — Proto Shape Lock

### Objective
Finalize naming for all intent/proposal/capability/rollback/provenance/approval domain
objects so downstream crates have stable field names for the quarter.

### Inputs
- Current proto definitions in `ferrum-proto/src/`
- Current JSON schemas in `schemas/`
- v1 support contract field references in `19-v1-single-node-support-contract.md`

### Outputs
- Stable proto shapes for: intent, proposal, capability, rollback, provenance, approval
- Updated JSON schemas matching proto shapes
- No field renames mid-quarter (enforced by this package closing G1)

### Dependencies
- None (first package in Q1)

### Affected crates / APIs
- `ferrum-proto` — all domain object shapes
- `ferrum-store` — uses proto types for state
- `ferrum-pdp` — uses proto types for decision context
- `ferrum-cap` — uses proto types for capability fields
- `ferrum-gateway` — uses proto types for API payloads
- OpenAPI spec — must match proto shapes

### Evidence required
- Proto file diff showing no further rename planned for the quarter
- Schema diff showing schemas in sync with proto
- A short note in `docs/artifacts/<date>/` confirming G1 is satisfied

### Done criteria
- Field names for intent/proposal/capability/rollback/provenance/approval are locked
- All downstream crate code that uses these shapes compiles with the new names
- No open rename items in the quarter plan

### Blockers
- None for starting

### Starting point
- `crates/ferrum-proto/src/` — domain object definitions (intent, proposal, capability, rollback, provenance, approval)
- `schemas/` — JSON schema files that must stay in sync

### Verification
```sh
cargo check --package ferrum-proto --workspace
cargo test --package ferrum-proto
```
Confirm all downstream crates (`ferrum-store`, `ferrum-pdp`, `ferrum-cap`, `ferrum-gateway`) still compile after shape changes:
```sh
cargo check --workspace
```

### Pack docs to update on close
- `10-master-checklist.md` — mark G1 (proto shape lock) done
- `01-quarterly-plan.md` — record G1 gate evidence in the Q1 Evidence table
- `docs/artifacts/<date>/` — add a short note confirming G1 is satisfied (field names locked, no mid-quarter rename planned)

### V1 boundary note
> Proto shape stability is a Q1 prerequisite (G1) for all downstream v1 hardening work.
> This package is fully within the v1 kernel. Stable proto shapes enable the
> downstream defect-closure work for Weak Spots 1–4.

---

## Q1-P2 — Store Integrity

### Objective
Audit and finalize explicit state transitions for executions, capabilities, and approvals
in `ferrum-store`. Ensure provenance append-only semantics hold and single-use
capability persistence path is closed.

### Inputs
- Current `ferrum-store` traits and state machine code
- Current `ferrum-store` execution/capability/approval state transition diagrams
- G2 gate requirement: state transitions must be settled before `ferrum-cap` and `ferrum-rollback` can proceed

### Outputs
- Documented explicit state transitions for executions (authorize → prepare → execute → terminal)
- Documented state transitions for capabilities (mint → used/revoked)
- Documented state transitions for approvals (pending → approved/rejected)
- Provenance append-only guarantee validated or gap documented

### Dependencies
- Q1-P1 (proto shape lock) complete — store uses proto types

### Affected crates / APIs
- `ferrum-store` — state trait and transition logic
- `ferrum-cap` — relies on store for single-use persistence
- `ferrum-rollback` — relies on store for rollback_class propagation
- `ferrum-graph` — relies on store for lineage append-only

### Evidence required
- Code reference or test output showing state transitions are explicit and deterministic
- If gaps remain: a note in `docs/artifacts/<date>/` listing the gap and risk-accepted status

### Done criteria
- All execution states have documented next-state transitions
- All capability states have documented next-state transitions
- All approval states have documented next-state transitions
- Provenance append-only is validated or gap is documented with accepted risk

### Blockers
- Q1-P1 must complete before this starts

### Starting point
- `crates/ferrum-store/src/` — examine the store traits and state transition implementations
- `crates/ferrum-store/src/` or `crates/ferrum-integration-tests/` — look for existing state machine diagrams or transition tests

### Verification
```sh
cargo test --package ferrum-store
```
If tests cover state transitions, confirm they pass. If no tests yet, add a minimal state-transition test (see `crates/ferrum-store/tests/` if it exists, otherwise add under `crates/ferrum-store/src/`).

### Pack docs to update on close
- `10-master-checklist.md` — mark G2 (store state transitions) done
- `01-quarterly-plan.md` — record G2 gate evidence in the Q1 Evidence table
- `docs/artifacts/<date>/` — add note confirming state transitions are explicit/deterministic, or listing any remaining gap with risk-accepted status

### V1 boundary note
> `ferrum-store` is part of the v1 kernel. State transition integrity directly affects
> Weak Spots 1–4. This package is v1 hardening work, not v1 scope expansion.

---

## Q1-P3 — PDP Hard-Rules Audit

### Objective
Audit all PDP decision branches for scope/taint/R3/draft-only enforcement.
Ensure every rule is deterministic — no "maybe" branches. Expose an explainable
decision structure.

### Inputs
- Current PDP code in `ferrum-pdp/src/`
- Scope, taint, R3, and draft-only rule definitions from `03-crate-workplan.md`
- G3 gate: PDP hard rules must be stable before `ferrum-cap` mark_used closure can be verified

### Outputs
- Audit notes showing every PDP decision path for scope, taint, R3, and draft-only
- No "maybe" branches in hard-rules enforcement
- Stable explainable decision structure for operator-facing "why" payload

### Dependencies
- Q1-P1 (proto shape lock) complete — PDP uses proto types

### Affected crates / APIs
- `ferrum-pdp` — decision engine and hard-rules
- `ferrum-gateway` — calls PDP for every execution authorization
- API: `POST /v1/proposals/{proposal_id}/evaluate` — must reflect stable PDP rules

### Evidence required
- PDP audit notes (can be a code annotation summary or a doc note in `docs/artifacts/<date>/`)
- Every rule branch has a clear deny/allow/quarantine/approval outcome
- Gate A evidence: PDP audit notes confirm scope/taint/R3/draft-only rules are deterministic

### Done criteria
- All scope enforcement paths are deterministic
- All taint scoring paths are deterministic
- R3 enforcement (no auto-commit without approval) is deterministic
- Draft-only revalidation is deterministic
- PDP "why" structure is stable and usable

### Blockers
- Q1-P1 must complete before this starts

### Starting point
- `crates/ferrum-pdp/src/` — examine decision branches for scope, taint, R3, and draft-only enforcement
- `crates/ferrum-pdp/src/` — look for existing rule-enforcement test files (e.g., `rules_tests.rs`, `decision_tests.rs`)

### Verification
```sh
cargo test --package ferrum-pdp
```
Confirm all scope/taint/R3/draft-only rule branches are exercised and pass. If gaps exist in test coverage, add test cases to cover the missing branches before declaring audit complete.

### Pack docs to update on close
- `10-master-checklist.md` — mark G3 (PDP hard rules audit) done
- `01-quarterly-plan.md` — record Gate A evidence in the Q1 Evidence table (PDP rules deterministic, no "maybe" branches)
- `docs/artifacts/<date>/` — add PDP audit notes confirming scope/taint/R3/draft-only rules are deterministic per branch

### V1 boundary note
> PDP hard-rules enforcement is part of the v1 kernel. The R3 no-auto-commit rule
> is directly relevant to Weak Spot 1 (prepare-step rollback class gap). This
> package is v1 hardening, not v1 scope expansion.

---

## Q1-P4 — Cap mark_used Path Closure + Rollback State Machine Fix

### Objective
Two closely coupled fixes that must be completed together:
1. Close `cap mark_used` single-use enforcement at the authorize path
2. Fix `rollback_class` propagation at the prepare step

### Inputs
- `ferrum-cap/src/` — cap authorization and mark_used logic
- `ferrum-rollback/src/` — rollback state machine and rollback_class propagation
- PDP hard-rules audit output (Q1-P3) — must be stable before this starts (Gate A)
- Store state transitions (Q1-P2) — must be stable before this starts (Gate B)

### Outputs
**Cap mark_used closure:**
- `mark_used` is called at authorize (not deferred to execute)
- Single-use enforcement is proven end-to-end with a test
- Capability cannot be reused after authorization

**Rollback state machine fix:**
- `rollback_class` is propagated to execution state at prepare step
- R3 `auto_commit=false` respected at prepare
- Rollback transitions (R0/R1/R2/R3) are consistent and tested

### Dependencies
- Q1-P3 (PDP hard-rules audit) complete — Gate A
- Q1-P2 (store integrity) complete — Gate B

### Affected crates / APIs
- `ferrum-cap` — mark_used path and single-use enforcement
- `ferrum-rollback` — rollback_class propagation and state machine
- `ferrum-store` — stores execution state with rollback_class
- `ferrum-gateway` — calls cap authorize and rollback prepare

### Evidence required
- Gate A evidence: PDP rules are stable (from Q1-P3)
- Gate B evidence: store state transitions are settled (from Q1-P2)
- Test or code reference showing mark_used called at authorize path
- Test or code reference showing rollback_class propagated at prepare step
- Test output for R3 auto_commit=false respected at prepare

### Done criteria
- Single-use capability is enforced end-to-end at authorize path — client discipline is not required
- Prepare-step rollback_class gap is closed — no bypass path for R3 no-auto-commit
- Gate C preconditions are satisfied (both P4 items pass)

### Blockers
- Gate A: Q1-P3 must complete before cap mark_used closure
- Gate B: Q1-P2 must complete before rollback state machine fix

### Starting point
- **Cap mark_used closure**: `crates/ferrum-cap/src/` — find the `authorize` path and `mark_used` call site; confirm `mark_used` is invoked at authorize, not deferred to execute
- **Rollback state machine fix**: `crates/ferrum-rollback/src/` — find `rollback_class` propagation at the prepare step; locate R0/R1/R2/R3 transition implementations

### Verification
```sh
cargo test --package ferrum-cap
cargo test --package ferrum-rollback
```
For cap: confirm a test exists that proves a capability cannot be reused after authorization (e.g., `tests/cap_tests.rs` or similar). For rollback: confirm a test shows `rollback_class` is present in execution state after prepare, and that `R3 auto_commit=false` is respected at prepare.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate A and Gate B preconditions satisfied
- `01-quarterly-plan.md` — record Gate A and Gate B evidence in the Q1 Evidence table
- `19-v1-single-node-support-contract.md` — if Weak Spots 1 or 2 remain partially open, update accepted-risk notes here
- `docs/artifacts/<date>/` — add test output or code reference showing mark_used at authorize and rollback_class at prepare

### V1 boundary note
> Cap mark_used closure addresses Weak Spot 2 (single-use enforcement).
> Rollback state machine fix addresses Weak Spot 1 (prepare-step rollback class gap).
> Both are v1 hardening within the existing v1 support contract.

---

## Q1-P5 — Gateway Lineage Completeness Test

### Objective
Add an end-to-end integration test that asserts the full provenance minimum-chain
for gateway executions. The test must verify all terminal-path events are present
in the API response.

### Inputs
- Gateway code in `ferrum-gateway/src/`
- Provenance code in `ferrum-graph/src/`
- Cap mark_used closure (Q1-P4) — Gate C requires this
- Rollback state machine fix (Q1-P4) — Gate C requires this

### Outputs
- Integration test for minimum-chain lineage assertion
- Test asserts: authorize event + prepare event + execute event + terminal event are all present
- Test is part of the permanent test suite, not a one-off check

### Dependencies
- Q1-P4 (cap mark_used + rollback fix) complete — Gate C

### Affected crates / APIs
- `ferrum-gateway` — emits provenance events
- `ferrum-graph` — aggregates lineage events
- API: `GET /v1/provenance/lineage/{execution_id}` — must return full minimum chain
- API: `POST /v1/provenance/lineage` — must accept the query

### Evidence required
- Gate C evidence: both cap enforcement and rollback fix are passing
- Test output showing lineage chain test passes with all terminal-path events present
- A note in `docs/artifacts/<date>/` if any terminal-path event is still missing (with risk-accepted status)

### Done criteria
- Integration test covers the full authorize → prepare → execute → terminal path
- All terminal-path events appear in the API response for the lineage endpoint
- Test passes consistently

### Blockers
- Gate C: Q1-P4 (both cap enforcement and rollback fix) must complete before this starts

### Starting point
- `crates/ferrum-gateway/src/` — examine provenance event emission at authorize/prepare/execute/terminal steps
- `crates/ferrum-graph/src/` — examine lineage aggregation and query response construction
- `crates/ferrum-integration-tests/` — look for existing lineage tests as a template for the new test

### Verification
```sh
cargo test -p ferrum-integration-tests --test integration -- lineage
```
If no integration test exists yet, create `crates/ferrum-integration-tests/tests/lineage_chain_test.rs` asserting authorize + prepare + execute + terminal events all appear in `GET /v1/provenance/lineage/{execution_id}`. Verify the test passes.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate C (lineage chain) satisfied
- `01-quarterly-plan.md` — record Gate C evidence in the Q1 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference showing full authorize→prepare→execute→terminal chain in API response; note any missing terminal-path event with risk-accepted status

### V1 boundary note
> Lineage completeness addresses Weak Spot 4 (minimum-chain integration test gap).
> This is v1 hardening within the existing v1 support contract.

### Status annotation — conservative closure
> **Q1-P5 — CLOSED (conservative minimum-chain pass):**
> The minimum chain (authorize + prepare + terminal-present) is confirmed over the
> existing gateway execution surface via `integration_lineage_chain.rs`. A literal
> `/v1/executions/execute` endpoint is absent and is not claimed. This is a
> conservative slice pass — not a claim of full Q1 exit gate closure beyond the
> Q1-P4 package dimension. Evidence: `docs/artifacts/2026-04-09/06-q1-p5-minimum-chain-evidence.md`.

---

## Q1-P6 — Testkit Adversarial Suite

### Objective
Build adversarial test cases that attempt to bypass each of the four weak spots
closed in Q1. Each bypass attempt must be stopped by the hardening done in P1–P5.

### Inputs
- Weak Spots 1–4 from `19-v1-single-node-support-contract.md`
- Cap mark_used closure (Q1-P4)
- Rollback state machine fix (Q1-P4)
- Gateway lineage test (Q1-P5)

### Outputs
- One or more adversarial test cases per weak spot
- Testkit directory structure for adversarial tests
- Evidence that bypass attempts are blocked

### Dependencies
- Q1-P5 (gateway lineage test) complete — Gate C

### Affected crates / APIs
- `ferrum-testkit` or `tests/` directory
- All four weak-spot areas receive explicit adversarial coverage

### Evidence required
- Gate C evidence: lineage test passes
- Adversarial test output for each weak spot showing bypass is blocked
- If a bypass is not fully stoppable: document as accepted risk in `19-v1-single-node-support-contract.md`

### Done criteria
- Each weak spot has at least one adversarial test case
- All adversarial tests pass (bypass blocked) or risk-accepted note exists
- Test suite is in permanent location, not ad-hoc

### Blockers
- Gate C: Q1-P5 must complete before this starts

### Starting point
- `crates/ferrum-testkit/src/` — examine existing testkit utilities and adversarial test helpers
- `tests/` or `crates/ferrum-integration-tests/tests/` — locate the adversarial test directory or create one

### Verification
```sh
cargo test -p ferrum-integration-tests --test integration --
```
Run the full integration suite including any new adversarial tests. Each weak spot should have a corresponding adversarial test that attempts a bypass and is blocked. Confirm all adversarial tests pass (bypass blocked) or document accepted risk.

If adding new test files, use a naming convention like `tests/adversarial/ws1_rollback_class_bypass.rs`.

### Pack docs to update on close
- `10-master-checklist.md` — mark adversarial suite done and Gate C satisfied
- `19-v1-single-node-support-contract.md` — if any bypass is not fully stoppable, document the accepted risk here
- `01-quarterly-plan.md` — record adversarial test results in exit gate evidence
- `docs/artifacts/<date>/` — add adversarial test output per weak spot

### V1 boundary note
> Adversarial testing against Weak Spots 1–4 is v1 hardening. Any bypass that
> cannot be fully closed must be documented as accepted risk in the v1 support
> contract, not claimed as a new v1 feature.

---

## Q1-P7 — Invariant Matrix Pass

### Objective
Run the full test suite and produce an evidence summary confirming all Q1 invariants
hold. This is the Q1 exit gate package.

### Inputs
- All Q1 packages complete (P1–P6)
- Full test suite in `tests/` and across crates

### Outputs
- Test output summary from full suite run
- Evidence summary note in `docs/artifacts/<date>/`
- Updated `10-master-checklist.md` with Q1 items marked done or risk-accepted
- Route table canonical note confirming docs/spec/runtime in sync

### Dependencies
- Q1-P6 (adversarial suite) complete — Gate C
- All prior Q1 packages complete

### Affected crates / APIs
- All v1 kernel crates
- OpenAPI spec and route table documentation
- `10-master-checklist.md`

### Evidence required
- Full test suite run output (or summary)
- v1.1 gate evidence recorded in `docs/artifacts/<date>/`
- Weak Spot 1: prepare-step rollback_class test passes or risk-accepted
- Weak Spot 2: mark_used at authorize test passes or risk-accepted
- Weak Spot 3: draft-only revalidation test passes or risk-accepted
- Weak Spot 4: lineage chain test passes or risk-accepted
- Route table reconciliation note

### Done criteria (Q1 exit gate)
- All four weak spots have passing integration tests or explicit risk-accepted status in `19-v1-single-node-support-contract.md`
- Route table is reconciled — evaluate endpoint naming consistent across docs/spec/runtime
- OpenAPI spec matches runtime router
- Evidence summary is recorded
- Q2 entry gate is satisfied

### Blockers
- All prior Q1 packages must be done before this starts

### Starting point
- `crates/ferrum-integration-tests/` — run the full integration test suite
- `crates/*/src/` — compile-check all workspace crates for any last-minute regressions
- `docs/artifacts/<date>/` — confirm all prior package evidence notes are present

### Verification
```sh
cargo test --workspace
```
All tests must pass. If any fail, investigate and fix or document as accepted risk before recording exit gate evidence.

Additionally, confirm route-table reconciliation:
```sh
cargo test -p ferrum-gateway
```
and verify OpenAPI spec matches runtime router (compare `openapi/` spec files against `crates/ferrum-gateway/src/server.rs` route names).

### Pack docs to update on close
- `10-master-checklist.md` — mark all Q1 items done or risk-accepted; mark Q1 exit gate passed
- `02-release-plan.md` — record v1.1 gate evidence and link from `docs/artifacts/<date>/`
- `19-v1-single-node-support-contract.md` — confirm all four weak spots are closed or have updated accepted-risk status
- `01-quarterly-plan.md` — mark Q1 Done criteria satisfied
- `docs/artifacts/<date>/` — add full test-suite summary, weak spot closure evidence, and route-table reconciliation note
- `11-current-state-baseline.md` — confirm baseline still accurate after Q1 changes (no new routes or behaviors that contradict the baseline)

### V1 boundary note
> The Q1 exit gate tests the v1 kernel. Accepted risks listed in the v1 support
> contract are the accepted baseline; closing them improves v1 but does not expand
> the support contract. Do not use Q1 results to claim new v1 support features.

### Status annotation — Q1 exit gate closure
> **Q1-P7 — CLOSED (invariant matrix pass / Q1 exit gate):**
> cargo test --workspace passed; cargo test -p ferrum-gateway passed; route parity
> 19/19 confirmed; Q1-P6 adversarial first slices chain (WS1-WS4) passed. This is
> a conservative gate pass for Q1/v1.1 scope only — no v1 scope expansion claimed.
> Q2 entry gate is now satisfied. Evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`.

---

## Cross-Package Dependency Summary

```
Q1-P1 (Proto Shape Lock)
  └── Q1-P2 (Store Integrity)          [G2: P2 → ferrum-cap, ferrum-rollback]
  └── Q1-P3 (PDP Hard-Rules Audit)    [G3: P3 → ferrum-cap mark_used closure]
          │
          │ Gate A: P3 → P4
          │
          └── Q1-P4 (Cap mark_used + Rollback Fix)  [Gate B: P2 → rollback fix]
                  │
                  │ Gate C: P4 → P5
                  │
                  └── Q1-P5 (Gateway Lineage Test)
                          │
                          │ Gate C: P5 → P6
                          │
                          └── Q1-P6 (Adversarial Suite)
                                  │
                                  │ Gate C: P6 → P7
                                  │
                                  └── Q1-P7 (Invariant Matrix Pass) = Q1 EXIT GATE
```
