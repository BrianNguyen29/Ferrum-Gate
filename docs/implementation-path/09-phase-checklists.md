# 09 — Phase checklists

Single-node v1 scope. Items marked [DONE], [PARTIAL], [TODO] per `docs/91-phase-success-criteria-and-kpis.md` phase status snapshot.
As of 2026-03-29 all P0/P1/P2 items are closed. Phase D adapters are skeleton-only (partial); real implementations are post-v1 backlog.

## Phase A — Compile and Shape Stability
- [DONE] cargo check pass
- [DONE] bins build
- [DONE] no missing modules

## Phase B — Storage Boundary
- [DONE] store traits
- [DONE] sqlite implementation
- [DONE] persist core objects

## Phase C — Firewall MVP
- [DONE] trust labels
- [DONE] taint scoring
- [DONE] sanitize
- [DONE] contradiction checks
- [DONE] poisoned context regression fixtures (6 curated fixtures)

## Phase D — Adapter-backed Rollback
- [PARTIAL] fs adapter (skeleton exists; real implementation post-v1)
- [PARTIAL] sqlite adapter (skeleton exists; real implementation post-v1)
- [PARTIAL] maildraft adapter (skeleton exists; real implementation post-v1)
- [DONE] rollback/compensate service (via NoopRollbackAdapter for integration tests)
- Note: Real adapter implementations are post-v1 backlog per `docs/implementation-path/11-remaining-tasks.md` P3.

## Phase E — Gateway Orchestration
- [DONE] gateway calls pdp
- [DONE] gateway calls cap
- [DONE] gateway calls rollback
- [DONE] gateway emits provenance
- [DONE] evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate flow (compensate is the primary recovery endpoint; commit and rollback routes are also exposed)
- [DONE] negative paths: deny, quarantine, RequireApproval, draft-only gated at evaluate (before prepare)
- [DONE] scope mismatch deny (explicit scope-bounds check implemented in PDP — P0 resolved)

## Phase F — Hardening, Evidence, and Integration Readiness
- [DONE] happy path test (compensate_execution_flow end-to-end)
- [DONE] deny test (implicit via StaticPdpEngine default Allow/Deny)
- [DONE] quarantine test (test_high_taint_triggers_quarantine)
- [DONE] rollback test (test_rollback_and_compensate_are_distinct_operations)
- [DONE] poisoned context test (6 curated regression fixtures — P1 resolved)
- [DONE] final docs pack (implementation-path docs finalized as cohesive Phase F pack — P1 resolved)
- [DONE] supported flows list (Phase F evidence — `docs/implementation-path/11-remaining-tasks.md` P1)
- [DONE] open gaps list (Phase F evidence — `docs/implementation-path/11-remaining-tasks.md` P1)
