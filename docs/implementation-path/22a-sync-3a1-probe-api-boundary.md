# 22a — Cross-Node Ledger Sync: Sync-3a.1 Probe API Boundary

Plan for Sync-3a.1 (probe API boundary / clean facade) of cross-node ledger
sync. Grounded in Sync-3a read-only transport probe per
`22-sync-3a-read-only-transport-probe.md`.

ASCII only. This slice does NOT include write-path implementation,
consensus, two-way merge, or peer discovery.

---

## Status

**This is Sync-3a.1 (probe API boundary / clean facade) work only.**
No sync implementation exists or is planned in this slice.

Successor to Sync-3a (read-only diagnostic transport probe). Write-path
implementation, consensus, and peer discovery are not in scope.

---

## What Is In Scope (Sync-3a.1 Only)

- Clean facade: define explicit input/output boundary for probe callers
- Abort-code-only failures: all errors collapse to Sync-1 abort codes; no
  transport DTOs leak through
- Read-only guarantee: facade contract is explicitly read-only; no write
  capability surfaces
- Transport DTOs as non-contractual internals: caller knows only the
  abort-code result, not the transport error taxonomy
- API boundary stabilization: formalize the probe contract before any
  adapter implementation begins
- No write-path: apply-phase is deferred to a post-boundary slice

---

## What Is Out of Scope (Not Sync-3a.1)

- Entry apply/write-path
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Full Merkle proof cryptographic verification (requires apply-phase anchor)
- Sync scheduling or triggering logic
- Capability or authorization model
- Ledger pruning or snapshot distribution
- Adapter implementation (adapter work is a subsequent slice)

---

## Design: Why a Facade/Boundary Now?

Sync-3a defines a diagnostic probe workflow but does not formally specify
the API surface that callers use. Sync-3a.1 establishes a clean boundary
between the probe internals and the caller so that:

1. The probe can be implemented and tested against a stable interface
2. Transport DTOs and internal error taxonomy remain private internals
3. The read-only guarantee is enforced by the facade contract, not by
   convention
4. Write-path work can proceed later against a stable, ratified boundary

This is a docs-first stabilization step. Adapter implementation is a
separate subsequent slice.

---

## Design: Probe Facade Contract

### Caller Input (Facade Accepts)

```
probe_request:
  leader_address: SocketAddr       // explicit input, not discovery
  follower_identity: NodeIdentity  // explicit input
  probe_count: u8                  // N for multi-probe consistency check (>= 3)
  timeout_per_probe_ms: u64        // per-call timeout
```

### Caller Output (Facade Returns)

```
probe_response:
  | ProbeOk {
      tip: LeaderTip,              // sequence, hash, timestamp
      proof_structure: ProofStructureInfo,  // shape only, no apply
    }
  | ProbeAborted { code: Sync1AbortCode }
```

**No transport DTOs leak through the facade.** The caller receives only
`Sync1AbortCode` on failure and `ProbeOk` with shape-only proof info on
success. This is intentional: the transport error taxonomy is an internal
concern.

### Read-Only Guarantee

The facade is explicitly read-only. Its contract:
- Never modifies local ledger state
- Never sends entries to the leader
- Never initiates write operations on the follower
- Always returns abort codes on any failure, never a partial/writable state

This is enforced by the facade design, not by caller convention.

---

## Design: Internal Structure (Non-Contractual)

The facade internally may use Sync-3 transport contracts and Sync-3a
diagnostic discipline, but these are private internals:

```
probe_facade (public boundary)
  |
  |-- internally: transport layer (private)
  |-- internally: multi-probe consistency check (private)
  |-- internally: proof structure verification (private)
  |-- internally: transport error -> abort code mapping (private)
  v
  only: ProbeOk { tip, proof_shape } or ProbeAborted { code }
```

The caller does not know about:
- TransportError variants (LeaderUnreachable, LeaderTimeout, etc.)
- ProofRequest / ProofResponse DTOs
- LeaderTipRequest / LeaderTipResponse DTOs
- The mapping from transport errors to abort codes

This separation ensures the transport layer can evolve without breaking
the probe API.

---

## Design: Error Collapse to Abort Codes

All internal failures collapse to a single `ProbeAborted { code }` output:

| Internal Failure | Abort Code Surface | Internal Handling |
|-----------------|-------------------|-------------------|
| Leader unreachable | A7 | Internal retry exhaustion |
| Leader timeout | A7 | Internal retry exhaustion |
| Tip inconsistent (multi-probe mismatch) | A7 | Internal check |
| Proof structure invalid | A3 | Internal verification |
| Leader version incompatible | A7 | Internal version check |
| Range not available | A3 | Internal range check |
| Internal error | A7 | Internal error containment |

The caller sees `ProbeAborted { code }` only. No transport DTOs,
no internal error variants, no distinction between transient and
persistent failures at the facade level.

---

## Design: Proof Shape Only (No Apply)

The facade returns `proof_structure: ProofStructureInfo` which contains
shape-only information:

```
ProofStructureInfo:
  entry_count: usize           // how many entries in range
  range_hash: Sha256Hex        // hash of entry range (no cryptographic
                              // verification without apply-phase anchor)
  continuity_proof_shape: HashPathShape
    node_count: usize          // number of proof nodes
    leaf_count: u64             // coverage
```

This is deliberately limited. The caller cannot:
- Apply entries (not in facade contract)
- Verify proof cryptographically (requires apply-phase anchor)
- Distinguish "invalid proof" from "unverified proof" at facade level

Full proof verification requires the write-path slice where the apply-phase
anchor is available.

---

## Relationship to Sync-3a

Sync-3a.1 adds an explicit API boundary on top of Sync-3a's diagnostic
discipline:

```
Sync-3 (transport sketch)
    |
    | defines: transport contracts, error mapping, proof format
    v
Sync-3a (read-only diagnostic probe)
    |
    | adds: multi-probe consistency, structure-only proof verification
    | adds: diagnostic-specific abort codes, probe workflow
    v
Sync-3a.1 (probe API boundary / clean facade)
    |
    | stabilizes: facade contract, input/output boundary
    | hides: transport DTOs, internal error taxonomy
    | enforces: read-only guarantee, abort-code-only failures
    v
[future slice: adapter implementation]
[future slice: write-path apply]
```

Sync-3a.1 does NOT change Sync-3a behavior. It adds a formal boundary
layer so the probe can be implemented and called against a stable
contract.

---

## Open Questions (Sync-3a.1 Output — Deferred to Subsequent Slices)

1. **Adapter implementation:** After boundary stabilization, the first
   implementation slice is the adapter that satisfies the facade contract.
   Adapter details (TCP, gRPC, HTTP) are not part of the boundary spec.
2. **Probe parameterization:** What default N? What default timeout?
   These are adapter/operator concerns, not boundary concerns.
3. **Write-path:** How does the follower apply entries? (Post-boundary
   slice, not in scope here.)

---

## Recommended Next Slice After Sync-3a.1

**Sync-3a.2: Adapter implementation** (not in this doc). After the probe
API boundary is ratified, the next slice implements the adapter that
satisfies the facade contract. The adapter is a pure implementation
concern; the facade contract is the only externally visible surface.

Sync-3a.2 does NOT include write-path, consensus, two-way merge, or peer
discovery.

**Write-path apply is deferred to a post-boundary slice** after the
adapter implementation stabilizes.

---

## Key Files (Reference for Future Implementation)

| File | Role |
|------|------|
| `crates/ferrum-sync/src/lib.rs:24` | Sync module entry point |
| `crates/ferrum-sync/src/transport.rs:353` | Transport boundary reference |
| `crates/ferrum-sync/src/transport.rs:384` | Transport error boundary reference |
| `crates/ferrum-sync/src/error.rs:11` | Sync error types |
| `crates/ferrum-sync/src/proof.rs:27` | Proof structure reference |
| `docs/implementation-path/18-cross-node-ledger-sync-plan.md` | Sync-0 safety contract (predecessor) |
| `docs/implementation-path/19-sync-1-protocol-sketch.md` | Sync-1 protocol sketch (predecessor) |
| `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md` | Sync-2 read-only preflight (predecessor) |
| `docs/implementation-path/21-sync-3-transport-sketch.md` | Sync-3 transport sketch (predecessor) |
| `docs/implementation-path/22-sync-3a-read-only-transport-probe.md` | Sync-3a read-only probe (predecessor) |

---

## References

- Sync-0 (safety contract): `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- Sync-1 (protocol sketch): `docs/implementation-path/19-sync-1-protocol-sketch.md`
- Sync-2 (read-only preflight + diff classifier): `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`
- Sync-3 (transport sketch): `docs/implementation-path/21-sync-3-transport-sketch.md`
- Sync-3a (read-only diagnostic probe): `docs/implementation-path/22-sync-3a-read-only-transport-probe.md`
- Sync-3a.1 adds: clean facade contract, input/output boundary, abort-code-only failures, read-only guarantee, transport DTO privacy
- Sync-3a.1 explicitly excludes: write-path, consensus, two-way merge, peer discovery, adapter implementation
