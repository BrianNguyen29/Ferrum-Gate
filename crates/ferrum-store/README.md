# ferrum-store

Persistence boundary for FerrumGate.

## Current Scope

The store crate provides the storage layer for all core domain objects:

- **Intents**: trust-labeled, taint-scored user intent records
- **Proposals**: evaluated governance requests tied to intents
- **Capabilities**: minted execution permits with rollback contracts
- **Executions**: tool-call execution records with verify/commit/rollback state
- **Provenance events**: lineage edges persisted to `provenance_edges` table
- **Ledger entries**: hash-chained append-only ledger for integrity verification

## Status

Phase B complete. Durable SQLite persistence is working and restart-safe for the supported flow set.

Key modules:
- `src/sqlite/`: SQLite repository implementations
- `src/migrations/`: schema migrations
- `src/provenance.rs`: provenance edge persistence and query
- `src/ledger.rs`: hash-chain append and tip verification

## Open Gaps

- Generic provenance query/replay/graph tooling (P2 backlog)
- Cross-node ledger sync (Sync-0 through Sync-3a.1 planned; no implementation)

## Provenance Query Surface

Core query surface is implemented:
- `POST /v1/provenance/query` with filters on `intent_id`, `proposal_id`, `execution_id`, `capability_id`, event kind, terminal state, time range, cursor pagination
- `ferrum-graph` read-model helpers: `terminal_events`, `walk_backwards_from`, `walk_forwards_from`
- `GET /v1/provenance/lineage/{execution_id}` for execution lineage reconstruction

Generic replay/fabric tooling remains P2 backlog.
