# 12 — Ledger Hash Chain Execution Plan

Commit-by-commit plan for completing the ledger hash chain integration.
Grounded in existing repo reality: `ferrum-ledger` has in-memory chain logic,
`ferrum-store` has `LedgerRepo` persistence shape, integration tests exist at
`tests/integration_gateway_flow.rs:9226`.

Oracle recommendation: keep hashing in `ferrum-ledger`, chain-tip/append atomic
in `ferrum-store`, gateway uses one store call. First slice focuses on
commit-path integrity.

ASCII only.

---

## Current State

| Component | Status |
|-----------|--------|
| `ferrum-ledger` in-memory logic | DONE (hash chain, verify_chain, LedgerEntry::from_event, unit tests) |
| `ferrum-store` LedgerRepo | DONE (atomic append, get_by_event, get_latest, list_recent; Commit 1 complete) |
| Gateway -> ledger wiring on commit | DONE (Commit 2 complete; Slice 3 tests at line 9226) |
| Store persistence reload on startup | DONE (Commit 3 complete; verify_chain called after reload) |
| End-to-end hash chain verification | DONE (Commit 4 complete; `ledger_hash_chain` and `commit_flow_writes_ledger_entry` tests pass) |

**Initial ledger integration slice is complete.** Commits 1-4 are done per `docs/18-phase-f-evidence-pack.md` line 159.

---

## Commit 1: Make ledger append atomic in ferrum-store

**Target:** `crates/ferrum-store/src/sqlite/` and `crates/ferrum-store/src/repos.rs`

**Scope:**
- Ensure `LedgerRepo::append()` is atomic: insert event + update chain tip in a
  single SQLite transaction
- Introduce a store-level atomic append API that accepts a `ProvenanceEvent`
  and handles sequencing internally, delegating hash computation to
  `ferrum-ledger`
- Keep hash computation in `ferrum-ledger`; store only persists the resulting
  entry and chain tip
- Return the new `LedgerEntry` with its assigned `sequence` to the caller

**Validation:**
- `cargo test -p ferrum-store -- Ledger` passes
- Append is durable only after SQLite transaction commits
- Concurrent appends do not corrupt chain tip

**Out of scope:**
- Gateway wiring (done in Commit 2)
- Startup reload (done in Commit 3)

---

## Commit 2: Wire gateway commit flow to use store append

**Target:** `crates/ferrum-gateway/src/` (commit/verify handler)

**Scope:**
- After `perform_commit` succeeds, call the store's atomic ledger append API
  with the committed provenance event
- After `perform_verify`, if R0 auto-commit fires, same ledger append
- Propagate any ledger error as a fatal internal error (ledger must be
  consistent; append failures are not silent)
- Gateway issues exactly one store call per committed event (store handles
  sequencing internally)

**Validation:**
- `test_commit_flow_writes_ledger_entry_linked_to_provenance_event` passes
  (`tests/integration_gateway_flow.rs:9230`)
- Ledger entry's `event.event_id` matches the stored ProvenanceEvent

**Out of scope:**
- Startup reload
- Hash chain verification across restarts

---

## Commit 3: Add `verify_chain` call after store reload

**Target:** `crates/ferrum-store/src/sqlite/` or `crates/ferrum-gateway/src/`

**Scope:**
- On store initialization / ledger load, call `ledger.verify_chain()`
  before opening for new appends
- If verification fails, log a fatal error and refuse to start

**Validation:**
- `cargo test -p ferrum-store` and `cargo test -p ferrum-gateway` pass
- Tamper a persisted ledger entry in a test; verify the node refuses to start

**Out of scope:**
- Online / live tamper detection (future P1/P2)

---

## Commit 4: Run and fix ledger integration tests

**Target:** `tests/integration_gateway_flow.rs` Slice 3

**Scope:**
- Run all tests in `// LEDGER INTEGRATION TESTS (Slice 3)` block (lines 9226-9485)
- Fix any failures: missing impl in store, missing wiring in gateway,
  event kind mismatch, sequence counter issues, hash computation drift

**Validation:**
- `cargo test ledger_hash_chain` and
  `cargo test commit_flow_writes_ledger_entry` both pass
- `test_ledger_hash_chain_correct_over_multiple_commits` passes

**Out of scope:**
- New test scenarios beyond the two that are already written

---

## Commit 5: Document and mark done

**Scope:**
- Update `docs/implementation-path/08-next-issue-backlog.md`: move ledger hash
  chain from P1 completed to a brief P1 DONE note or remove from backlog
- Update `docs/implementation-path/11-remaining-tasks.md`: mark `[x]` done with
  citation to this doc
- Update `docs/implementation-path/README.md`: this doc already added in order
- Confirm `docs/18-phase-f-evidence-pack.md` line 159 gap entry is updated
  to reflect DONE status

**Validation:**
- All five docs are consistent and reference each other correctly

---

## Out of Scope (Future Backlog)

- Online/live hash chain verification during append (not just on startup)
- Ledger pruning or compaction
- Cross-node ledger sync / distributed ledger
- Ledger replay for recovery (beyond chain verification)

---

## Recommended Next Slice

**Runtime integration boundary** (P2 priority).

Define the model for mapping external runtime/tool events into FerrumGate
provenance graph without leaking vendor assumptions into core crates.
Select one first integration (e.g., MCP/runtime event bridge) and prove
internal + external events share an execution lineage.

Source: `docs/implementation-path/08-next-issue-backlog.md` P3 lines 29-31.

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs` | In-memory ledger with hash chain |
| `crates/ferrum-store/src/sqlite/` | LedgerRepo persistence (atomic append) |
| `crates/ferrum-store/src/repos.rs:155` | LedgerRepo trait shape |
| `crates/ferrum-gateway/src/` | Commit/verify handlers |
| `tests/integration_gateway_flow.rs:9226` | Ledger integration tests |
| `docs/18-phase-f-evidence-pack.md:159` | Gap tracking entry |
| `docs/implementation-path/11-remaining-tasks.md:29` | Remaining work checklist |
