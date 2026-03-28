# 22 — Cross-Node Ledger Sync: Sync-3a Read-Only Transport Probe

Plan for Sync-3a (read-only transport probe / diagnostic fetch / local
verification) of cross-node ledger sync. Grounded in Sync-3 transport
sketch per `21-sync-3-transport-sketch.md`.

ASCII only. This slice does NOT include write-path implementation,
consensus, two-way merge, or peer discovery.

---

## Status

**This is Sync-3a (read-only diagnostic transport probe) work only.**
No sync implementation exists or is planned in this slice.

Successor to Sync-3 (transport sketch). Write-path implementation,
consensus, and peer discovery are not in scope.

---

## What Is In Scope (Sync-3a Only)

- Diagnostic tip fetch: verify leader is reachable and returning
  consistent tip data across multiple probes
- Diagnostic proof fetch: verify proof retrieval is returning
  well-formed proofs with valid structural properties (not full
  cryptographic verification, which requires leader-tip as anchor)
- Local proof structure verification: verify proof has correct shape,
  non-empty ranges, and hash continuity without applying entries
- Abort-code mapping validation: confirm all transport error variants
  map to Sync-1 abort codes per the fail-closed table in Sync-3
- No entry apply: entries are fetched but NOT applied; apply is a
  future slice

---

## What Is Out of Scope (Not Sync-3a)

- Entry apply/write-path (how entries are stored on disk)
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Full Merkle proof cryptographic verification (requires apply-phase
  anchor)
- Sync scheduling or triggering logic
- Capability or authorization model
- Ledger pruning or snapshot distribution

---

## Design: Read-Only Diagnostic Probe

Sync-3a is a diagnostic-only slice that exercises the Sync-3 transport
contracts without committing any state. Its purpose is to validate
transport connectivity, error mapping, and proof structure before any
write-path work begins.

### Probe Workflow

```
Sync-3a Probe Steps (all read-only on follower side):
=============================================

1. Sync-2 preflight: PF1-PF8 all pass
2. Sync-2 diff classify: produces DiffClass
3. If DiffClass is InSync, FollowerAhead, Divergent, Unknown:
   -> DONE or ABORT per Sync-2 decision table; no probe needed
 4. If DiffClass is LeaderAhead or Bootstrap:
   -> Proceed with Sync-3a diagnostic probe

Step 4 Detail:
4a. LeaderTipRequest -> validate leader_version compatibility
4b. LeaderTipResponse: record tip values (sequence, hash, timestamp)
4c. Repeat 4a-4b N times (N >= 3) to detect inconsistent responses
4d. ProofRequest(start_sequence=follower_tip+1, end_sequence=leader_tip)
4e. ProofResponse: verify structure only (non-empty entries,
    range_hash present, continuity_proof nodes non-empty)
4f. On structural mismatch: map to Sync-1 abort code (A3)
4g. On all checks pass: return ProbeOk { tip, proof_structure }
   On any failure: return TransportError mapped to Sync-1 abort code
```

**Sync-3a does NOT apply entries.** It only exercises the read-only
transport contracts and validates the structure of responses. Any
failure maps to an abort code.

---

## Design: Multi-Probe Tip Consistency Check

A single tip fetch is insufficient to establish transport reliability.
Sync-3a introduces a lightweight multi-probe check:

```
tip_consistency_check(leader_address, N=3):
  tips = []
  for i in 1..N:
    tip = leader_tip_fetch(leader_address)
    if tip is Err:
      return Err(tip.error)
    tips.push(tip)
  if all t.sequence == tips[0].sequence AND all t.hash == tips[0].hash:
    return Ok(tips[0])  // consistent
  else:
    return Err(InconsistentTip)  // maps to A7 (Network error)
```

This detects:
- Leader returning stale tip (sequence jumped between calls)
- Leader returning divergent tips (hash changed without sequence change)
- Transient vs persistent leader unavailability

---

## Design: Proof Structure Verification

Full proof verification (matching range_hash against leader_tip) requires
the apply-phase anchor and is deferred to the write-path slice.
Sync-3a performs structure-only verification:

```
verify_proof_structure(proof):
  1. proof.entries is non-empty
     -> if empty: return Err(ProofStructureInvalid)
  2. proof.entries are in strictly increasing sequence order
     -> if not ordered: return Err(ProofStructureInvalid)
  3. proof.range_hash is non-empty hex string
     -> if empty: return Err(ProofStructureInvalid)
  4. proof.continuity_proof.nodes is non-empty
     -> if empty: return Err(ProofStructureInvalid)
  5. proof.continuity_proof.leaf_count >= proof.entries.len()
     -> if not: return Err(ProofStructureInvalid)
  6. return Ok(())
```

If any check fails, map to Sync-1 abort code A3 (HashPathInvalid).
Local chain is unchanged.

---

## Error Mapping (Sync-3a Diagnostic Layer)

Sync-3a adds one new abort code for diagnostic-specific failures:

| Diagnostic Error | -> Sync-1 Abort Code | Rationale |
|------------------|---------------------|-----------|
| TipInconsistent (multi-probe mismatch) | A7 (Network error) | Leader returned inconsistent tips; abort |
| ProofStructureInvalid | A3 (HashPathInvalid) | Proof missing required fields; abort |
| LeaderUnreachable | A7 (Network error) | Cannot sync; abort |
| LeaderTimeout | A7 (Network error) | Transient; may retry |
| LeaderCapabilityDenied | A8 (Capability denied) | Not authorized |
| LeaderVersionIncompatible | A7 (Network error) | Protocol mismatch; abort |
| RangeNotAvailable | A3 (HashPathInvalid) | Leader does not have range; treat as divergence |
| InternalError | A7 (Network error) | Treat as unreachable |

---

## Relationship to Sync-3

Sync-3a extends Sync-3 by adding diagnostic discipline to the transport
sketch:

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
[future slice: write-path apply]
```

Sync-3a does NOT change Sync-3's transport contracts or error mappings.
It adds a diagnostic layer on top that validates transport behavior
before write-path work begins.

---

## Open Questions (Sync-3a Output — Deferred to Write-Path Slice)

1. **Full proof verification:** Cryptographic verification of proof
   against leader_tip requires the apply-phase anchor. Deferred to
   write-path slice.
2. **Probe parameterization:** How many probes (N)? What timeout per
   probe? What constitutes "transient" vs "persistent" failure?
3. **Leader version compatibility:** What version scheme? How is
   min_version determined? How to handle version negotiation?
4. **Write-path:** How does the follower actually store entries?
   (Sync-4+ territory.)

---

## Recommended Next Slice After Sync-3a

**Sync-3a.1: Probe API boundary** (not in this doc). After the
read-only diagnostic probe is ratified, the next slice establishes a
clean facade/boundary for callers: explicit inputs, explicit outputs,
abort-code-only failures, and read-only guarantee. The boundary
explicitly keeps transport DTOs and internal error taxonomy as
non-contractual internals behind the facade. Write-path apply is
deferred to a post-boundary slice.

Sync-3a.1 does NOT include adapter implementation, write-path,
consensus, two-way merge, or peer discovery.

See `docs/implementation-path/22a-sync-3a1-probe-api-boundary.md`.

---

## Key Files (Reference for Future Implementation)

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs:229` | `verify_entry` -- core chain integrity check |
| `crates/ferrum-ledger/src/lib.rs:260` | `verify_chain` -- full chain verification |
| `crates/ferrum-store/src/repos.rs:162` | `LedgerRepo` trait boundary |
| `crates/ferrum-store/src/sqlite/ledger.rs:188` | SQLite ledger append path |
| `crates/ferrum-proto/src/common.rs:8` | Protocol type definitions |
| `docs/implementation-path/18-cross-node-ledger-sync-plan.md` | Sync-0 safety contract (predecessor) |
| `docs/implementation-path/19-sync-1-protocol-sketch.md` | Sync-1 protocol sketch (predecessor) |
| `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md` | Sync-2 read-only preflight (predecessor) |
| `docs/implementation-path/21-sync-3-transport-sketch.md` | Sync-3 transport sketch (predecessor) |

---

## References

- Sync-0 (safety contract): `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- Sync-1 (protocol sketch): `docs/implementation-path/19-sync-1-protocol-sketch.md`
- Sync-2 (read-only preflight + diff classifier): `docs/implementation-path/20-sync-2-read-only-preflight-diff-classifier.md`
- Sync-3 (transport sketch): `docs/implementation-path/21-sync-3-transport-sketch.md`
- Sync-3a adds: multi-probe tip consistency check, proof structure verification, diagnostic-specific abort code (TipInconsistent), probe workflow
- Sync-3a explicitly excludes: write-path, consensus, two-way merge, peer discovery, full Merkle proof cryptographic verification
