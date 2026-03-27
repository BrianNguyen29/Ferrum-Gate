# 21 — Cross-Node Ledger Sync: Sync-3 Transport Sketch

Transport sketch for Sync-3 (one-way fast-forward transport contract) of
cross-node ledger sync. Grounded in Sync-2 read-only preflight + diff
classifier per `20-sync-2-read-only-preflight-diff-classifier.md`.

ASCII only. This slice does NOT include consensus, two-way merge, peer
discovery, or write-path implementation.

---

## Status

**This is Sync-3 (read-only transport sketch / leader-tip retrieval /
proof retrieval / fail-closed error mapping) work only.** No sync
implementation exists or is planned in this slice.

Successor to Sync-2 (read-only preflight + diff classifier). Write-path
implementation, consensus, and peer discovery are not in scope.

---

## What Is In Scope (Sync-3 Only)

- Minimal transport contract: leader-tip retrieval
- Minimal transport contract: proof (hash-path) retrieval
- Error mapping layer: transport errors -> Sync-1 abort codes
- Fail-closed design: any transport failure maps to an abort code, never to
  a sync-able state
- No write-path: entries are fetched but NOT applied; apply is a future slice

---

## What Is Out of Scope (Not Sync-3)

- Entry apply/write-path (how entries are stored on disk)
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Capability or authorization model
- Sync scheduling or triggering logic
- Ledger pruning or snapshot distribution
- Byzantine fault tolerance

---

## Transport Model

Sync-3 is intentionally minimal: it covers the read-only retrieval of
two data items from a leader node:

1. **Leader-tip:** the sequence number and hash of the leader's current tip
2. **Proof:** a hash-path (Merkle proof) proving continuity between two
   sequence points

The follower uses these two reads to feed the Sync-1 decision table and
the Sync-2 diff classifier. The follower does NOT send entries to the
leader in Sync-3.

### Transport Principle

All transport calls are:
- **Idempotent:** calling the same endpoint multiple times with the same
  parameters produces the same result (no state change on leader)
- **Read-only:** leader state is never modified by a sync request
- **Fail-closed:** any error maps to an abort code; never to a guessed
  sync state

---

## Transport Contract: Leader-Tip Retrieval

### Request

```
LeaderTipRequest:
  request_id: Uuid       // unique per sync attempt
  follower_identity: NodeIdentity

LeaderTipResponse:
  status: Ok(LeaderTip) | Err(TransportError)
  leader_tip:
    sequence: u64
    hash: Sha256Hex
    timestamp: DateTimeUtc
  leader_version: VersionString   // leader software version (for compatibility)
```

### Transport Error Mapping

| Transport Error | -> Sync-1 Abort Code | Rationale |
|----------------|---------------------|-----------|
| LeaderUnreachable | A7 (Network error) | Cannot sync; abort |
| LeaderTimeout | A7 (Network error) | Transient; may retry |
| LeaderCapabilityDenied | A8 (Capability denied) | Not authorized |
| LeaderVersionIncompatible | A7 (Network error) | Protocol mismatch; abort |
| InternalError | A7 (Network error) | Treat as unreachable |

---

## Transport Contract: Proof Retrieval

### Purpose

The follower needs a hash-path proof to distinguish:
- `LeaderAhead`: leader is ahead with valid continuity (should sync)
- `Divergent`: leader diverged from local chain (must abort)

Without a proof, Sync-2 conservatively classifies `LeaderAhead` when
`leader_tip.sequence > follower_tip.sequence` without knowing if the
hashes are consistent.

### Request

```
ProofRequest:
  request_id: Uuid
  follower_identity: NodeIdentity
  start_sequence: u64       // inclusive; usually follower tip + 1
  end_sequence: u64         // inclusive; usually leader tip

ProofResponse:
  status: Ok(Proof) | Err(TransportError)
  proof:
    entries: Vec<EntryHashInfo>
      - sequence: u64
      - entry_hash: Sha256Hex
    range_hash: Sha256Hex   // hash of concatenated entry_hashes in range
    continuity_proof: HashPath
      - nodes: Vec<Sha256Hex>  // Merkle proof nodes
      - leaf_count: u64
```

### Proof Verification (Follower Side)

The follower verifies the proof locally:

```
verify_proof(proof, start_sequence, end_sequence, expected_prev_hash):
  1. Reconstruct range_hash from proof.entries (ordered by sequence)
     -> if mismatch: return Err(ProofInvalid)
  2. Verify continuity_proof:
     - path root must match range_hash
     - leaf_count must cover entries in range
     -> if mismatch: return Err(ProofInvalid)
  3. Verify first entry.prev_hash matches expected_prev_hash
     -> if mismatch: return Err(ProofInvalid)
  4. return Ok(())
```

If verification fails, the proof is rejected (Sync-1 abort code A3:
HashPathInvalid). Local chain is unchanged.

### Transport Error Mapping

| Transport Error | -> Sync-1 Abort Code | Rationale |
|----------------|---------------------|-----------|
| LeaderUnreachable | A7 (Network error) | Cannot sync; abort |
| LeaderTimeout | A7 (Network error) | Transient; may retry |
| LeaderCapabilityDenied | A8 (Capability denied) | Not authorized |
| RangeNotAvailable | A3 (HashPathInvalid) | Leader does not have range; treat as divergence signal |
| InternalError | A7 (Network error) | Treat as unreachable |

---

## Sync-3 Read-Only Execution Flow

```
Sync-3 Steps (all read-only on follower side):
==============================================

1. Sync-2 preflight: PF1-PF8 all pass
2. Sync-2 diff classify: produces DiffClass
3. If DiffClass is InSync, FollowerAhead, Divergent, Unknown:
   -> DONE or ABORT per Sync-2 decision table
4. If DiffClass is LeaderAhead or LeaderAheadEmpty or Bootstrap:
   -> Proceed with Sync-3 transport retrieval

Step 4 Detail:
4a. LeaderTipRequest -> validate leader_version compatibility
4b. LeaderTipResponse validated
4c. ProofRequest(start_sequence=follower_tip+1, end_sequence=leader_tip)
4d. ProofResponse verified locally
4e. On success: return (leader_tip, proof) to caller
   On failure: return TransportError mapped to Sync-1 abort code
```

**Sync-3 does NOT apply entries.** The tuple `(leader_tip, proof)` is
returned to the caller (Sync-1 execution handler) for decision and
apply (future slice).

---

## Error Taxonomy (Sync-3 Transport Layer)

```
TransportError:
  | LeaderUnreachable { address: SocketAddr }
  | LeaderTimeout { address: SocketAddr, duration_ms: u64 }
  | LeaderCapabilityDenied { leader: NodeIdentity, required_capability: CapabilityName }
  | LeaderVersionIncompatible { leader_version: VersionString, follower_min_version: VersionString }
  | RangeNotAvailable { start_sequence: u64, end_sequence: u64 }
  | InternalError { details: String }

SyncError (from Sync-1):
  | PreflightFailed { code: PF1 | ... | PF8 }
  | ManifestFetchFailed { cause: TransportError }
  | HashPathInvalid { expected: Sha256Hex, actual: Sha256Hex }
  | LeaderBehindFollower { leader_tip: u64, follower_tip: u64 }
  | EntryVerificationFailed { sequence: u64, cause: LedgerError }
  | C3Violation { event_id: String, existing_entry_hash: Sha256Hex, new_entry_hash: Sha256Hex }
  | NetworkError { cause: TransportError }
  | CapabilityDenied { cause: TransportError }
```

TransportError is always mapped to a Sync-1 abort code before returning
to the caller. The caller never sees raw TransportError.

---

## Fail-Closed Error Mapping Table

| Condition | Detected At | Transport Action | Sync-1 Abort Code | Follower State |
|-----------|-------------|------------------|--------------------|----------------|
| Leader unreachable | Tip or Proof | Return error | A7 | Unchanged |
| Leader timeout | Tip or Proof | Return error | A7 | Unchanged |
| Capability denied | Tip or Proof | Return error | A8 | Unchanged |
| Version incompatible | Tip | Return error | A7 | Unchanged |
| Range not available | Proof | Return error | A3 | Unchanged |
| Proof invalid | Proof verification | Return error | A3 | Unchanged |
| Continuity broken | Proof verification | Return error | A3 | Unchanged |
| First entry prev_hash != expected | Proof verification | Return error | A5 | Unchanged |

**Key principle:** A successful return from Sync-3 means the transport
layer successfully retrieved `(leader_tip, proof)` and the proof passed
local verification. Any failure maps to an abort code. There is no
"partially successful" state.

---

## Relationship to Sync-1 and Sync-2

Sync-3 fills the transport gap between Sync-2 (read-only preflight) and
Sync-1 (execution decision table):

```
Sync-2 (read-only preflight + diff classify)
    |
    | diff_classify() -> DiffClass
    v
Sync-3 (transport: leader-tip + proof retrieval)
    |
    | transport_retrieve() -> (leader_tip, proof) or abort
    v
Sync-1 (decision table + apply-or-abort)
```

Sync-3 is read-only from the follower's perspective. It does not modify
the local ledger. It does not execute the Sync-1 decision table. It
only retrieves and verifies transport-layer data.

---

## Open Questions (Sync-3 Output — Deferred to Sync-3a+)

1. **Transport protocol:** TCP with custom framing? gRPC? HTTP/REST? The
   contract above is protocol-agnostic; actual protocol choice deferred.
2. **Proof format:** Merkle tree variant? Range hash scheme? Fan-out?
   The contract specifies the fields, not the internal proof structure.
3. **Leader address discovery:** How does the follower learn the leader's
   address? (Static config? Operator input? Discovery protocol? Out of
   scope for Sync-3.)
4. **Leader versioning:** What version scheme? How is min_version
   determined? (Deferred.)
5. **Write-path:** How does the follower actually store entries? (Sync-4+
   territory.)

---

## Recommended Next Slice After Sync-3

**Sync-3a: Read-only transport probe** -- see
`docs/implementation-path/22-sync-3a-read-only-transport-probe.md`.
After the transport sketch is ratified, the next slice adds a
diagnostic layer that validates transport behavior (multi-probe tip
consistency, proof structure verification) before any write-path work
begins. Sync-3a is explicitly read-only: no entries are applied.

Write-path implementation, consensus, two-way merge, and peer discovery
are not in scope for Sync-3a.

---

## Key Files (Reference for Future Implementation)

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs:229` | `verify_entry` — core chain integrity check |
| `crates/ferrum-ledger/src/lib.rs:260` | `verify_chain` — full chain verification |
| `crates/ferrum-store/src/repos.rs:162` | `LedgerRepo` trait boundary |
| `crates/ferrum-store/src/sqlite/ledger.rs:188` | SQLite ledger append path |
| `docs/implementation-path/18-cross-node-ledger-sync-plan.md` | Sync-0 safety contract (predecessor) |
| `docs/implementation-path/19-sync-1-protocol-sketch.md` | Sync-1 protocol sketch (predecessor) |
| `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md` | Sync-2 read-only preflight (predecessor) |

---

## References

- Sync-0 (safety contract): `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- Sync-1 (protocol sketch): `docs/implementation-path/19-sync-1-protocol-sketch.md`
- Sync-2 (read-only preflight + diff classifier): `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`
- Sync-3 adds: transport contract for leader-tip retrieval, transport contract for proof retrieval, fail-closed error mapping (TransportError -> Sync-1 abort codes), proof verification logic
- Sync-3 explicitly excludes: write-path, consensus, two-way merge, peer discovery
