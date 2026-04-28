# 09 — Phase checklists

Single-node v1 scope. Items marked [DONE], [PARTIAL], [TODO] per `docs/91-phase-success-criteria-and-kpis.md` phase status snapshot.
As of 2026-04-28 all P0/P1/P2 items are closed. Phase D adapters remain partial: fs has verified local slices for FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod (135 tests), git has verified local slices for GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete (86 tests), http has a verified first prepare/verify slice with PUT/PATCH replay (103 tests), sqlite has a transaction-based rollback implementation (16 tests), maildraft has full lifecycle implementation (13 tests: create/update/delete), and broader adapter completion is post-v1 backlog. Tier 2 complete: ferrum-ledger SHA-256 hash chain (13 tests), ferrum-firewall TaintScoringFirewall (21 tests), ferrum-graph HashMap adjacency + BFS traversal (10 tests).

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
- [PARTIAL] fs adapter (prepare/verify/execute/rollback for FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod; FileWrite/FileDelete: snapshot-recovery for existing files via deterministic snapshot paths, new-file FileWrite with cleanup-on-rollback; **FileMove**: prepare snapshots source, execute renames source→dest, verify checks dest exists + source absent, rollback moves dest back with hash verification; **FileCopy**: prepare captures source hash, execute copies with dest snapshotting, verify checks dest hash match, rollback idempotently deletes/restores; **DirCreate**: validate parent + dir absent, mkdir, verify dir exists, rollback removes created dir; **DirDelete**: reject non-empty, rm empty dir, verify dir gone, rollback recreates dir; **FileAppend**: prepare captures original hash + length, execute appends data, verify confirms growth, rollback truncates with hash verification; **FileChmod**: prepare captures current permissions, execute changes mode bits, verify confirms new permissions, rollback restores original mode with verification; explicit checks fail-closed, phase-aware validation; 135 tests passing)
- [PARTIAL] git adapter (local rollback/recovery implementation: prepare captures HEAD ref, rollback resets hard with dirty-worktree guard, verify checks ref matches, execute captures after_ref, GitBranchCreate support with branch creation/deletion, base_ref validation/resolution, prepare-time rejection of existing branches and detached-HEAD-without-explicit-base, branch-name validation during prepare using git-native `git check-ref-format --branch` (fail-closed), verify fail-closed when the created branch is currently checked out, detached-HEAD / safe-delete fail-closed guards, and P2.3 hardening for implicit HEAD base_ref_sha persistence plus enriched verify audit metadata; **GitTagCreate**: tag name validation via `check-ref-format`, reject existing tag, create lightweight tag at HEAD, verify confirms existence, rollback idempotent delete; **GitTagDelete**: reject missing tag, capture tag_sha during prepare, execute deletes tag, verify confirms gone, rollback recreates at captured SHA with hash verification; resolve_ref_to_commit_sha added for annotated tag compatibility; **GitBranchDelete**: safe branch deletion with recreate rollback (prepare captures branch SHA + current HEAD, execute deletes branch, verify confirms deletion, rollback recreates branch at captured SHA); 86 tests passing)
- [PARTIAL] sqlite adapter (transaction-based rollback implementation; not production-verified; 16 tests passing)
- [DONE] maildraft adapter (full MailDraftRollbackAdapter with create/update/delete lifecycle; 13 tests passing)
- [PARTIAL] http adapter (bounded HttpMutation prepare/execute/verify with method-aware `HttpStatusExpected` checks and `expected_statuses` array support; execute sends real requests and captures request/response metadata + digests plus digest-only `rollback_groundwork_v1` and `http_recovery_readiness_v1` groundwork metadata; bounded replay-based recovery is supported for POST/PUT/PATCH via strict one-step `http.replay_v1` + exact URL/digest binding + `Idempotency-Key` transport + required strict `expected_statuses`; unsupported shapes fail closed with structured reasons; 103 tests passing)
- [DONE] rollback/compensate service (via NoopRollbackAdapter for integration tests)
- Note: Real adapter implementations beyond current fs/git/http slices are post-v1 backlog per `docs/implementation-path/11-remaining-tasks.md` P3.

## Phase E — Gateway Orchestration
- [DONE] gateway calls pdp
- [DONE] gateway calls cap
- [DONE] gateway calls rollback
- [DONE] gateway emits provenance
- [DONE] evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate flow (commit/rollback are internal orchestration semantics; compensate is the exposed v1 recovery endpoint)
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
