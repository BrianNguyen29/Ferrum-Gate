# 17 — Ledger Live Hash Verification During Append

Execution plan for verifying the hash chain at append-time (not on startup).
This is the next slice after the initial ledger hash chain integration
(Commits 1-4 complete per `12-ledger-hash-chain-execution-plan.md`).

ASCII only. Commits A-C implemented per evidence below.

---

## Scope

**What is in scope:**
- During `LedgerRepo::append()`, verify the incoming entry's `prev_hash`
  matches the current chain tip's hash before committing
- If mismatch, return an error and do NOT commit the entry
- Verification is inline with append in the same SQLite transaction

**What is out of scope:**
- Ledger read-model (separate future P2 item)
- Cross-node ledger sync (separate future P2 item)
- Startup chain verification (already done in `12-ledger-hash-chain-execution-plan.md` Commit 3)

---

## Current State

| Component | Status |
|-----------|--------|
| `ferrum-ledger` in-memory hash chain + verify_chain | DONE per `12-ledger-hash-chain-execution-plan.md` |
| `ferrum-store` atomic append | DONE per `12-ledger-hash-chain-execution-plan.md` Commit 1 |
| Gateway commit wiring | DONE per `12-ledger-hash-chain-execution-plan.md` Commit 2 |
| Startup verify_chain | DONE per `12-ledger-hash-chain-execution-plan.md` Commit 3 |
| Live append-time verification | DONE (Commits A-C complete per this plan) |

---

## Commit A: Add `verify_entry` helper to `ferrum-ledger`

**Target:** `crates/ferrum-ledger/src/lib.rs`

**Scope:**
- Add `verify_entry(entry, expected_prev_hash) -> Result<(), LedgerError>` helper
- Unit tests: valid chain passes, broken `prev_hash` fails, genesis entry succeeds
- Keep hash computation and comparison rules inside `ferrum-ledger`

**Validation:**
- `cargo test -p ferrum-ledger` passes

---

## Commit B: Add append-time hash verification to LedgerRepo

**Target:** `crates/ferrum-store/src/sqlite/mod.rs` (LedgerRepo append logic)

**Scope:**
- Before inserting the new entry, read the current chain tip (latest entry by sequence)
- Call `ledger.verify_entry(new_entry, expected_prev_hash)` where expected_prev_hash
  is the tip's hash
- If verification fails, abort the SQLite transaction and return an error
- The hash computation itself stays in `ferrum-ledger`

**Validation:**
- `cargo test -p ferrum-store -- Ledger` passes
- Tamper a ledger entry's prev_hash in a test; verify append rejects it

---

## Commit C: Wire gateway to treat append verification failure as fatal

**Target:** `crates/ferrum-gateway/src/` (commit/verify handler)

**Scope:**
- If store's ledger append returns an error (hash mismatch), treat as fatal
  internal error: log error, do not proceed with the commit
- This maintains the invariant that a committed event always has a valid
  ledger entry

**Validation:**
- In an integration test, tamper the ledger DB file between two commits;
  verify the next commit returns a fatal error and does not write

---

## Commit D: Update docs to reflect done state

**Status:** DONE (this commit)

**Scope:**
- Mark this plan complete in `docs/implementation-path/08-next-issue-backlog.md`
- Mark done in `docs/implementation-path/11-remaining-tasks.md`
- Update `docs/18-phase-f-evidence-pack.md` gap entry for live verification

**Evidence:**
- `ferrum-ledger/src/lib.rs:229` - verify_entry helper
- `ferrum-store/src/sqlite/ledger.rs:22` - LedgerRepo append with live verification
- `ferrum-store/src/sqlite/ledger.rs:77` - chain tip read for expected_prev_hash
- `ferrum-gateway/src/server.rs:1602` - fatal error handling on verification failure
- `ferrum-store/src/sqlite/tests.rs:1423` - tamper test confirming rejection

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs` | `verify_entry` helper |
| `crates/ferrum-store/src/sqlite/mod.rs:139` | LedgerRepo append (insertion point for verification) |
| `crates/ferrum-store/src/sqlite/mod.rs:210` | Chain tip read (used to get expected_prev_hash) |
| `crates/ferrum-gateway/src/` | Commit handler (error propagation) |

---

## Out of Scope (Future Backlog)

- Ledger read-model / query API
- Cross-node ledger sync
- Ledger pruning or compaction
- Non-fatal ledger errors (ledger inconsistencies are always fatal by design)

---

## Recommended Next Slice After This

Cross-node ledger sync discovery/plan slice, since ledger read-model / provenance
query enhancement is already marked DONE elsewhere and this plan is scoped only to
append-time verification.

Source: `docs/implementation-path/08-next-issue-backlog.md` line 13.
