# 20 — Cross-Node Ledger Sync: Sync-2 Read-Only Preflight Diff Classifier

Plan for Sync-2 (read-only preflight + diff classifier) of cross-node ledger
sync. Grounded in Sync-1 protocol sketch per `19-sync-1-protocol-sketch.md`.

ASCII only. This slice does NOT include transport design, consensus, two-way
merge, peer discovery, or write-path implementation.

---

## Status

**Sync-2 groundwork is partially implemented** in `crates/ferrum-sync/src/preflight.rs`.
The pure functions `classify()`, `run_preflight()`, and `diff_class_to_decision()`
are implemented with unit tests, including roundtrip checks proving consistency
with the Sync-1 decision kernel. PF3/PF8 transport-boundary helpers
(`PreflightTransportInput`, `PreflightTransportFlags`) are implemented in
`crates/ferrum-sync/src/transport.rs`.

A **trait-only repo port** (`SyncPreflightRepo`) and supporting types
(`LocalPreflightState`, `SyncRepoError`) have been added in
`crates/ferrum-sync/src/repo.rs`. This defines the read-only repo interface
that Sync-2 preflight will use.

Concrete implementations exist:
- `InMemorySyncPreflightRepo` in `ferrum-sync`: in-memory test double for tests
- `SqliteSyncPreflightRepo` in `ferrum-store`: supports PF1 + PF5 via
  `verify_local_chain()`; supports PF8 via `read_leader_tip()` backed by
  the `leader_tips` cache table (migration 002); supports PF2/PF6/PF7
  via `read_local_state()` backed by the `sync_state` table (migration 003).
  PF4 is implemented via `is_leader_authorized()` backed by the
  `leader_allowlist` table (migration 004) with deny-by-default semantics
  (missing entry => Ok(false), DB error => Err).

A pure adapter `build_preflight_input()` in `preflight.rs` bridges
`LocalPreflightState` + externally supplied flags into `PreflightInput`.
PF3 is explicitly excluded from the trait (transport/config concern).

PF3 (leader identity known) remains transport-side: callers supply
`leader_identity_known` via `PreflightTransportInput::evaluate()` which is
fail-closed on empty/unknown leader address.

PF2/PF6/PF7 state is now backed by the `sync_state` table (migration 003).
PF8 (leader tip available) is backed by the `leader_tips` cache table.
PF4 (leader authorization) is backed by the `leader_allowlist` table
(migration 004) with deny-by-default semantics.

PF8 cache population is implemented via the real probe-to-cache path
(`probe_and_cache_leader_tip` in `ferrum-store/src/sync_service.rs`),
which performs PF4 authorization check + HTTP probe + cache write
(Sync-3a territory).

What remains deferred: retry/backoff on transient probe failure,
write/apply path, consensus, and two-way merge.

Successor to Sync-1 (one-way fast-forward protocol sketch). Transport,
consensus, and write-path implementation are not in scope.

---

## What Is In Scope (Sync-2 Only)

- Read-only diff classifier: given two ledger states (local tip, leader tip),
  classify the relationship (in-sync / behind / ahead / divergent)
- Read-only preflight checks: verify local node is in a safe state to attempt
  sync, without modifying any state
- Decision table expansion: map diff-classifier outcomes to Sync-1 decision
  table entries
- Failure-closed design: any ambiguity or detection failure results in a
  classified "unknown" state that prevents sync initiation
- No transport: all inputs are local state queries; no network calls

---

## What Is Out of Scope (Not Sync-2)

- Network transport design (TCP, gRPC, HTTP, WebSocket, etc.)
- Consensus algorithm or leader election
- Two-way merge or bidirectional sync
- Peer discovery or address management
- Write-path implementation (how entries are actually stored on disk)
- Ledger pruning or snapshot distribution
- Conflict resolution automation (operator decision required)
- Byzantine fault tolerance
- Actual sync execution (Sync-3+ territory)

---

## Design: Read-Only Preflight Checks

All preflight checks are local-only queries against the follower node's
ledger state. No network calls. No state modification.

### Preflight Check Set (PF1-PF8)

```
PF1: local chain passes verify_chain()
     -> query: ledger.verify_chain() == Ok(())

PF2: no in-flight commits on local node
     -> query: no active transaction on ledger repo

PF3: leader address/identity known (out of scope: how)
     -> query: local config has leader identity stored

PF4: leader is authorized for sync (capability check, out of scope: model)
     -> query: local capability store grants sync capability to leader

PF5: local ledger is readable (can query tip)
     -> query: ledger.get_tip() returns Some or None (empty is OK)

PF6: local ledger has no uncommitted local entries
     -> query: no entries since last confirmed sequence

PF7: local node is not currently syncing
     -> query: no active sync session in progress

PF8: leader tip is available (local query; out of scope: how obtained)
     -> query: leader_tip info is present in local sync state
```

### Preflight Failure Handling

If any preflight check fails:
- Sync does not begin
- Local ledger state is unchanged
- A structured `PreflightError` is returned with the failed check code

---

## Design: Read-Only Diff Classifier

Given:
- `follower_tip: Option<SequenceInfo>` (local)
- `leader_tip: Option<SequenceInfo>` (obtained out-of-scope; e.g., from
  static config or operator input for Sync-2)

The classifier outputs one of:

```
enum DiffClass {
    InSync,           // leader_tip == follower_tip (including both None)
    FollowerAhead,    // follower_tip > leader_tip
    LeaderAhead,      // leader_tip > follower_tip AND follower_tip exists
    LeaderAheadEmpty, // leader_tip > follower_tip AND follower_tip is None
    Bootstrap,       // follower_tip is None AND leader_tip exists
    Divergent,        // same sequence, different hashes
    Unknown,          // insufficient info to classify (fail-closed)
}
```

### Diff Classification Logic

```
classify(follower_tip, leader_tip):

  // Both empty or both match -> in sync
  if follower_tip is None AND leader_tip is None:
    return InSync
  if follower_tip == leader_tip:
    return InSync

  // Follower has entries but leader does not -> follower ahead (local commits)
  if follower_tip exists AND leader_tip is None:
    return FollowerAhead

  // Leader has entries but follower does not -> bootstrap case
  if follower_tip is None AND leader_tip exists:
    return Bootstrap

  // Both have entries: compare sequences
  if follower_tip.sequence > leader_tip.sequence:
    return FollowerAhead

  if leader_tip.sequence > follower_tip.sequence:
    // Need hash continuity check to detect divergence
    // Sync-2 is read-only: we cannot fetch hash_path from leader yet
    // So we classify as LeaderAhead with Unknown flag until hash_path
    // is available (Sync-3+ territory)
    return LeaderAhead

  if follower_tip.sequence == leader_tip.sequence:
    if follower_tip.hash == leader_tip.hash:
      return InSync
    else:
      return Divergent

  return Unknown  // fail-closed on any ambiguity
```

### Fail-Closed Unknown Handling

If any input is `None` or cannot be reliably determined locally:
- Return `Unknown`
- `Unknown` is treated as "do not sync" in the decision table
- This prevents any sync attempt when classification is ambiguous

---

## Decision Table (Sync-2 Read-Only View)

Sync-2 produces a `DiffClass` that feeds the Sync-1 decision table:

| DiffClass              | Sync-1 Decision  | Next Action                          |
|------------------------|------------------|--------------------------------------|
| InSync                 | DONE             | No sync needed                       |
| FollowerAhead          | ABORT (A4)        | Do not sync; local commits ahead      |
| LeaderAhead            | SYNC             | Prepare to fetch entries N+1..M      |
| LeaderAheadEmpty       | FAST_FORWARD     | Prepare to fetch entries 0..M         |
| Bootstrap              | FAST_FORWARD     | Prepare to fetch entries 0..M         |
| Divergent              | ABORT (A3/A6)    | Do not sync; chains diverge          |
| Unknown                | ABORT (A0)       | Do not sync; insufficient info        |

Sync-2 does NOT execute the "Next Action" column. It only provides the
classification. Execution remains Sync-1 territory.

---

## Relationship to Sync-1

Sync-2 extends Sync-1's preflight phase (PF1-PF4) with additional local-only
checks (PF5-PF8) and adds a read-only diff classifier that maps two ledger
tips to a decision-category.

Sync-1's decision table already covers the outcomes. Sync-2 provides the
read-only machinery to safely produce the inputs for that table.

Sync-2 does NOT replace Sync-1's Phase 2 (Apply Decision). It clarifies
the pre-conditions and provides a named classifier for the decision logic.

---

## Open Questions (Sync-2 Output — Deferred to Sync-3+)

1. **Hash path fetching:** To distinguish LeaderAhead from Divergent, the
   follower needs a hash_path from the leader. This requires network
   transport and is Sync-3+ territory.
2. **Leader tip bootstrapping:** How does the follower obtain the leader's
   tip without a transport? (Static config? Operator input? DNS? Sync-3+.)
3. **Retry backoff:** On transient failure, when and how to retry? (Not
   relevant for read-only Sync-2; relevant for Sync-3+ execution.)
4. **Sync session tracking:** Does the follower record that a sync is
   in-progress to prevent concurrent sync attempts? (State management,
   not read-only.)
5. **Acknowledgment semantics:** Does the leader record follower progress?
   (Leader-side concern, not follower read-only.)

---

## Recommended Next Slice After Sync-2

**Sync-3: Transport sketch** — see
`docs/implementation-path/21-sync-3-transport-sketch.md`.
After the read-only preflight + diff classifier is ratified, the next
slice defines minimal transport requirements that preserve the one-way
fast-forward model and enable the hash-path fetch needed to distinguish
LeaderAhead from Divergent. Consensus and peer discovery remain excluded.

Note: Sync-3 does not yet include write-path implementation.

---

## Key Files (Reference for Future Implementation)

| File | Role |
|------|------|
| `crates/ferrum-ledger/src/lib.rs:229` | `verify_entry` — core chain integrity check |
| `crates/ferrum-ledger/src/lib.rs:260` | `verify_chain` — full chain verification |
| `crates/ferrum-store/src/repos.rs:155` | LedgerRepo trait shape |
| `crates/ferrum-store/src/sqlite/mod.rs:155` | SQLite ledger repo implementation |
| `docs/implementation-path/18-cross-node-ledger-sync-plan.md` | Sync-0 safety contract (predecessor) |
| `docs/implementation-path/19-sync-1-protocol-sketch.md` | Sync-1 protocol sketch (predecessor) |

---

## References

- Sync-0 (safety contract): `docs/implementation-path/18-cross-node-ledger-sync-plan.md`
- Sync-1 (protocol sketch): `docs/implementation-path/19-sync-1-protocol-sketch.md`
- Sync-1 defines: protocol phases, preflight checks (PF1-PF4), decision table, abort triggers (A1-A8), silent-skip handling (S1-S2), error taxonomy
- Sync-2 adds: extended preflight checks (PF5-PF8), read-only diff classifier (DiffClass enum), fail-closed Unknown handling, decision table mapping
- Sync-2 explicitly excludes: transport, consensus, two-way merge, peer discovery, write-path
