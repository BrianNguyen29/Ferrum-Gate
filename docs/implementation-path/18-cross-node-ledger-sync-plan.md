# 18 — Cross-Node Ledger Sync: Sync-0 Safety Contract

Discovery and execution plan for the safety-contract slice (Sync-0) of
cross-node ledger sync. Grounded in existing repo reality: single-node
ledger hash chain is complete per `12-ledger-hash-chain-execution-plan.md`
and `17-ledger-live-hash-verification-execution-plan.md`.

ASCII only. This slice does NOT include protocol design, consensus choice,
or implementation details.

---

## Status

**This is Sync-0 (safety contract / invariants / failure semantics /
entry criteria) work only.** No sync implementation exists or is planned
in this slice.

---

## What Is In Scope (Sync-0 Only)

- Formalize the safety invariants that ANY cross-node sync protocol must
  preserve
- Define failure semantics: what happens when invariants are violated
- Define entry criteria: under what conditions it is safe to attempt
  cross-node sync
- Identify which existing ledger components are in-scope for sync safety
  analysis
- Establish which invariants are local (single-node) vs. require
  cross-node agreement
- Document known hazards specific to distributed ledger sync that are
  NOT fully resolved by single-node append-time verification

---

## What Is Out of Scope (Not Sync-0)

- Protocol choice (gossip, 2PC, Paxos, Raft, chain replication, etc.)
- Consensus algorithm design or selection
- Implementation details (network transport, message formats, etc.)
- Byzantine fault tolerance analysis
- Node discovery or peer management
- Sync scheduling or triggering logic
- Ledger pruning or compaction across nodes
- Full ledger bootstrap or snapshot distribution

---

## Current State

| Component | Status |
|-----------|--------|
| Single-node ledger hash chain | DONE per `12-ledger-hash-chain-execution-plan.md` |
| Single-node live append-time verification | DONE per `17-ledger-live-hash-verification-execution-plan.md` |
| Cross-node ledger sync | NOT STARTED (Sync-0 is the first slice) |

---

## Sync-0: Safety Contract

### Invariant L1: Local Chain Integrity (already enforced)

Every ledger entry stored locally was appended by the local node's own
append-time verification. The `prev_hash` of each entry matches the hash
of the preceding entry. No entry exists that was not produced by a
committed provenance event.

- **Enforced by:** `ferrum-ledger::verify_entry` + `LedgerRepo::append`
- **Evidence:** `ferrum-ledger/src/lib.rs:229`, `ferrum-store/src/sqlite/ledger.rs:22`

### Invariant L2: Single-Writer Sequencing (already enforced)

At any given time, at most one actor is appending to the ledger. All
entries for a given node are produced by that node's commit path.

- **Enforced by:** Single-node gateway commit flow (no concurrent writers
  by design per `12-ledger-hash-chain-execution-plan.md`)
- **Hazard:** Cross-node sync must NOT allow two nodes to produce
  interleaved entries for the same logical chain

### Invariant L3: Event-Ledger Binding (already enforced)

Each ledger entry is bound to exactly one provenance event via
`event_id`. A ledger entry cannot be created without a corresponding
committed event.

- **Enforced by:** Gateway commit flow wiring per
  `12-ledger-hash-chain-execution-plan.md` Commit 2
- **Evidence:** `ferrum-gateway/src/server.rs:1602`

### Invariant C1: Cross-Node Ordering Contract (NEW — Sync-0 to define)

If node A and node B both hold entries for the same logical ledger, the
ordering of entries must be consistent: any two entries must be ordered
the same way on both nodes, or one node must hold no entries for that
prefix.

- **Open question:** Is strict total order required, or is causal order
  sufficient?
- **Hazard:** Out-of-order delivery, network partitions, or concurrent
  appends can violate this

### Invariant C2: Hash Continuity Across Nodes (NEW — Sync-0 to define)

If entry N appears on both node A and node B, then:
- Both entries have identical `sequence`, `prev_hash`, `event_hash`, and
  `event_id`
- Entry N-1 on both nodes also has identical fields (induction)

- **Open question:** Must `event_id` be globally unique, or just
  locally unique?
- **Hazard:** Divergent chains at the same sequence number cannot be
  merged; one must be rejected

### Invariant C3: No Event Duplication (NEW — Sync-0 to define)

A provenance event (identified by `event_id`) may appear in at most one
ledger entry across all nodes. Once an event is committed to a node's
ledger, it may not be re-committed.

- **Hazard:** A event committed on node A could also be independently
  committed on node B if sync is not careful

---

## Failure Semantics

### F1: Local Chain Violation (already handled)

If a node's local chain fails `verify_chain` at startup, the node refuses
to start. This is fatal. Sync must not relax this.

- **Current behavior:** Fatal error + refusal to start per
  `12-ledger-hash-chain-execution-plan.md` Commit 3

### F2: Append-Time Hash Mismatch (already handled)

If an incoming entry's `prev_hash` does not match the current chain tip,
the append is rejected and returns an error.

- **Current behavior:** Abort transaction + error per
  `17-ledger-live-hash-verification-execution-plan.md` Commit B

### F3: Cross-Node Ordering Violation (NEW — Sync-0 to define)

If sync discovers that another node has committed entries that break
local ordering (C1 or C2), the sync attempt must abort. The local chain
is not modified. No automatic resolution is attempted.

- **Proposed behavior:** Sync returns `Conflict` error, local chain
  unchanged, operator alerted
- **Open question:** Should a partial sync be attempted (sync up to the
  conflicting sequence), or must the entire sync abort?

### F4: Event Replay Attempt (NEW — Sync-0 to define)

If sync attempts to append an entry whose `event_id` already exists in the
local ledger, the append is rejected (C3 violation).

- **Proposed behavior:** Silent skip OR conflict error depending on
  whether the entries are identical (same hash) or divergent (C2
  violation)

---

## Entry Criteria for Cross-Node Sync

Before any sync attempt, the following must be true:

### EC1: Local Chain Verified

```
ledger.verify_chain() == Ok(())
```
Must have passed at startup. No sync while local chain is suspect.

### EC2: No In-Flight Commits

No commit operation is active on the local node during sync. The local
node must be in a quiescent state with respect to writes.

- **Rationale:** Sync must not race with a concurrent local append

### EC3: Peer Node Chain Verified

The peer node's local chain must also pass `verify_chain()` before any
sync exchange. A node with a broken local chain cannot be a sync source.

- **Open question:** How does the local node learn the peer node's chain
  health? (Not in Sync-0 scope, but a prerequisite)

### EC4: Hash Continuity with Peer (Entry Point)

Before attempting to merge, the local node must establish a common
ancestor entry: an entry with the same `sequence` and `hash` on both
nodes.

- If no common ancestor exists and chains are non-empty, sync cannot
  proceed (F3 applies)
- If one chain is empty and the other is not, the empty node can
  bootstrap from the non-empty node (one-way sync only, not merge)

### EC5: Capability-Based Authorization (Future)

Sync peers must be authorized. Authorization model is out of Sync-0
scope but is a prerequisite before any network transport.

---

## Open Questions (Sync-0 Output)

These must be resolved before Sync-1 (protocol sketch):

1. **Causal vs. total order:** Does the system require strict total order
   across all nodes, or is causal ordering (per-event-source) sufficient?
2. **Common ancestor strategy:** When chains diverge, is there ever a
   scenario where they can safely reconverge, or must one chain be
   rejected in its entirety?
3. **Event identity:** Is `event_id` globally unique (UUID-style) or
   locally unique? If globally unique, how is this enforced?
4. **Sync direction:** Is sync one-way (leader-follower) or two-way
   (merge)? This affects whether C2/C3 violations are possible.
5. **Conflict granularity:** If a conflict is detected at sequence N, is
   it safe to sync entries 0..N-1 from the peer, or must the entire sync
   abort?
6. **Operator involvement:** Are conflicts resolved automatically (one
   chain wins) or do they require operator decision?

---

## Recommended Next Slice After Sync-0

**Sync-1: Protocol sketch** (not in this doc). After the safety contract
is ratified, the next slice would sketch a minimal protocol that
preserves the invariants defined here, without committing to
implementation details.

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs` | Single-node hash chain, `verify_entry`, `verify_chain` |
| `crates/ferrum-store/src/sqlite/ledger.rs:22` | Atomic append with live verification |
| `crates/ferrum-store/src/repos.rs:155` | LedgerRepo trait shape |
| `crates/ferrum-gateway/src/server.rs:1602` | Fatal error on verification failure |
| `docs/implementation-path/12-ledger-hash-chain-execution-plan.md` | Single-node ledger history |
| `docs/implementation-path/17-ledger-live-hash-verification-execution-plan.md` | Append-time verification history |
| `docs/18-phase-f-evidence-pack.md:164` | Future work entry for cross-node sync |

---

## References

- Safety invariants L1-L3 are already enforced in single-node operation
- Invariants C1-C3 and failure semantics F3-F4 are new and require
  cross-node analysis
- Entry criteria EC1-EC5 define when sync is safe to attempt
