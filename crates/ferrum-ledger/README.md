# ferrum-ledger

Append-only audit ledger with hash-chain integrity.

## Design

Each `LedgerEntry` wraps a `ProvenanceEvent` and adds chain metadata:
- `sequence`: zero-based index (0 = genesis)
- `prev_hash`: hash of the previous entry (`None` for genesis)
- `entry_hash`: deterministic SHA-256 hash of (event content + prev_hash)

The genesis entry uses `"GENESIS"` as its prev_hash sentinel for hashing.

## Core API

- `InMemoryLedger::new()` — creates an empty ledger
- `append(event)` — appends a ProvenanceEvent, validates chain, returns &LedgerEntry
- `verify_chain()` — validates entire chain integrity
- `genesis()` / `last_entry()` / `entries()` — read access

## Invariants

- Entries are append-only; no insert or delete operations
- Each entry's `prev_hash` must match the preceding entry's `entry_hash`
- Each entry's `entry_hash` is deterministically recomputed and verified
- Duplicate `event_id` entries are rejected

## Persistence

This crate provides in-memory storage only. Persistence via
`ferrum-store` (SQLite) is the next integration slice.
