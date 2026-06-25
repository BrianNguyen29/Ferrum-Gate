# ferrum-ledger

Append-only audit ledger prototype for FerrumGate.

## Status

**Prototype / not runtime-integrated.** The runtime audit chain is maintained by `ferrum-store` (audit table, provenance events, and tamper-evident Merkle roots). This crate is not linked into the active production path.

## Historical purpose

- Experimental append-only ledger design with hash-chain verification.
- Superseded by the audit store in `ferrum-store`.

## Where to find the runtime audit chain

- `crates/ferrum-store/src/audit.rs` — audit table, event storage, and lineage queries.
- `docs/architecture/tamper-evident-audit-design.md` — Merkle-root and checkpoint design.
- `docs/PRODUCTION_NOTES.md` — runtime audit configuration and verification.

## Related docs

- `crates/ferrum-store/README.md` — current runtime audit implementation.
