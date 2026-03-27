# 19 — Cross-Node Ledger Sync: Sync-1 Protocol Sketch

Protocol sketch for Sync-1 (one-way fast-forward sync) of cross-node ledger
sync. Grounded in Sync-0 safety contract per `18-cross-node-ledger-sync-plan.md`.

ASCII only. This slice does NOT include transport, consensus, two-way merge,
peer discovery, or write-path implementation.

---

## Status

**This is Sync-1 (one-way fast-forward protocol sketch / preflight /
decision table / abort semantics) work only.** No sync implementation exists
or is planned in this slice.

Successor to Sync-0 (safety contract). Sync-2+ (transport, consensus,
implementation) are not in scope.

---

## What Is In Scope (Sync-1 Only)

- One-way fast-forward protocol sketch (leader -> follower, pull model)
- Preflight checks before sync attempt
- Decision table for sync outcomes (accept / abort / conflict)
- Abort semantics: when and how sync must fail-closed
- Protocol state machine (bounded to one-way pull)
- Minimum viable sync dialog (handshake through apply-or-abort)

---

## What Is Out of Scope (Not Sync-1)

- Network transport design (TCP, gRPC, HTTP, WebSocket, etc.)
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Write-path implementation (how entries are actually stored on disk)
- Ledger pruning or snapshot distribution
- Conflict resolution automation (operator decision required)
- Byzantine fault tolerance

---

## Protocol Design Decisions (Inherited from Sync-0)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sync direction | One-way (follower pulls from leader) | Avoids C2/C3 violations from concurrent appends |
| Event identity | Locally unique `event_id` | Per-node sequencing; no global uniqueness required |
| Ordering | Causal order sufficient | Total order not required for audit use case |
| Conflict handling | Fail-closed | Any divergence triggers abort; no auto-resolution |
| Chain model | Append-only | No rebasing or history rewriting |

---

## One-Way Fast-Forward Protocol Sketch

### Model

```
Leader Node                      Follower Node
    |                                  |
    |  <-- prefetch manifest --------- |
    |  (sequence ranges, entry_hashes) |
    |                                  |
    |  ------- request entries -------->
    |  (sequence N+1 .. M)             |
    |                                  |
    |  <-- entries with proofs --------|
    |  ( LedgerEntry[] + hash path )   |
    |                                  |
    |     [local verification]         |
    |     [apply or abort]             |
```

### Phases

#### Phase 0: Preflight

Before any network communication, follower verifies:

```
PF1: local chain passes verify_chain()
PF2: no in-flight commits on local node
PF3: leader address/identity known (out of scope: how)
PF4: leader is authorized for sync (capability check, out of scope: model)
```

If any prefight check fails, sync does not begin. Local state unchanged.

#### Phase 1: Manifest Fetch (Leader -> Follower)

Follower requests manifest from leader describing its chain:

```
ManifestRequest:
  follower_chain_tip: Option<SequenceInfo>

ManifestResponse:
  status: Ok(Manifest) | Err(SyncError)
  manifest:
    leader_tip_sequence: u64
    leader_tip_hash: Sha256Hex
    ranges: Vec<HashRange>
      range: [start_seq, end_seq]
      range_hash: Sha256Hex  // hash of concatenated entry_hashes in range
```

Hash range enables follower to verify continuity without transferring all
entry_hashes. This is a Merkle-chunk sketch (not full Merkle tree; design
deferred to Sync-2).

#### Phase 2: Apply Decision (Follower)

Follower applies decision table:

```
DECISION TABLE
==============

Condition                          | Decision
-----------------------------------|------------------
leader_tip == follower_tip        | DONE (no sync)
leader_tip < follower_tip         | ABORT (ahead by local commits)
leader_tip > follower_tip AND      | SYNC
  follower_tip exists AND         |   fetch entries N+1..M
  hash_path valid                  |   apply on success
leader_tip > follower_tip AND      | ABORT (C2 violation)
  follower_tip exists AND          |   log divergence
  hash_path INVALID                |   do NOT apply
leader_tip > follower_tip AND      | FAST_FORWARD
  follower_tip is NONE (empty)     |   fetch entries 0..M
                                  |   apply genesis + all
leader_tip exists AND             | FAST_FORWARD (bootstrap)
  follower_tip is NONE            |   fetch entries 0..M
                                  |   apply genesis + all
```

#### Phase 3: Entry Fetch (Leader -> Follower)

On SYNC or FAST_FORWARD decision, follower requests entries:

```
EntryRequest:
  start_sequence: u64
  end_sequence: u64

EntryResponse:
  status: Ok(Vec<LedgerEntry>) | Err(SyncError)
  entries: Vec<LedgerEntry>  // ordered by sequence
```

#### Phase 4: Local Verification and Apply (Follower)

Follower verifies each entry before applying:

```
For each entry in response (in sequence order):
  1. verify_entry(entry, expected_prev_hash) -> Ok or ABORT
  2. check event_id not already in local ledger -> skip or ABORT on C3
  3. append to local ledger atomically
```

If any entry fails verification (BrokenChain, TamperDetected, C3
violation), the entire sync aborts. No partial apply.

#### Phase 5: Acknowledgment (Optional)

Follower optionally acknowledges sync completion:

```
AckRequest:
  last_applied_sequence: u64
  last_applied_hash: Sha256Hex

// Out of scope: how leader processes ack, whether leader records follower state
```

---

## Abort Semantics (Fail-Closed)

Sync MUST abort (stop immediately, local chain unchanged) when any of
the following occur:

### Abort Triggers

| Code | Trigger | Detection Point | Local State |
|------|---------|-----------------|-------------|
| A1 | Local `verify_chain()` fails at any point | Preflight or post-apply | Unchanged |
| A2 | In-flight commit detected during sync | Preflight | Unchanged |
| A3 | Leader manifest hash_path invalid | Phase 2 | Unchanged |
| A4 | Leader is behind follower (leader_tip < follower_tip) | Phase 2 | Unchanged |
| A5 | Entry fails `verify_entry()` (prev_hash mismatch) | Phase 4 | Unchanged |
| A6 | Entry fails C3 check (duplicate event_id) | Phase 4 | Unchanged |
| A7 | Network error or leader unreachable | Any phase | Unchanged |
| A8 | Capability check fails | Preflight | Unchanged |

### Abort Behavior

- Local ledger is **never modified** during a failed sync attempt
- Each entry is verified before the next is fetched (no batch-and-revert)
- Abort returns a structured `SyncError` with reason code
- Operator is alerted (alerting mechanism out of scope)
- Sync can be retried after conditions are resolved

### Non-Abort: Silent Skip

Some conditions do NOT abort but require handling:

| Code | Condition | Handling |
|------|-----------|----------|
| S1 | Entry `event_id` already exists AND entry_hash matches | Silent skip (idempotent replay guard) |
| S2 | Entry `event_id` already exists AND entry_hash differs | ABORT (A6 - C3 violation with divergence) |

---

## Error Taxonomy

```
SyncError:
  | PreflightFailed { code: PF1 | PF2 | PF3 | PF4, details: String }
  | ManifestFetchFailed { cause: NetworkError | LeaderUnreachable | ... }
  | HashPathInvalid { expected: Sha256Hex, actual: Sha256Hex }
  | LeaderBehindFollower { leader_tip: u64, follower_tip: u64 }
  | EntryVerificationFailed { sequence: u64, cause: LedgerError }
  | C3Violation { event_id: String, existing_entry_hash: Sha256Hex, new_entry_hash: Sha256Hex }
  | NetworkError { ... }
  | CapabilityDenied { ... }
```

---

## Key Invariants Preserved by Protocol

| Invariant | Enforced By |
|-----------|-------------|
| L1: Local Chain Integrity | `verify_entry` on every entry before apply |
| L2: Single-Writer Sequencing | One-way model: follower never races with leader |
| L3: Event-Ledger Binding | Each entry bound to event via gateway commit path |
| C1: Cross-Node Ordering | Hash continuity check via `prev_hash` chain |
| C2: Hash Continuity | `verify_entry` checks `prev_hash` match on every entry |
| C3: No Event Duplication | C3 check before every apply (A6 / S1 / S2) |

---

## Open Questions (Sync-1 Output — Deferred to Sync-2+)

1. **Manifest hash range design:** Merkle-chunk size, fan-out, proof
   format. Not needed for correctness; affects bandwidth efficiency.
2. **Capability model:** What capabilities are required for sync? Who
   issues them? How are they validated?
3. **Leader address bootstrapping:** How does follower discover leader?
   Static config? DNS? Discovery protocol?
4. **Retry backoff:** On transient failure (A7), when and how to retry?
5. **Acknowledgment semantics:** Does leader record follower progress?
   Is this required for correctness or only for observability?
6. **Partial sync abort granularity:** If entry N fails verification,
   can entries 0..N-1 be safely applied? (Sync-1 says NO; entire
   sync aborts. This conservative choice may be relaxed in future.)

---

## Recommended Next Slice After Sync-1

**Sync-2: Transport sketch** (not in this doc). After the protocol sketch
is ratified, the next slice would define minimal transport requirements
that preserve the one-way fast-forward model, without implementing
consensus or peer discovery.

---

## Relationship to Sync-0

Sync-1 satisfies all Sync-0 entry criteria (EC1-EC5) and implements the
decisions sketched in Sync-0's "Recommended Next Slice After Sync-0" section.
Sync-1 does NOT resolve Sync-0's open questions marked for future work;
those remain deferred.

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs:229` | `verify_entry` — core chain integrity check |
| `crates/ferrum-ledger/src/lib.rs:260` | `verify_chain` — full chain verification |
| `crates/ferrum-store/src/sqlite/ledger.rs:22` | Atomic append with live verification |
| `crates/ferrum-store/src/repos.rs:155` | LedgerRepo trait shape |
| `docs/implementation-path/18-cross-node-ledger-sync-plan.md` | Sync-0 safety contract (predecessor) |
| `docs/implementation-path/12-ledger-hash-chain-execution-plan.md` | Single-node ledger history |
| `docs/implementation-path/17-ledger-live-hash-verification-execution-plan.md` | Append-time verification history |

---

## References

- Sync-0 (this doc's predecessor): `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- Sync-0 defined: L1-L3 (local), C1-C3 (cross-node), F1-F4 (failure), EC1-EC5 (entry criteria)
- Sync-1 adds: protocol phases, preflight checks, decision table, abort triggers (A1-A8), silent-skip handling (S1-S2), error taxonomy
- Sync-1 explicitly excludes: transport, consensus, two-way merge, peer discovery, write-path
