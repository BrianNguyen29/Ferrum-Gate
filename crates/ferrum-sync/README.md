# ferrum-sync

Cross-node ledger sync crate: transport probe + Sync-1 decision kernel +
Sync-2 groundwork.

## Current Scope

### Sync-3a / Sync-3a.1: Read-Only Transport Probe

A diagnostic-only transport probe that exercises Sync-3 transport contracts
without committing any state. It validates transport connectivity, error
mapping, and proof structure before any write-path work begins.

The `ProbeFacade` provides the current Sync-3a.1 facade boundary and maps
failures to `Sync1AbortCode` in fail-closed fashion.

### Sync-1 Decision Kernel

A pure, read-only decision function (`decide`) that implements the one-way
fast-forward sync decision table from the Sync-1 protocol sketch. Given
follower state and leader state, it returns the correct Sync-1 decision
with zero side effects.

### Sync-2 Groundwork (Partial)

A read-only preflight checker (PF1-PF8) and diff classifier (`DiffClass`) that
operates purely on caller-provided inputs. No transport, no repo queries, no
mutation. This is groundwork aligned with
`docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`.

**This is NOT the full Sync-2 implementation.** The following are deferred to P3:

- Actual ledger queries (PF1/PF5/PF6 require repo access)
- Transport-based leader tip acquisition (PF3/PF8)
- Sync session tracking (PF7)
- Capability model enforcement (PF4)

### What Is In Scope

- **Diagnostic tip fetch**: verify leader is reachable and returning consistent tip data
- **Diagnostic proof fetch**: verify proof retrieval returns well-formed proofs
- **Local proof structure verification**: verify proof has correct shape, non-empty ranges,
  and hash continuity without applying entries
- **Abort-code mapping validation**: confirm all transport error variants map to Sync-1
  abort codes per the fail-closed table
- **Sync-1 decision kernel**: pure decision table for one-way fast-forward sync
  (DONE / SYNC / FAST_FORWARD / ABORT)
- **Sync-2 groundwork**: pure preflight checker (PF1-PF8) + diff classifier (`DiffClass`)
  + bridge to Sync-1 decision kernel

### What Is Out of Scope

- Entry apply/write-path
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Full Sync-2 implementation (repo queries, transport-based tip acquisition,
  sync session tracking, capability model enforcement)

## Key Types

| Type | Purpose |
|------|---------|
| `decide()` | Pure Sync-1 decision kernel function |
| `DecisionInput` | Follower tip + leader tip + hash_path_valid |
| `Sync1Decision` | DONE / SYNC / FAST_FORWARD / ABORT(code) |
| `TipId` | Lightweight tip identity (sequence + hash) |
| `classify()` | Pure Sync-2 diff classifier (follower_tip x leader_tip -> DiffClass) |
| `DiffClass` | InSync / FollowerAhead / LeaderAhead / LeaderAheadEmpty / Bootstrap / Divergent / Unknown |
| `run_preflight()` | Ordered PF1-PF8 preflight checker over caller-provided inputs |
| `PreflightInput` | 8 boolean fields for PF1-PF8 preflight checks |
| `PreflightResult` | Pass / Fail(PreflightCheckCode) |
| `diff_class_to_decision()` | Bridge: DiffClass -> Sync1Decision |
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

## Decision Kernel Contract

The decision kernel guarantees:

- **Pure function**: no side effects, no transport calls, no mutation
- **Fail-closed**: any ambiguous input results in an abort
- **Exhaustive**: every row in the Sync-1 decision table is covered

## Sync-2 Groundwork Contract

The Sync-2 groundwork module guarantees:

- **Pure functions**: `classify()`, `run_preflight()`, `diff_class_to_decision()` have no side effects
- **Fail-closed**: `Unknown` maps to `Abort(A0)`; first preflight failure short-circuits
- **Bridge consistency**: `classify() -> diff_class_to_decision()` agrees with `decide()` for
  all reachable DiffClass variants (proven by roundtrip tests)
- **No transport, no repo queries, no mutation**

## Relationship to Sync Plan Documents

This crate corresponds to multiple sync slices in the implementation-path:

- `18-cross-node-ledger-sync-plan.md` — Sync-0 safety contract
- `19-sync-1-protocol-sketch.md` — Sync-1 decision table (implemented in `decision.rs`)
- `20-sync-2-read-only-preflight-diff-classifier.md` — Sync-2 preflight + diff classifier (groundwork in `preflight.rs`)
- `22-sync-3a-read-only-transport-probe.md` — Sync-3a probe
- `22a-sync-3a1-probe-api-boundary.md` — Sync-3a.1 facade boundary (`facade.rs`)

## Key Files

- `src/lib.rs`: crate overview + public re-exports
- `src/decision.rs`: Sync-1 decision kernel (`decide`, `DecisionInput`, `Sync1Decision`)
- `src/preflight.rs`: Sync-2 groundwork (`classify`, `run_preflight`, `diff_class_to_decision`, `DiffClass`, `PreflightInput`)
- `src/facade.rs`: `ProbeFacade`, `ProbeFacadeRequest`, `ProbeFacadeResponse`, `ProbeFacadeConfig`
- `src/transport.rs`: `Transport` trait, `TransportProbe`, `FakeLeaderTransport`, DTOs
- `src/proof.rs`: proof structure verification
- `src/error.rs`: `ProbeError`, `Sync1AbortCode`, `map_transport_error_to_abort`
