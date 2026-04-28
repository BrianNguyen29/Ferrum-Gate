# 14 — Q2 Work Packages

## Purpose

This document provides **execution-ready work packages** for FerrumGate Q2 governed
engineering changes beta. Each package is sized for one engineer or one agent to
pick up directly.

**Q2 is explicitly post-v1 scope.** All packages in this document describe
post-v1 deliverables. The v1 support contract (`19-v1-single-node-support-contract.md`)
confirms all adapters are skeleton-only in v1. "Done when" criteria describe
the target post-v1 state, not the current v1 support baseline.

**Q2 must not begin until the v1.1 exit gate is passed.** Evidence of gate pass
is required before Q2 work is treated as committed. Record gate evidence in
`docs/artifacts/<date>/` and link from `02-release-plan.md` v1.1 gate section.

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
For adapter-level integration or multi-crate verification, see the per-package `Verification` field.

---

## Package map

| # | Package | Steps | Gate |
|---|---|---|---|
| Q2-P1 | Proto Type Extension for Adapter Payloads | 2.1 | Gate D |
| Q2-P2 | Store Adapter Artifact Persistence | 2.2 | Gate E |
| Q2-P3 | ferrum-adapter-fs Implementation | 2.3 | Gate E |
| Q2-P4 | ferrum-adapter-git Implementation | 2.4 | Gate E |
| Q2-P5 | ferrum-adapter-sqlite Implementation | 2.5 | Gate E |
| Q2-P6 | Gateway Orchestration Integration | 2.6 | Gate F |
| Q2-P7 | Policy Packs for Engineering Workflows | 2.7 | Gate G |
| Q2-P8 | End-to-End Demo (fs + db verify/compensate) | 2.8 | Gate G |

---

## Q2-P1 — Proto Type Extension for Adapter Payloads

### Objective
Extend `ferrum-proto` types to cover fs/git/db adapter payloads — the data
shapes needed to describe file targets, git refs, and SQL statements as
governed action parameters.

### Inputs
- Current `ferrum-proto` domain objects
- Adapter payload requirements from `05-adapter-roadmap.md` sections 1–3
- v1.1 gate evidence (Gate D entry precondition)

### Outputs
- Proto message definitions for: file target (path, hash, snapshot), git target (ref, before_ref, after_ref, repo), db target (statement, predicate, table, rowset)
- Proto enum for adapter action type (backup, restore, commit, rollback, revert, etc.)
- Proto message for adapter artifact metadata (storage key, hash, timestamp)

### Dependencies
- v1.1 exit gate evidence (Gate D entry precondition — Q1 exit gate must be passed)

### Affected crates / APIs
- `ferrum-proto` — new adapter payload types
- `ferrum-store` — uses adapter artifact metadata type for persistence
- `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite` — consume these types
- `ferrum-gateway` — uses these types for adapter orchestration

### Evidence required
- Gate D evidence: v1.1 exit gate test output or artifact note
- Proto diff showing new adapter payload types added
- Proto file compiles without errors across all crates

### Done criteria
- Adapter payload types exist in `ferrum-proto`
- Downstream crates can import and use these types without circular dependency
- Proto types are considered stable for Q2 adapter implementation work

### Blockers
- Gate D: v1.1 exit gate must be passed before this starts

### Starting point
- `crates/ferrum-proto/src/` — existing domain object definitions; add adapter payload messages here
- `05-adapter-roadmap.md` — sections 1–3 define the required adapter payload shapes for reference

### Verification
```sh
cargo check --package ferrum-proto --workspace
cargo test --package ferrum-proto
```
Confirm adapter payload types compile and downstream crates (`ferrum-store`, `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite`, `ferrum-gateway`) all compile with the new types.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate D (v1.1 → proto extension) done
- `01-quarterly-plan.md` — record Gate D evidence in the Q2 Evidence table
- `docs/artifacts/<date>/` — add proto diff or note confirming adapter payload types added and compile cleanly

### V1 boundary note
> This package extends proto types for post-v1 adapter work. Proto additions
> for adapters do not affect the v1 kernel. Do not backport adapter payload
> types to v1 scope without a formal v1 contract amendment.

---

## Q2-P2 — Store Adapter Artifact Persistence

### Objective
Add `ferrum-store` support for persisting adapter-specific artifacts needed for
fs/git/db verify and restore operations.

### Inputs
- `ferrum-store/src/` — current store trait and implementation
- `ferrum-proto` adapter artifact metadata type (Q2-P1)
- G4 gate: store must persist adapter artifacts before adapter integration

### Outputs
- Store schema or trait extension for adapter artifact persistence
- Methods to save/retrieve adapter artifacts keyed by execution or lineage ID
- Query paths for artifact retrieval needed by adapter verify/restore

### Dependencies
- Q2-P1 (proto type extension) complete — Gate E

### Affected crates / APIs
- `ferrum-store` — new artifact persistence methods
- `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite` — depend on artifact storage
- `ferrum-gateway` — orchestrates artifact persistence via store

### Evidence required
- Gate E evidence: proto extension is complete (from Q2-P1)
- Store artifact persistence has at least unit-level test
- Code reference or test showing artifacts can be saved and retrieved

### Done criteria
- Store can persist adapter artifacts (backup snapshot, ref snapshot, transaction savepoint)
- Store can retrieve artifacts by execution or lineage ID
- At least one adapter can use the persistence path in a test

### Blockers
- Gate E: Q2-P1 must complete before this starts

### Starting point
- `crates/ferrum-store/src/` — examine existing store trait and schema; add artifact persistence methods
- `crates/ferrum-proto/src/` — reference the adapter artifact metadata type from Q2-P1

### Verification
```sh
cargo test --package ferrum-store
```
Confirm at least one test covers saving and retrieving an adapter artifact by execution or lineage ID. If no such test exists, add a unit test under `crates/ferrum-store/tests/` (or `crates/ferrum-store/src/`).

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate E (store adapter artifact persistence) done
- `01-quarterly-plan.md` — record Gate E evidence in the Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference confirming artifact persistence path works

### V1 boundary note
> Adapter artifact persistence is post-v1. `ferrum-store` in v1 does not have
> adapter artifact persistence. This package is Q2 scope.

---

## Q2-P3 — ferrum-adapter-fs Implementation

### Objective
Implement the filesystem adapter with real backup / hash verify / restore semantics.

### Inputs
- `ferrum-adapter-fs/src/` — current adapter skeleton
- Proto types from Q2-P1
- Store artifact persistence from Q2-P2
- `05-adapter-roadmap.md` Adapter 1 checklist

### Outputs
- Real `prepare`: capture pre-mutate backup snapshot
- Real `execute`: perform file mutation within scoped path
- Real `verify`: hash comparison before/after
- Real `compensate`: restore from backup snapshot
- Path allowlist/scope enforcement
- Integration tests for success, verify fail, restore fail, path deny

### Dependencies
- Q2-P2 (store artifact persistence) complete — Gate E
- Q2-P1 (proto types) complete — Gate E

### Affected crates / APIs
- `ferrum-adapter-fs` — adapter implementation
- `ferrum-store` — artifact storage
- `ferrum-proto` — target types
- `ferrum-gateway` — calls adapter methods
- `ferrum-cap` — path-scoped capability binding for fs operations

### Evidence required
- Gate E evidence: store supports adapter artifact persistence
- Test output for backup + hash + restore path
- Test for path deny (capability scope enforcement)
- Test for verify failure path

### Done criteria
- fs adapter has real backup / hash verify / restore path
- prepare/execute/verify/compensate all have real implementations (not noop)
- Integration tests pass for the full path
- Policy pack can bind a capability to a file path scope

### Blockers
- Gate E: Q2-P2 must complete before this starts

### Starting point
- `crates/ferrum-adapter-fs/src/` — existing adapter skeleton; implement real prepare/execute/verify/compensate methods
- `05-adapter-roadmap.md` Adapter 1 checklist — checklist items to work through
- `crates/ferrum-store/src/` — confirm artifact persistence API to use for backup snapshots

### Verification
```sh
cargo test --package ferrum-adapter-fs
```
Integration-level verification requires gateway + store:
```sh
cargo test -p ferrum-integration-tests --test integration -- fs
```
Confirm tests pass for: backup + hash + restore path; path deny (capability scope enforcement); verify failure path.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate E satisfied and fs adapter implementation done
- `01-quarterly-plan.md` — record fs adapter implementation evidence in Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference for backup/restore path and path deny test

### V1 boundary note
> `ferrum-adapter-fs` is listed as explicitly unsupported in the v1 support
> contract. All work in this package is post-v1 scope. Do not claim fs adapter
> is v1-supported regardless of any implementation code present in the repo.

---

## Q2-P4 — ferrum-adapter-git Implementation

### Objective
Implement the git adapter with real before_ref / after_ref / revert-reset semantics.

### Inputs
- `ferrum-adapter-git/src/` — current adapter skeleton
- Proto types from Q2-P1
- Store artifact persistence from Q2-P2
- `05-adapter-roadmap.md` Adapter 2 checklist

### Outputs
- Real `prepare`: capture before_ref snapshot
- Real `execute`: perform git mutation (commit, branch, reset)
- Real `verify`: diff and ref movement verification
- Real `compensate`: revert/reset to before_ref
- Protected branch enforcement
- Integration tests for ref mismatch, verify failure, protected branch deny

### Dependencies
- Q2-P2 (store artifact persistence) complete — Gate E
- Q2-P1 (proto types) complete — Gate E

### Affected crates / APIs
- `ferrum-adapter-git` — adapter implementation
- `ferrum-store` — artifact storage
- `ferrum-proto` — target types
- `ferrum-gateway` — calls adapter methods
- `ferrum-cap` — ref-scoped capability binding for git operations

### Evidence required
- Gate E evidence: store supports adapter artifact persistence
- Test output for before_ref/after_ref + revert path
- Test for protected branch enforcement

### Done criteria
- git adapter has real before_ref / after_ref / revert path
- prepare/execute/verify/compensate all have real implementations (not noop)
- Protected branch rules are enforced
- Integration tests pass for the full path

### Blockers
- Gate E: Q2-P2 must complete before this starts

### Starting point
- `crates/ferrum-adapter-git/src/` — existing adapter skeleton; implement real before_ref/after_ref/revert/reset methods
- `05-adapter-roadmap.md` Adapter 2 checklist — checklist items to work through

### Verification
```sh
cargo test --package ferrum-adapter-git
```
Integration-level verification:
```sh
cargo test -p ferrum-integration-tests --test integration -- git
```
Confirm tests pass for: before_ref/after_ref capture + revert path; protected branch enforcement; ref mismatch and verify failure paths.

### Pack docs to update on close
- `10-master-checklist.md` — mark git adapter implementation done
- `01-quarterly-plan.md` — record git adapter implementation evidence in Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference for before_ref/after_ref + revert path and protected branch enforcement

### V1 boundary note
> `ferrum-adapter-git` is listed as explicitly unsupported in the v1 support
> contract. All work in this package is post-v1 scope. Do not claim git adapter
> is v1-supported regardless of any implementation code present in the repo.

---

## Q2-P5 — ferrum-adapter-sqlite Implementation

### Objective
Implement the SQLite adapter with real transaction wrapper / verify predicate / rollback semantics.

### Inputs
- `ferrum-adapter-sqlite/src/` — current adapter skeleton
- Proto types from Q2-P1
- Store artifact persistence from Q2-P2
- `05-adapter-roadmap.md` Adapter 3 checklist

### Outputs
- Real `prepare`: open transaction boundary or savepoint
- Real `execute`: run SQL statement(s) within transaction
- Real `verify`: predicate check or row count verification
- Real `compensate`: rollback transaction
- SQL mutation class classification (safe/high-risk/destructive)
- Integration tests for row mismatch, verify failure, partial failure

### Dependencies
- Q2-P2 (store artifact persistence) complete — Gate E
- Q2-P1 (proto types) complete — Gate E

### Affected crates / APIs
- `ferrum-adapter-sqlite` — adapter implementation
- `ferrum-store` — artifact storage
- `ferrum-proto` — target types
- `ferrum-gateway` — calls adapter methods
- `ferrum-cap` — table-scoped capability binding for SQL operations
- `ferrum-pdp` — mutation class risk mapping

### Evidence required
- Gate E evidence: store supports adapter artifact persistence
- Test output for transaction wrapper + rollback
- Test for verify failure with rollback
- Test for mutation class classification

### Done criteria
- sqlite adapter has real transaction wrapper / verify predicate / rollback path
- prepare/execute/verify/compensate all have real implementations (not noop)
- Mutation risk classification is enforced
- Integration tests pass for the full path

### Blockers
- Gate E: Q2-P2 must complete before this starts

### Starting point
- `crates/ferrum-adapter-sqlite/src/` — existing adapter skeleton; implement real transaction/verify/rollback methods
- `05-adapter-roadmap.md` Adapter 3 checklist — checklist items to work through
- `crates/ferrum-pdp/src/` — review mutation class risk mapping if used by the adapter

### Verification
```sh
cargo test --package ferrum-adapter-sqlite
```
Integration-level verification:
```sh
cargo test -p ferrum-integration-tests --test integration -- sqlite
```
Confirm tests pass for: transaction wrapper + rollback; verify failure with rollback; mutation class classification.

### Pack docs to update on close
- `10-master-checklist.md` — mark sqlite adapter implementation done
- `01-quarterly-plan.md` — record sqlite adapter implementation evidence in Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference for transaction/rollback path and mutation classification

### V1 boundary note
> `ferrum-adapter-sqlite` is listed as explicitly unsupported in the v1 support
> contract. Compensate may be noop-backed in v1. This package is Q2 post-v1 scope.
> Do not claim db adapter is v1-supported regardless of any implementation code
> present in the repo.

---

## Q2-P6 — Gateway Orchestration Integration

### Objective
Integrate the three real adapter implementations into `ferrum-gateway` orchestration.
The gateway becomes the single valid entry point for adapter-backed mutation flows.

### Inputs
- Real fs adapter from Q2-P3
- Real git adapter from Q2-P4
- Real sqlite adapter from Q2-P5
- G5 gate: all three adapters must have real implementations before gateway integration

### Outputs
- Gateway routes that call fs/git/sqlite adapter methods in correct sequence
- Gateway passes adapter artifacts to store for persistence
- Gateway emits provenance events for adapter operations
- Adapter integration tests showing full prepare/execute/verify/compensate via gateway

### Dependencies
- Q2-P3 (fs adapter) complete — Gate F
- Q2-P4 (git adapter) complete — Gate F
- Q2-P5 (sqlite adapter) complete — Gate F

### Affected crates / APIs
- `ferrum-gateway` — orchestration layer wiring all three adapters
- `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite` — called by gateway
- `ferrum-store` — called by gateway for artifact persistence
- `ferrum-graph` — receives provenance events from gateway
- API: execution inspect endpoint enriched with adapter artifact summary

### Evidence required
- Gate F evidence: all three adapters have real implementations (test output or code reference)
- Integration test showing gateway calls fs adapter and persists artifact
- Integration test showing gateway calls git adapter and persists artifact
- Integration test showing gateway calls sqlite adapter and persists artifact
- Gateway lineage trace includes adapter operation events

### Done criteria
- Gateway orchestrates all three adapters with real implementations
- Gateway can drive fs + git + sqlite through prepare/execute/verify/compensate
- Operator can inspect execution and see adapter artifact summary
- Provenance chain includes adapter events

### Blockers
- Gate F: all three adapters must be real-implemented before this starts

### Starting point
- `crates/ferrum-gateway/src/` — examine `build_router_core` or equivalent to understand current routing; add adapter-backed routes and orchestration logic
- `crates/ferrum-adapter-fs/src/`, `crates/ferrum-adapter-git/src/`, `crates/ferrum-adapter-sqlite/src/` — confirm all three have real implementations ready to wire

### Verification
```sh
cargo test --package ferrum-gateway
cargo test -p ferrum-integration-tests --test integration -- gateway
```
Confirm gateway can drive fs, git, and sqlite through prepare/execute/verify/compensate in integration tests. Verify provenance chain includes adapter operation events.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate F (gateway orchestration) done
- `01-quarterly-plan.md` — record Gateway F evidence in Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference showing gateway → adapter wiring and provenance events

### V1 boundary note
> Gateway orchestration for real adapters is post-v1. The v1 router does not
> include adapter-backed routes. This package is Q2 scope.

---

## Q2-P7 — Policy Packs for Engineering Workflows

### Objective
Create policy packs that bind PDP rules to the three adapter types. These packs
provide reusable policy templates for fs/git/db engineering workflows.

### Inputs
- PDP rules from `ferrum-pdp/`
- Adapter implementations from Q2-P3, Q2-P4, Q2-P5
- Gateway orchestration from Q2-P6
- G6 gate: gateway orchestration must be ready before policy pack work

### Outputs
- Policy pack for fs: path allowlist, destructive mutation rules, scope subset validation
- Policy pack for git: protected branch rules, ref scope constraints, destructive mutation rules
- Policy pack for db: table allowlist, mutation class rules, high-risk statement rules
- Policy pack loading mechanism or documented manual registration

### Dependencies
- Q2-P6 (gateway orchestration) complete — Gate G

### Affected crates / APIs
- `ferrum-pdp` — policy engine
- `ferrum-cap` — capability bindings for fs/git/db scopes
- `ferrum-gateway` — consults PDP for every adapter-backed execution
- `ferrumctl` — may expose policy pack inspect commands

### Evidence required
- Gate G evidence: gateway orchestration is ready
- PDP policy pack for fs with at least one rule test
- PDP policy pack for git with at least one rule test
- PDP policy pack for db with at least one rule test

### Done criteria
- At least fs and db policy packs exist with test coverage
- Git policy pack exists with protected branch enforcement
- Policy packs are documented or discoverable by an operator

### Blockers
- Gate G: Q2-P6 must complete before this starts

### Starting point
- `crates/ferrum-pdp/src/` — existing PDP rules; add fs/git/db policy pack modules
- `crates/ferrum-cap/src/` — review existing capability bindings to understand how to add path/ref/table-scoped bindings

### Verification
```sh
cargo test --package ferrum-pdp
```
Confirm tests exist for at least fs and db policy pack rules. If no tests exist yet, add test cases covering: path allowlist enforcement, destructive mutation blocking, protected branch enforcement.

### Pack docs to update on close
- `10-master-checklist.md` — mark Gate G (policy packs) done
- `01-quarterly-plan.md` — record policy pack evidence in Q2 Evidence table
- `docs/artifacts/<date>/` — add test output or code reference for fs, git, and db policy pack rules

### V1 boundary note
> Engineering workflow policy packs are post-v1 scope. The v1 PDP does not
> include fs/git/db adapter policy packs. This package is Q2 scope.

---

## Q2-P8 — End-to-End Demo (fs + db verify/compensate)

### Objective
Demonstrate a real fs mutation and a real db mutation through the full gateway
pipeline, with verify and compensate/rollback working on a realistic workload.

### Inputs
- Gateway orchestration (Q2-P6)
- Policy packs (Q2-P7)
- fs adapter (Q2-P3)
- sqlite adapter (Q2-P5)
- G6 gate: both gateway orchestration and policy packs must be ready before demo

### Outputs
- Runnable end-to-end demo for fs mutation: prepare → execute → verify → compensate
- Runnable end-to-end demo for db mutation: prepare → execute → verify → rollback
- Operator-visible execution trace showing decision + lineage for both demos
- Demo scripts or documentation so design partners can replicate

### Dependencies
- Q2-P6 (gateway orchestration) complete — Gate G
- Q2-P7 (policy packs) complete — Gate G

### Affected crates / APIs
- `ferrum-gateway` — demo runs through gateway
- `ferrum-adapter-fs` — fs demo
- `ferrum-adapter-sqlite` — db demo
- `ferrum-graph` — lineage trace visible in demo output
- `ferrum-proto` / `ferrum-store` — underlying types and storage

### Evidence required
- Gate G evidence: gateway orchestration and policy packs are ready
- Demo trace showing fs mutation with verify + compensate path
- Demo trace showing db mutation with verify + rollback path
- Operator-visible execution + lineage for both demos
- Evidence recorded in `docs/artifacts/<date>/`

### Done criteria (Q2 exit gate)
- End-to-end fs demo runs with real verify + compensate
- End-to-end db demo runs with real verify + rollback
- Operator can inspect execution and lineage for both demos
- Q2 exit gate is satisfied: verify and compensate path demonstrable for fs + db

### Blockers
- Gate G: Q2-P6 and Q2-P7 must complete before this starts

### Starting point
- `crates/ferrum-integration-tests/tests/` — create or extend an end-to-end demo test that exercises fs and db adapter paths through the gateway
- `crates/ferrum-gateway/src/` — confirm gateway routes for adapter-backed executions are wired and emitting provenance events

Run the demo manually to verify operator-visible output:
```sh
cargo run -p ferrumd &
# Then exercise the demo path via ferrumctl or HTTP client
```

### Verification
```sh
cargo test -p ferrum-integration-tests --test integration --
```
Confirm end-to-end tests pass for fs mutation (prepare→execute→verify→compensate) and db mutation (prepare→execute→verify→rollback). Verify execution inspect and lineage endpoints return adapter artifact summary.

If creating a demo script rather than a test, run it manually and record the output:
```sh
# Example demo invocation pattern
cargo run -p ferrumctl -- server execute-adapter-demo --adapter fs --target /tmp/test-file
```

### Pack docs to update on close
- `10-master-checklist.md` — mark Q2 exit gate passed
- `02-release-plan.md` — record Q2 exit gate evidence and link from `docs/artifacts/<date>/`
- `01-quarterly-plan.md` — confirm Q2 Done criteria satisfied
- `docs/artifacts/<date>/` — add demo trace output showing fs verify+compensate and db verify+rollback; add operator-visible execution + lineage for both demos
- `11-current-state-baseline.md` — confirm baseline still accurate after Q2 changes (adapters now have real implementations)

### V1 boundary note
> End-to-end demos for fs and db adapters are post-v1. The demos exercise
> post-v1 adapter implementations. Do not present these demos as v1 capabilities.

---

## Cross-Package Dependency Summary

```
v1.1 EXIT GATE (Q1 exit gate)
  │
  │  Gate D
  │
  └── Q2-P1 (Proto Type Extension)
          │
          │  Gate E
          │
          ├── Q2-P2 (Store Adapter Artifact Persistence)
          │       │
          │       │  Gate E
          │       │
          │       ├── Q2-P3 (fs adapter) ──────────────────┐
          │       ├── Q2-P4 (git adapter) ────────────────┼── Gate F
          │       └── Q2-P5 (sqlite adapter) ──────────────┘
          │               │
          │               │  Gate F
          │               │
          │               └── Q2-P6 (Gateway Orchestration)
          │                       │
          │                       │  Gate G
          │                       │
          │                       ├── Q2-P7 (Policy Packs)
          │                       └── Q2-P8 (End-to-End Demo) = Q2 EXIT GATE
          │
          │  (P3/P4/P5 can run in parallel after Gate E)
          │  (P6 requires all three adapters — Gate F)
          │  (P7/P8 require P6 — Gate G)
```

## Q2 Exit Gate Criteria

Q2 is done when:
- fs adapter has real backup / hash verify / restore path
- git adapter has real before_ref / after_ref / revert path
- sqlite adapter has real transaction wrapper / verify predicate / rollback
- Policy packs exist for fs and db engineering workflows
- End-to-end demo shows verify + compensate working on fs mutation
- End-to-end demo shows verify + rollback working on db mutation
- Operator can inspect execution and lineage for all three adapter types
- Evidence recorded in `docs/artifacts/<date>/`
