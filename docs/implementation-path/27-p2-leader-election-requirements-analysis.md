# 27 — P2.4b Leader-Election Requirements Analysis

## Overview

This document analyzes leader-election requirements for FerrumGate multi-node
deployment. It is **analysis/decision only**; no implementation, no consensus
algorithm code, no transport design, and no write-path changes.

ASCII only.

**Scope**: Bounded to P2.4b analysis slice. This doc is the analysis output
for P2.4b. It is a direct continuation of P2.4a (`26-p2-sqlite-read-replica-use-cases.md`),
which established read-replica use cases and non-goals. Leader-election is the
complement that enables write-path HA and multi-node writes.

**Context**: FerrumGate v1 is scoped to single-node SQLite deployment.
P2.4b is a pre-implementation analysis step that should complete before any
future HA or multi-node write implementation begins. This doc is not a
blocker for v1 RC.

---

## 1. Relationship to P2.4a

P2.4a (`26-p2-sqlite-read-replica-use-cases.md`) established:

- Sanctioned read-only use cases for SQLite read-replicas
- Explicit non-goals (write forwarding, two-way sync, HA failover, etc.)
- Operational requirements for replica readiness

P2.4b is the complement: how does a node become the leader? This is required
for:

- HA automatic failover (when leader dies, a follower promotes)
- Multi-node write-path (leader coordinates writes across nodes)
- Consensus-based sync (leader election is a prerequisite for Raft/Paxos)

**P2.4b does not implement HA failover, multi-node writes, or consensus.
It defines the requirements and makes a technology recommendation that will
guide future implementation planning.**

---

## 2. System Model

### 2.1 Nodes and Roles

| Role | Description |
|------|-------------|
| Leader | Single node that receives writes, sequences entries, and replicates to followers. Only one leader at a time per cluster. |
| Follower | Node that receives replicated entries from leader. Participates in election. May serve reads (see P2.4a). |
| Candidate | Follower that is attempting to become leader via election. |
| Observer | Optional node that receives entries but does not vote or elect. Used for read-scaling without write pressure. |

### 2.2 Assumptions

| Assumption | Detail |
|------------|--------|
| A1 | Nodes communicate over a network that may drop, delay, or reorder messages |
| A2 | A node may crash and restart |
| A3 | At most one leader exists per cluster at any given time (leader singularity) |
| A4 | A newly elected leader has all entries that were committed before the election started (leader completeness) |
| A5 | SQLite is the persistence layer; it is local to each node |
| A6 | The sync protocol (Sync-1/Sync-2/Sync-3) provides one-way fast-forward from leader to follower |

### 2.3 Non-Assumptions (Explicit Out of Scope)

| Non-Assumption | Reason |
|----------------|--------|
| NA1 | Byzantine fault tolerance (BFT) — not required for current threat model |
| NA2 | Global unique event_id generation across nodes — local UUIDs are sufficient for now |
| NA3 | Cross-node transactional writes — out of scope for v1 |
| NA4 | Dynamic cluster membership changes — fixed cluster initial config is sufficient for v1 |

---

## 3. Functional Requirements

### 3.1 Leader Election

| Req ID | Requirement | Rationale |
|--------|-------------|-----------|
| LE1 | **Single leader at a time** — The system must ensure at most one leader exists per cluster. Two simultaneous leaders would produce divergent chains. | Core safety invariant |
| LE2 | **Eventual leader availability** — If a leader fails, a new leader must be elected within a bounded time. The bound is not tight (minutes-scale is acceptable for v1 HA). | Availability |
| LE3 | **Leader completeness** — A newly elected leader must have all committed entries. No committed data may be lost during election. | Safety: prevents silent data loss |
| LE4 | **Write continuation** — After election, the new leader must be able to accept writes within a bounded time. | Availability |
| LE5 | **Follower liveness** — Followers must detect leader failure and initiate election. | Detection |
| LE6 | **Log replication leadership** — Only the leader may append to the shared ledger. Followers receive appends via sync replication (Sync-1/Sync-2/Sync-3). | Single-writer invariant (L2 in Sync-0) |

### 3.2 Node Identity and Authentication

| Req ID | Requirement | Rationale |
|--------|-------------|-----------|
| NI1 | **Node identity** — Each node must have a stable, unique identifier. | Required for leader election state |
| NI2 | **Mutual authentication** — Nodes must authenticate each other before participating in election or sync. | Security boundary |
| NI3 | **Leader address propagation** — The current leader's address must be discoverable by followers for sync probe (Sync-3a). | Sync-3a probe requires leader tip |

### 3.3 Persistence and Recovery

| Req ID | Requirement | Rationale |
|--------|-------------|-----------|
| PR1 | **Election state persistence** — Election metadata (current term, voted-for, log length) must survive node restart. | Correct election recovery |
| PR2 | **Log persistence before acknowledgment** — An entry must not be acknowledged to the client until it is persisted locally and replicated to a quorum. | Durability |
| PR3 | **Clean restart** — A restarting node must rejoin the cluster correctly, either as follower or candidate. | Availability |

---

## 4. Comparison of Options

### 4.1 Option A — Raft (Recommended)

| Aspect | Detail |
|--------|--------|
| Description | Consensus algorithm with leader election built-in. Single leader handles all writes; log replication to followers; automatic failover. |
| Pros | - Well-understood; extensive production record - Single leader eliminates write conflicts - Built-in membership changes - Strong leader completeness guarantee - Good Rust ecosystem (`tokio-rs/loom`, `risinglightdb/tokio-raft`) |
| Cons | - Higher resource usage (memory, network) - Complex to implement correctly from scratch - Requires dedicated Raft log storage alongside SQLite - Not a drop-in SQLite wrapper |
| Fitness for LE1-LE6 | Satisfies all LE1-LE6 with standard Raft semantics |
| Implementation scope | Requires: Raft crate integration, SQLite + Raft log co-existence design, RPC layer, membership store |

### 4.2 Option B — External Coordinator (Etcd/Consul)

| Aspect | Detail |
|--------|--------|
| Description | Leader election delegated to external distributed store (etcd or Consul). Nodes acquire a lock/key; holder is leader. |
| Pros | - Battle-tested external service - Simple local implementation - Separates consensus from application logic |
| Cons | - External dependency (etcd/Consul cluster) - Adds operational overhead - Still need to design write coordination between nodes - Coordination is not leader election per se (is lock acquisition) |
| Fitness for LE1-LE6 | LE1-LE5 partially satisfied; LE6 (log replication) still needs design |
| Implementation scope | Requires: external cluster, lock acquisition logic, write forwarding design |

### 4.3 Option C — Custom Leader Election (Minimal)

| Aspect | Detail |
|--------|--------|
| Description | Simple heartbeat-based leader election with deterministic promotion rules. No log replication coordination. |
| Pros | - Minimal complexity - Can reuse existing Sync-3a probe infrastructure - Good for read-replica HA failover (promote follower to leader) |
| Cons | - Not true consensus; can have split-brain if network partitions - No log continuity guarantee across promotion - Requires careful design to avoid divergent chains on network partition - LE3 (leader completeness) is not guaranteed without log design |
| Fitness for LE1-LE6 | LE1, LE2, LE5 satisfied; LE3, LE4, LE6 require additional design |
| Implementation scope | Requires: heartbeat mechanism, deterministic promotion, manual conflict resolution |

### 4.4 Option D — Multi-Paxos

| Aspect | Detail |
|--------|--------|
| Description | Paxos family with leader election as a separate component. |
| Pros | - Strong theoretical guarantees - Flexible leader election |
| Cons | - Significantly more complex than Raft - Leader lease optimization adds complexity - Poor Rust ecosystem - Overkill for FerrumGate's use case |
| Fitness for LE1-LE6 | All satisfied in theory; implementation complexity is high |
| Implementation scope | Requires: Paxos implementation, leader lease, disk Paxos or Mencius optimization |

### 4.5 Comparison Matrix

| Criteria | Raft (A) | External Coord (B) | Custom (C) | Multi-Paxos (D) |
|----------|----------|-------------------|-----------|-----------------|
| LE1 (single leader) | Yes | Yes | Yes (if lucky) | Yes |
| LE2 (eventual leader) | Yes | Yes | Yes | Yes |
| LE3 (leader completeness) | Yes | Partial | No | Yes |
| LE4 (write continuation) | Yes | Partial | No | Yes |
| LE5 (follower liveness) | Yes | Yes | Yes | Yes |
| LE6 (log replication) | Yes | No | No | Yes |
| Implementation complexity | Medium | Low | Low | High |
| Production readiness | High | High | Low | Medium |
| Rust ecosystem | Good | N/A | N/A | Poor |
| External dependency | None | Yes (etcd/Consul) | None | None |

---

## 5. Recommendation

**Option A (Raft)** is recommended for the following reasons:

1. **LE3 (leader completeness) is non-negotiable.** The hash chain must never
   lose committed entries. Raft provides the strongest guarantee here with
   minimal custom design.

2. **Raft has mature prior art and established operational semantics.**
   Building on a well-understood consensus model reduces implementation risk
   versus inventing a custom election protocol.

3. **Single leader model aligns with Sync-1/Sync-2/Sync-3.** FerrumGate's
   one-way fast-forward sync already assumes a single leader. Raft's single
   leader model is a natural fit.

4. **Etcd/Consul adds operational burden.** FerrumGate is self-contained.
   Adding an external coordination cluster contradicts the self-contained
   deployment model.

5. **Custom election (C) has split-brain risk.** For a system that stores
   provenance and audit data, split-brain is unacceptable. The marginal
   complexity savings do not justify the risk.

**Recommended next step after this analysis**: Before implementing, a separate
**Raft integration design doc** must be created that addresses:

- Co-existence of SQLite ledger and Raft WAL
- Mapping from Raft committed entries to SQLite ledger entries
- Node configuration and initial cluster bootstrap
- Snapshot and log compaction strategy

This is a design doc, not an implementation. It is a prerequisite for any
future implementation slice.

---

## 6. Minimal Future Interface Contract

The following defines the minimal interface contract between the leader
election layer and the rest of the system. This is not implementation;
it is the boundary that any future implementation must satisfy.

### 6.1 Election State Machine

```
trait ElectionStateMachine {
    /// Returns the current node role
    fn role(&self) -> NodeRole; // Leader | Follower | Candidate | Observer

    /// Returns the current term number (monotonically increasing)
    fn term(&self) -> u64;

    /// Returns the leader's node ID, if known
    fn leader_id(&self) -> Option<NodeId>;

    /// Returns the leader's network address, if known
    fn leader_address(&self) -> Option<SocketAddr>;

    /// Returns true if this node can vote in the current term
    fn is_voter(&self) -> bool;
}

enum NodeRole {
    Leader,
    Follower,
    Candidate,
    Observer,
}
```

### 6.2 Leadership Readiness

```
/// Readiness probe for the sync subsystem.
/// The sync subsystem (Sync-3a probe) must know when leadership is stable
/// before attempting replication.
trait LeadershipReadiness {
    /// Returns true if the node is ready to serve as leader or actively
    /// replicating from a leader. Returns false during leader election or
    /// when no leader is known.
    fn is_leadership_stable(&self) -> bool;

    /// Returns the time since the last leader heartbeat, if a leader exists.
    fn time_since_last_leader_heartbeat(&self) -> Option<Duration>;
}
```

### 6.3 WAL Entry Interface

```
/// The leader election layer produces WAL entries.
/// The application layer (FerrumGate ledger) consumes committed entries.
trait WALEntry {
    fn data(&self) -> &[u8];
    fn index(&self) -> u64;
    fn term(&self) -> u64;
}

/// Callback invoked when a WAL entry is committed by Raft
trait CommittedEntryHandler {
    /// Called when a WAL entry is committed.
    /// The handler deserializes the entry and applies it to the SQLite ledger.
    async fn on_entry_committed(&self, entry: WALEntry) -> Result<(), AppError>;
}
```

### 6.4 Node Configuration

```
struct NodeConfig {
    node_id: NodeId,
    listen_address: SocketAddr,
    initial_cluster: Vec<PeerAddr>, // node_id -> address
    storage_path: PathBuf,           // for Raft log and state
}
```

---

## 7. Done Criteria

This slice is **done** when:

- [x] System model (Section 2) is documented with roles, assumptions, and
  non-assumptions
- [x] All functional requirements (LE1-LE6, NI1-NI3, PR1-PR3) are documented
  with rationale
- [x] All four options (Raft, External Coordinator, Custom, Multi-Paxos) are
  compared against the requirements
- [x] A recommendation (Option A — Raft) is made with explicit rationale
- [x] Minimal future interface contract (Section 6) is documented
- [x] A prerequisite Raft integration design doc is identified as future work
- [x] This doc is referenced from:
  - `24-p1-p2-p3-execution-plan.md` (P2.4b status row — change from TODO to DONE)
  - `23-production-readiness-assessment.md` (HA readiness section — P2.4b analysis complete)
  - `docs/implementation-path/README.md` (document index — add 27-p2-leader-election-requirements-analysis.md)
  - `26-p2-sqlite-read-replica-use-cases.md` (cross-reference to P2.4b as complement)

**This is an analysis-only slice. No code changes, no Raft implementation,
no transport design, and no write-path design are in scope.**

---

## 8. Relationships to Other Documents

| Document | Relationship |
|----------|--------------|
| `24-p1-p2-p3-execution-plan.md` | P2.4b is a row in the P2.4 HA readiness table; this doc is the analysis output |
| `23-production-readiness-assessment.md` | HA readiness section (2.7) references this doc as the analysis output for P2.4b |
| `26-p2-sqlite-read-replica-use-cases.md` | P2.4b is the write-path complement to P2.4a's read-replica analysis |
| `18-cross-node-ledger-sync-plan.md` | Leader election is a prerequisite for consensus; Sync-0 invariants L1-L3, C1-C3 apply; leader election does not modify sync semantics |
| `19-sync-1-protocol-sketch.md` | Sync-1 is one-way fast-forward from leader to follower; Raft leader is the Sync-1 source |
| `22a-sync-3a1-probe-api-boundary.md` | Sync-3a probe discovers leader tip; leader election provides the leader address; no sync semantics changes |

---

## 9. Future Work (Out of Scope for P2.4b)

The following are **explicitly deferred** and are not part of this slice:

| Item | Phase | Notes |
|------|-------|-------|
| Raft integration design doc | Future | Must address SQLite + Raft WAL co-existence before implementation |
| Raft implementation | Future | Requires integration design doc first |
| HA automatic failover implementation | Future | Requires Raft + integration design |
| Multi-node write-path | Future | Requires Raft + sync implementation |
| Observer node implementation | Future | Non-voting read-scaling node |
| Cluster membership changes | Future | Dynamic node addition/removal |
| Leader lease optimization | Future | For write availability during leader elections |
| BFT leader election | Beyond v1 | Not in threat model for v1 |
