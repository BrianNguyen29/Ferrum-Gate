# ferrum-sync

Read-only transport probe crate for cross-node ledger sync diagnostics.

## Current Scope (Sync-3a Only)

This crate implements **Sync-3a**: a diagnostic-only transport probe that exercises
Sync-3 transport contracts without committing any state. It validates transport
connectivity, error mapping, and proof structure before any write-path work begins.

### What Is In Scope

- **Diagnostic tip fetch**: verify leader is reachable and returning consistent tip data
- **Diagnostic proof fetch**: verify proof retrieval returns well-formed proofs
- **Local proof structure verification**: verify proof has correct shape, non-empty ranges,
  and hash continuity without applying entries
- **Abort-code mapping validation**: confirm all transport error variants map to Sync-1
  abort codes per the fail-closed table

### What Is Out of Scope

- Entry apply/write-path
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Adapter implementation (this crate is contract/transport-layer only)

## Key Types

| Type | Purpose |
|------|---------|
| `ProbeFacade` | Caller-facing boundary over `TransportProbe` |
| `ProbeFacadeRequest` | Follower identity + tip sequence + probe params |
| `ProbeFacadeResponse` | Either `ProbeOk { tip, proof_structure }` or `ProbeAborted { code }` |
| `Sync1AbortCode` | Unified abort code enum (A0-A8) |
| `TransportProbe` | Internal probe orchestration over any `Transport` |
| `FakeLeaderTransport` | Test transport with injectable tip/proof/errors |

## Facade Contract

The facade guarantees:

- **Read-only**: no local ledger state is modified
- **Abort-only failures**: no transport DTOs or error variants leak through
- **Shape-only proof**: caller receives proof structure, not apply-ready entries

## Relationship to Sync Plan Documents

This crate corresponds to the Sync-3a and Sync-3a.1 slices in the implementation-path:

- `22-sync-3a-read-only-transport-probe.md`
- `22a-sync-3a1-probe-api-boundary.md`

## Key Files

- `src/lib.rs`: crate overview + public re-exports
- `src/facade.rs`: `ProbeFacade`, `ProbeFacadeRequest`, `ProbeFacadeResponse`, `ProbeFacadeConfig`
- `src/transport.rs`: `Transport` trait, `TransportProbe`, `FakeLeaderTransport`, DTOs
- `src/proof.rs`: proof structure verification
- `src/error.rs`: `ProbeError`, `Sync1AbortCode`, `map_transport_error_to_abort`
