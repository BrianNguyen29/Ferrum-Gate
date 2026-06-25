# ADR 006 — Archive ferrum-ledger and Deferred Runtime Items

## Status

Accepted

## Context

`ferrum-ledger` was an append-only audit ledger prototype with hash-chain verification. It was never linked into the active production path; the runtime audit chain is maintained by `ferrum-store` (audit table, provenance events, and tamper-evident Merkle roots). Keeping it in the workspace adds compile-time cost and maintenance surface for code that has no consumers.

Separately, two runtime-facing items have been discussed but are not yet committed:

1. **Runtime PostgreSQL default-on / packaging** — making the `postgres` feature enabled by default or providing a separate binary that bundles the PostgreSQL backend.
2. **WORM export bundle** — a write-once-read-many export format for audit artifacts with local direct-verify support.

Both require additional design evidence and operator validation before implementation.

## Decision

1. **Archive `ferrum-ledger`** — remove it from the workspace and delete the crate directory. Rationale is preserved in this ADR and the crate README is preserved in git history.
2. **Keep `ferrum-graph` and `ferrum-sync`** — they are internal prototypes with active consumers (audit/provenance reconciliation and sync/MCP bridge paths). Do not remove them.
3. **Defer WORM export bundle** — document in ROADMAP as a future priority. No implementation until external anchoring design and operator evidence exist.
4. **Defer runtime PostgreSQL default-on** — document in ROADMAP as a future priority. No implementation until packaging, feature-gate, and binary-size tradeoffs are reviewed.

## Consequences

- **Positive**: Reduced workspace compile time and maintenance surface.
- **Positive**: Clear signal that `ferrum-ledger` is superseded by `ferrum-store` audit.
- **Positive**: Deferred items remain visible in ROADMAP without premature code changes.
- **Negative**: If a future design needs a standalone ledger crate, it must be recreated or recovered from git history.
- **Neutral**: `ferrum-graph` and `ferrum-sync` continue as internal-only prototypes with no external stability guarantee.

## Related

- `crates/ferrum-store/README.md` — current runtime audit implementation.
- `docs/adr/003-store-split.md` — SQLite/PostgreSQL store split.
- `docs/ROADMAP.md` — deferred WORM export and PostgreSQL packaging.
