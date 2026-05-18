# FerrumGate Concepts Guide

> **Status**: Scaffold. Definitions are accurate; examples will expand.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Core concepts

### Intent

A user or agent declares **what they want to do**.

- Contains: `action`, `target`, `parameters`, `requested_ttl_secs`
- Immutable after submission
- Example: write a file, send an HTTP request, create a git branch

### Proposal

The gateway evaluates an intent against active policy and produces a **proposal**.

- Contains: `decision` (Allow / Deny / RequireApproval / Quarantine / DraftOnly)
- If `Allow`: proceed to capability minting
- If `RequireApproval`: human operator must approve before proceeding
- If `Deny`: blocked immediately

### Policy decision

The PDP (Policy Decision Point) evaluates:

- Trust labels
- Taint scoring
- Contradiction checks
- Active policy bundle rules

Decisions are deterministic and auditable.

### Capability

A time-bound, single-use authorization to execute a prepared action.

- TTL max: **300 seconds**
- Single-use only
- Can be revoked
- Scope must not exceed the original intent

### Approval

For high-risk actions (R3), the gateway may require human approval.

- Operator inspects intent, risk, and compensation plan
- Operator approves or rejects with reason
- R3 **never auto-commits**

### Rollback class

| Class | Meaning | Example |
|-------|---------|---------|
| R0 | No-op / read-only | health check, list intents |
| R1 | Reversible with compensation | file write with snapshot |
| R2 | Reversible with external cleanup | git branch create/delete |
| R3 | Requires approval + compensation | destructive mutations |

### Provenance

Every significant event is recorded:

- PolicyEvaluated
- CapabilityMinted
- ActionProposalSubmitted
- SideEffectPrepared
- ToolCallPrepared
- ToolCallExecuted
- SideEffectVerified
- Terminal (SideEffectCommitted / SideEffectCompensated / SideEffectRolledBack)

### Lineage

The directed graph of provenance events for an execution.

- Query via `/v1/lineage`
- Supports ancestor/descendant traversal
- Cycle-protected

### Adapter

An implementation of a specific execution domain:

| Adapter | Operations |
|---------|-----------|
| fs | FileWrite, FileDelete, FileMove, FileCopy, DirCreate, DirDelete, FileAppend, FileChmod |
| git | GitCommit, GitBranchCreate, GitTagCreate, GitTagDelete, GitBranchDelete |
| http | HttpMutation, replay |
| sqlite | SQL mutation with transaction rollback |
| maildraft | Create/update/delete draft emails |

### Risk tiers

| Tier | Name | Trigger |
|------|------|---------|
| R0 | Safe | Read-only, no side effects |
| R1 | Reversible | Side effects with automatic compensation |
| R2 | Recoverable | Side effects with manual recovery possible |
| R3 | Critical | Destructive; requires approval |

## Status caveat

> **production-ready = NO**. These concepts describe the intended governance model. Not all features are fully hardened for unbounded production use. See [`docs/ROADMAP.md`](../../ROADMAP.md) for gaps.

## Related docs

- [`quickstart.md`](./quickstart.md) — Hands-on 10-minute tutorial.
- [`policy-authoring.md`](./policy-authoring.md) — How to write policy bundles.
- [`mcp-integration.md`](./mcp-integration.md) — Using FerrumGate via MCP.
