# Q2 fs-first Foundation Slice — Gate E/G4 Entry Evidence

**Date:** 2026-04-09
**Updated:** 2026-04-10 to reflect stronger gateway-facing fs-first slice
**Scope:** Foundation slice — NOT full Q2 completion

## Stronger gateway-facing fs-first slice (2026-04-10 update)

The gateway-facing fs-first slice for FileWrite is now:

**HTTP prepare → persisted contract → HTTP compensate restore**

Specifically:
- `POST /v1/executions/{id}/prepare` creates the snapshot and persists the contract
- Contract includes `compensation_plan` with `fs/restore_snapshot` step
- `POST /v1/executions/{id}/compensate` retrieves the contract and invokes FsAdapter compensate path, restoring the file

Evidence: `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035) exercises the full HTTP-level compensate path end-to-end including the FsAdapter restore.

**Execute and verify HTTP surfaces now exist.** The gateway router (server.rs:155–162) exposes `POST /v1/executions/{id}/execute` and `POST /v1/executions/{id}/verify` endpoints with explicit state guards:
- Execute: requires contract=`Prepared`, execution=`Prepared`/`Authorized`/`Proposed`
- Verify: requires contract=`ExecutedAwaitingVerify`, execution=`Running`/`AwaitingVerification`
- Both return 409 Conflict for invalid state transitions
- The fs adapter's execute and verify paths are exercised both via HTTP endpoints and via integration tests calling the adapter directly.

## What was demonstrated

FileWrite (fs-first) contract now covers the full prepare→persist→retrieve→execute→verify→rollback lifecycle through `SqliteRollbackRepo`, including state transitions, `verify_checks`, and `compensation_plan` paths — all via integration tests in `crates/ferrum-integration-tests/src/integration_fs_roundtrip.rs`.

## Evidence

Six passing integration tests in `crates/ferrum-integration-tests/src/integration_fs_roundtrip.rs`:

| Test | Lines | What it shows |
|------|-------|---------------|
| `test_fs_filewrite_prepare_persist_retrieve_execute_verify_rollback` | 160–287 | Full lifecycle: prepare captures snapshot, contract persisted and retrieved, execute writes new content, verify passes, rollback restores original; snapshot_path survives SQLite round-trip |
| `test_fs_filewrite_newfile_metadata_persists_through_store` | 290–404 | New-file case (no snapshot): `created_new_file=true` metadata round-trips through store; execute creates file; verify passes; rollback deletes file |
| `test_fs_filewrite_state_transitions_persist_through_store` | 554–632 | State transitions (Prepared → ExecutedAwaitingVerify → Verified) survive through `update()` and retrieval via `SqliteRollbackRepo` |
| `test_fs_filewrite_verify_checks_are_exercised` | 457–548 | `verify_checks` (FileHashMatches) are stored in contract and exercised during verify phase post-retrieval |
| `test_fs_filewrite_compensation_plan_exercises_rollback` | 638–736 | `compensation_plan` with fs/rollback step is persisted and exercised via `compensate()` path; file restored to original |
| `test_fs_filewrite_compensation_plan_persists_through_store` | 739–827 | `compensation_plan` (multi-step) round-trips through `SqliteRollbackRepo` with order and idempotency_key intact |
| `test_fs_filewrite_verify_checks_persist_and_are_exercised` | 830–913 | `verify_checks` survive store round-trip and are exercised post-retrieval with matching content |

Plus three passing unit tests in `crates/ferrum-store/src/sqlite/rollback.rs` (metadata-only slice):

| Test | Lines | What it shows |
|------|-------|---------------|
| `test_rollback_contract_metadata_round_trips_through_store` | 279–364 | fs-prepare metadata inserted and retrieved; `adapter_kind=ferrum-adapter-fs`, `snapshot_path`, `original_path`, `bytes_written` all persist |
| `test_rollback_contract_update_preserves_metadata` | 366–410 | metadata survives `update()` (state change to `ExecutedAwaitingVerify`) |
| `test_list_by_execution_returns_metadata_intact` | 412–455 | metadata intact when queried via `list_by_execution` |

## What the stronger slice covers

The fs-first FileWrite slice now validates:
- **prepare**: FsAdapter captures snapshot metadata (`snapshot_path` or `created_new_file`) and contract fields
- **persist**: contract inserted into `SqliteRollbackRepo`; metadata and plan fields survive SQLite round-trip
- **retrieve**: contract retrieved by ID with all fields intact
- **execute**: retrieved contract drives `execute()`; new content written to file
- **verify**: `verify()` runs `verify_checks` (e.g., FileHashMatches) against post-execute state
- **rollback**: `rollback()` restores original file content from snapshot
- **state transitions**: Prepared → ExecutedAwaitingVerify → Verified (or similar paths) persist through store `update()`
- **verify_checks**: stored in contract and exercised during verify phase post-retrieval
- **compensation_plan**: stored and exercised via `compensate()` which delegates to rollback for fs

## Conservative framing

- This is a **foundation slice** for Gate E/G4 (store must support adapter artifact persistence before adapter crates integrate with real storage).
- **Gateway-level HTTP execute and verify endpoints now exist** (server.rs:155–162) with state guards. The fs adapter's execute and verify paths are exercised via HTTP endpoints and via integration tests calling the adapter directly.
- **Gateway-level compensate endpoint IS exercised** via `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035).
- The fs adapter itself already has broad lifecycle tests covering capture/restore/verify/path-scope; those are pre-existing.
- Q2.2 (gateway-level verify endpoint) and Q2.3 (full fs integration via gateway execute path) are NOT claimed complete.
- No gate-level flip on the master roadmap; this records only the confirmed store+adapter-layer capability plus the demonstrated compensate HTTP path.

## Relation to roadmap

- Gate E: "ferrum-store adapter artifact persistence has at least unit-level test" — **satisfied** (unit tests in rollback.rs pass; stronger integration-level evidence now available)
- Gate G4: Store must support adapter artifact persistence before adapter crates integrate — **store layer confirmed passing** (integration tests validate fs metadata + plan + checks round-trip)
- Q2 Definition of Done for G4/Gate E: "persist adapter-specific artifacts needed for fs/git/db verify and restore" — fs-first FileWrite slice confirmed; gateway-level execute/verify wiring is implemented (server.rs:155–162)

## Verification

```
cargo test -p ferrum-integration-tests test_fs_filewrite_prepare_persist_retrieve_execute_verify_rollback
cargo test -p ferrum-integration-tests test_fs_filewrite_newfile_metadata_persists_through_store
cargo test -p ferrum-integration-tests test_fs_filewrite_state_transitions_persist_through_store
cargo test -p ferrum-integration-tests test_fs_filewrite_verify_checks_are_exercised
cargo test -p ferrum-integration-tests test_fs_filewrite_compensation_plan_exercises_rollback
cargo test -p ferrum-integration-tests test_fs_filewrite_compensation_plan_persists_through_store
cargo test -p ferrum-integration-tests test_fs_filewrite_verify_checks_persist_and_are_exercised
cargo test -p ferrum-store test_rollback_contract_metadata_round_trips
cargo test -p ferrum-store test_rollback_contract_update_preserves_metadata
cargo test -p ferrum-store test_list_by_execution_returns_metadata_intact
```
All tests pass.

## Notes

- Existing fs adapter tests (pre-Q2) cover the broader capture/restore/verify lifecycle — this note does not re-record those.
- The store-level metadata round-trip is a necessary (not sufficient) condition for Gate F integration.
- Gateway-level execute and verify HTTP endpoints now exist with state guards (server.rs:155–162). Gateway-level compensate HTTP endpoint IS exercised by `test_compensate_endpoint_restores_file_via_fs_adapter`. Execute/verify surface details are in `11-gateway-execute-verify-surface-design-note.md`.
