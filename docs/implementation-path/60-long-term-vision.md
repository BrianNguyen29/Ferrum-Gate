# 60 — Long-Term Vision

**Last updated:** 2026-04-12
**Status:** VISION — non-binding, exploratory, not a roadmap
**Owner:** Core team (vision framing)
**Audience:** Strategic planners, potential contributors, architecture reviewers

---

## Purpose

This document describes FerrumGate's **long-term strategic intent** — where the project intends to go and why, grounded in its four core invariants.

This document is **explicitly non-binding**. It does not:
- Ratify any feature or milestone
- Create a commitment or roadmap item
- Override the v1 support contract or the v2 ratified contract
- Alter support boundaries or scope classifications

It is distinct from:
- **`19-v1-single-node-support-contract.md`** — what is supported today (contract, binding)
- **`20-v2-single-node-production-support-contract.md`** — v2 scope (contract, ratified)
- **`50-post-v2-roadmap.md`** — sequenced backlog of near/mid/long-term items (roadmap, non-binding)
- **`30-production-roadmap.md`** — current execution plan for v1/v2 (roadmap, near-term)

This vision doc describes the **end state capability plane** FerrumGate is building toward, not the execution path to get there.

---

## Relationship to Other Documents

| Doc | Role | Binding? |
|-----|------|----------|
| `19-v1-single-node-support-contract.md` | Today's support contract | **Yes — ratified for v1 scope** |
| `20-v2-single-node-production-support-contract.md` | v2 scope | **Ratified** |
| `30-production-roadmap.md` | Near-term execution plan (v1/v2) | No — current execution |
| `50-post-v2-roadmap.md` | Post-v2 backlog with Horizons H1/H2/H3 | No — sequencing, not promises |
| **This doc** | Long-term end-state vision | **No — direction, not commitment** |

The four invariants (Intent, Capability, Provenance, Rollback) are the **constant** across all
documents. They do not change with v1, v2, or any future version.

---

## FerrumGate's Four Invariants — Long-Term Interpretation

### Intent — Every mutation starts with a clear, auditable intent

**Long-term vision:** FerrumGate becomes the intent governance layer for any agent runtime.
Every action taken by any tool-using agent within any runtime passes through an intent
submission, evaluation, and authorization flow. Intent is the primary artifact —
more durable than the action itself, retained independently of execution state.

Future planes:
- **Multi-runtime intent federation** — a single intent can span multiple FerrumGate
  runtimes; intent ID is the correlation key across runtime boundaries
- **Structured intent authoring** — policy authors use intent templates with rich
  outcome clauses, temporal constraints, and scope selectors; authoring tooling
  makes intent creation first-class rather than API-only
- **Intent replay and simulation** — before committing to execution, operators can
  simulate intent outcome against a shadow store to see what would happen

### Capability — Authority is minimal, scoped, and time-bounded

**Long-term vision:** FerrumGate's capability model becomes the standard for least-privilege
agent tool execution. Capabilities are minted per intent, scoped to the exact parameters
of the action, short-lived, and consumable — never reusable. The model is expressive
enough for complex multi-step workflows but always enforces the minimum necessary grant.

Future planes:
- **Hierarchical capability scopes** — nested scope refinement where a parent intent
  can spawn child capabilities with tighter bounds
- **Capability revocation and expiry** — time-bounded capabilities that self-revoke;
  explicit revocation APIs for operators
- **Cross-adapter composite capabilities** — a single capability governing multi-adapter
  transactions (e.g., fs + http + maildraft in one coordinated intent)

### Provenance — Every meaningful side effect carries enough lineage to be traced

**Long-term vision:** FerrumGate's provenance graph is the authoritative record of what
happened, when, why (intent), and under whose authority. It is queryable, exportable,
and serves as the audit trail for compliance, debugging, and recovery. Provenance is
not an afterthought — it is woven into every execution path from the start.

Future planes:
- **Cross-runtime provenance fabric** — unified lineage graph spanning multiple
  FerrumGate runtimes; trust federation across runtime boundaries; query API that
  spans the mesh, not a single runtime
- **Provenance streaming and observability** — real-time lineage event streaming to
  external observability platforms; structured audit log emission as a first-class
  output of every execution
- **Provenance compression and archival** — long-term retention policies with cryptographic
  integrity verification; efficient lineage query over archived state

### Rollback — Every registered mutation has a recovery path appropriate to its risk class

**Long-term vision:** FerrumGate's rollback contract model is the standard recovery
primitive for agentic tool execution. R0/R1/R2/R3 are understood, enforceable, and
correct by construction — not by convention. The reversible execution planner
coordinates multi-step compensation graphs that span adapter boundaries, optimizing
for minimal disruption while preserving safety.

Future planes:
- **Reversible execution planner (U2)** — full execution reversal with dependency-aware
  planning; multi-step compensation graph optimization; cross-adapter rollback
  sequencing that respects adapter-specific recovery semantics
- **Adaptive rollback policy** — rollback strategy selected by risk class and runtime
  context; higher-risk mutations get more conservative recovery paths by default
- **Rollback verification and attestation** — every rollback action is itself verified
  and provenance-tracked; rollback attestations serve as audit evidence for
  compliance

---

## Long-Term Capability Planes

The following planes describe high-level capability directions. Each is grounded in
the four invariants. Items here are **not** roadmap items — they describe directions,
not commitments.

### Plane A — Intent-Scoped Agent Governance

FerrumGate as the **control plane for any MCP/tool-using agent runtime**, not just
a single-node sidecar.

- Universal intent submission surface — agents from any runtime submit intent via
  a standard protocol (MCP-native binding)
- Multi-tenant intent isolation — scoped tenant boundaries with cross-tenant intent
  correlation
- Policy bundle authoring tooling — first-class CLI for intent/policy creation,
  migration, and versioning

**Invariant mapping:** Intent (primary), Capability (scope enforcement), Provenance (lineage), Rollback (recovery)

### Plane B — Multi-Runtime Provenance Mesh

FerrumGate as the **provenance authority** across a mesh of runtimes.

- Cross-runtime lineage correlation — a single intent executed across runtimes
  produces a unified provenance graph
- Trust federation — runtime boundaries are treated as trust domains with
  explicit federation rules
- Export and compliance — structured audit output for compliance frameworks
  (SOC2, ISO 27001, etc.)

**Invariant mapping:** Provenance (primary), Intent (correlation key), Capability (cross-runtime scope), Rollback (cross-runtime recovery)

### Plane C — Reversible Execution Plane

FerrumGate as the **standard recovery primitive** for agentic tool execution.

- Full reversible execution planner with dependency-aware compensation graphs
- Cross-adapter rollback sequencing that preserves transactional safety
- Rollback verification and attestation as first-class outputs

**Invariant mapping:** Rollback (primary), Capability (scope), Provenance (verification), Intent (tracking)

### Plane D — HA / Multi-Node Topology

FerrumGate as a **production-grade distributed control plane**.

- Multi-leader replication with leader election
- Write-ahead log replication with conflict resolution
- Read-replica routing for read-heavy workloads

**Invariant mapping:** All four invariants remain constant; the topology changes
how they are enforced in a distributed context.

---

## How Vision Maps to Current Backlog

The Horizons in `50-post-v2-roadmap.md` (H1/H2/H3) are the **execution path** toward
the capability planes described above. The mapping below shows which vision plane
each Horizon advances:

| Horizon | Vision Plane | Key Items |
|---------|-------------|-----------|
| H1 (near-term post-v2) | Plane A (Intent-Scoped Governance) | Policy bundle tooling (H1.1), U1 expressiveness + authoring CLI (H1.2) |
| H2 (next capability) | Plane D (HA), Plane C (Reversible Execution) | HA/multi-leader (H2.1), U2 Reversible Planner (H2.2) |
| H3 (long-term/deferred) | Plane B (Provenance Mesh), Plane A (Multi-runtime), Plane D (Distributed) | U3 Provenance Fabric (H3.1), U4 Runtime Integrations (H3.2), Multi-node distributed (H3.4) |

---

## What This Document Does Not Change

- **v1 support contract** (`19-v1-single-node-support-contract.md`) — unchanged; **✅ RATIFIED for v1 scope**
- **v2 support contract** (`20-v2-single-node-production-support-contract.md`) — unchanged; **✅ RATIFIED for v2 single-node scope**
- **roadmap execution order** — Horizons in `50-post-v2-roadmap.md` are not reordered by this doc
- **support tier boundaries** — T1/T2/T3 scope is unchanged

---

## Invariant Stability Statement

The four invariants (Intent, Capability, Provenance, Rollback) are **design constraints**,
not feature descriptions. They will not change as FerrumGate evolves through v1, v2,
and beyond. Any future capability plane must respect these invariants; if a proposed
capability cannot be implemented within the invariant boundaries, the invariant takes
precedence.

---

## Key References

| Topic | File | Status |
|-------|------|--------|
| v1 support contract | `19-v1-single-node-support-contract.md` | ✅ RATIFIED for v1 scope |
| v2 support contract | `20-v2-single-node-production-support-contract.md` | ✅ RATIFIED for v2 single-node scope |
| Production roadmap (v1/v2) | `30-production-roadmap.md` | Current execution |
| Post-v2 roadmap (H1/H2/H3) | `50-post-v2-roadmap.md` | Post-v2 backlog |
| Project canon (invariants) | `00-project-canon.md` Section 5 | Source of truth for invariants |