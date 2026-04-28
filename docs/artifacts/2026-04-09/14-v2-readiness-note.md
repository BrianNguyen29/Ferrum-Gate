# v2 (Q2) Readiness Note — 2026-04-11

**Date:** 2026-04-11
**Scope:** Q2/v2 adapter completion — current status and concrete next steps
**Purpose:** Operator-facing readiness assessment and next-step plan for Q2 completion

## v2 Status Summary

v2 (Q2 — Governed Engineering Changes Beta) is **not yet complete**. This note records what is confirmed, what remains open, and the concrete path to v2 completion.

### Confirmed (as of 2026-04-11)

| Item | Status | Evidence |
|------|--------|----------|
| Gate E/G4 — fs-first FileWrite store+adapter persistence | CONFIRMED | `10-q2-fs-foundation-evidence.md` — 7 integration tests + 3 unit tests pass |
| HTTP compensate endpoint (fs-first) | CONFIRMED | `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035) |
| HTTP execute endpoint (fs-first, state-guarded) | CONFIRMED | `test_execute_and_verify_endpoint_flow_for_file_write` (integration_gateway_flow.rs:4056–4427); `11-gateway-execute-verify-surface-design-note.md` |
| HTTP verify endpoint (fs-first, state-guarded) | CONFIRMED | Same test as above; state guard tests in `12-gateway-execute-verify-invalid-state-409-evidence.md` |
| 409 invalid-state guard coverage (4 tests) | CONFIRMED | `12-gateway-execute-verify-invalid-state-409-evidence.md` |

### Not Yet Confirmed

| Item | Status |
|------|--------|
| Git adapter (ferrum-adapter-git) — real before_ref/after_ref/revert path | NOT STARTED |
| SQLite adapter (ferrum-adapter-sqlite) — real transaction/verify/rollback | NOT STARTED |
| Gate F — all three adapters real → gateway integration | NOT STARTED |
| Gate G — gateway + policy packs → fs+db verify/compensate demo | NOT STARTED |
| v2 exit gate — end-to-end demo running, verify+compensate demonstrable | NOT STARTED |

## Gate E/G4 — Partial Status

Gate E (ferrum-store adapter artifact persistence → adapter crates) is **partially satisfied**:

- **Satisfied:** fs-first FileWrite slice — prepare → persist → execute → verify → compensate/restore all confirmed at store+adapter layer
- **Not satisfied:** git and sqlite adapter slices are not implemented; full Gate E (all three adapters) is open

The master checklist (`10-master-checklist.md`) reflects this partial status with explicit "partial" annotations.

## v2 Completion — Concrete Next-Step Plan

The following ordered steps close the remaining Q2 gate requirements:

### Step 1: Git Adapter Real Path (Gate F prerequisite)
- **What:** Implement `FsAdapter`-equivalent lifecycle for git — `before_ref` snapshot, `after_ref` capture, `revert-reset` restore
- **Location:** `crates/ferrum-adapter-git/src/`
- **Exit criterion:** At least one integration test showing real git revert-reset via gateway HTTP surface
- **Evidence file target:** New evidence note in `docs/artifacts/2026-04-09/` documenting the git adapter lifecycle

### Step 2: SQLite Adapter Real Path (Gate F prerequisite)
- **What:** Implement transaction wrapper, verify predicate, rollback for sqlite
- **Location:** `crates/ferrum-adapter-sqlite/src/`
- **Exit criterion:** At least one integration test showing real sqlite transaction rollback via gateway HTTP surface
- **Evidence file target:** New evidence note in `docs/artifacts/2026-04-09/` documenting the sqlite adapter lifecycle

### Step 3: Gateway Integration — All Three Adapters (Gate F)
- **What:** Integrate git and sqlite adapters into `ferrum-gateway` orchestration; add policy packs for fs/git/db engineering workflows
- **Location:** `crates/ferrum-gateway/src/server.rs` (routes), `crates/ferrum-gateway/src/policy/`
- **Exit criterion:** Gateway routes accept all three adapter types; policy templates exist for fs, git, and db workflows
- **Evidence file target:** Updated `11-gateway-execute-verify-surface-design-note.md` with multi-adapter scope

### Step 4: End-to-End Demo — fs + db Verify/Compensate (Gate G / v2 exit gate)
- **What:** Operator-visible execution + lineage trace for fs mutation; demonstrate verify + compensate/rollback on a real workload
- **Exit criterion:** End-to-end demo runs; verify and compensate path demonstrable for fs + db
- **Evidence file target:** New evidence note documenting the demo run with operator-readable output

## What This Note Is NOT

- **Not a roadmap rewrite** — the quarterly plan (`01-quarterly-plan.md`) and master checklist (`10-master-checklist.md`) are the authoritative scope documents
- **Not a gate claim** — no Q2 exit gate is claimed; git/sqlite adapters are NOT started
- **Not a retro** — this is forward-looking; focused on what remains and what the concrete next steps are

## Relationship to Existing Bundle

This note is consistent with and supplements:
- `10-q2-fs-foundation-evidence.md` (foundation slice confirmed, partial scope acknowledged)
- `11-gateway-execute-verify-surface-design-note.md` (current surface confirmed)
- `12-gateway-execute-verify-invalid-state-409-evidence.md` (409 guard coverage confirmed)
- `10-master-checklist.md` (Q2 items marked partial, next steps explicit)

## Manifest Entry

Once complete, this note should be indexed in `manifest.txt` as an entry under the Q2/v2 section.