# FerrumGate Concepts Guide

> **Parent**: [`guides/README.md`](./README.md)

---

## What FerrumGate does

FerrumGate is an **intent-scoped execution gateway**. Before any side effect occurs, a user or agent declares what they want to do (the *intent*). The gateway evaluates that intent against policy, mints a time-bound *capability*, prepares and executes the action, verifies the outcome, and records the full *lineage* for audit and rollback.

---

## Core concepts

### Intent

A user or agent declares **what they want to do**.

- Contains: `action`, `target`, `parameters`, `requested_ttl_secs`
- Immutable after submission
- Example: write a file, send an HTTP request, create a git branch

Intents are compiled via `POST /v1/intents/compile` and receive a stable `intent_id`. The intent record is retained for the full lifecycle and is queryable through provenance.

### Proposal

The gateway evaluates an intent against active policy and produces a **proposal**.

- Contains: `decision` (Allow / Deny / Quarantine / RequireApproval / AllowDraftOnly)
- If `Allow`: proceed to capability minting
- If `RequireApproval`: human operator must approve before proceeding
- If `Deny`: blocked immediately

Proposals link an intent to the policy bundle version that was active at evaluation time. This link is preserved in provenance.

### Policy decision

The PDP (Policy Decision Point) evaluates:

- Trust labels
- Taint scoring
- Contradiction checks
- Active policy bundle rules

Decisions are deterministic and auditable. The active policy bundle can be switched via `PUT /v1/policy-bundles/{bundle_id}/active`; this action itself emits an audit event.

### Capability

A time-bound, single-use authorization to execute a prepared action.

- TTL max: **300 seconds**
- Single-use only
- Can be revoked via `POST /v1/capabilities/{capability_id}/revoke`
- Scope must not exceed the original intent

Capabilities bridge policy evaluation and execution. A capability carries the `rollback_class` and `adapter` information needed for prepare/execute/verify.

### Approval

For high-risk actions (R3), the gateway may require human approval.

- Operator inspects intent, risk, and compensation plan
- Operator approves or rejects with reason via `POST /v1/approvals/{approval_id}/resolve`
- R3 **never auto-commits**

Approval records are part of the provenance chain and include `actor_id`, `actor_label`, and `reason`.

### Rollback class

Every operation is classified by reversibility:

| Class | Meaning | Example |
|-------|---------|---------|
| R0 | No-op / read-only | health check, list intents |
| R1 | Reversible with compensation | file write with snapshot |
| R2 | Reversible with external cleanup | git branch create/delete |
| R3 | Requires approval + compensation | destructive mutations |

The rollback class determines whether approval is required and what compensation strategy is prepared before execution.

### Provenance

Every significant event is recorded:

- `PolicyEvaluated`
- `CapabilityMinted`
- `ActionProposalSubmitted`
- `SideEffectPrepared`
- `ToolCallPrepared`
- `ToolCallExecuted`
- `SideEffectVerified`
- Terminal (`SideEffectCommitted` / `SideEffectCompensated` / `SideEffectRolledBack`)
- `ErrorRaised` for fail-closed terminal failures such as incomplete recovery

Provenance events are queryable via `POST /v1/provenance/query` and `GET /v1/provenance/lineage/{execution_id}`.

### Lineage

The directed graph of provenance events for an execution.

- Query via `/v1/provenance/lineage/{execution_id}`
- Supports ancestor/descendant traversal
- Cycle-protected

The minimum lineage chain before a side effect is committed:

```
PolicyEvaluated
  → CapabilityMinted
  → ActionProposalSubmitted
  → SideEffectPrepared
  → ToolCallPrepared
  → ToolCallExecuted
  → SideEffectVerified
  → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)
```

If any required step is missing, the gateway fails closed and will not commit.

If a compensation or rollback adapter returns `recovered=false`, the gateway treats the attempt as
`recovery-incomplete`: the rollback contract and execution move to `Failed`, the HTTP response is
not marked compensated/rolled back, and provenance emits `ErrorRaised` with recovery metadata.

### Adapter

An implementation of a specific execution domain:

| Adapter | Operations |
|---------|-----------|
| fs | FileWrite, FileDelete, FileMove, FileCopy, DirCreate, DirDelete, FileAppend, FileChmod |
| git | GitCommit, GitBranchCreate, GitTagCreate, GitTagDelete, GitBranchDelete |
| http | HttpMutation, replay |
| sqlite | SQL mutation with transaction rollback |
| maildraft | Create/update/delete draft emails |

Adapters are responsible for prepare, execute, verify, and compensate steps within their domain. Each adapter declares its default rollback class per operation.

### Risk levels

| Level | Name | Trigger |
|-------|------|---------|
| R0 | Safe | Read-only, no side effects |
| R1 | Reversible | Side effects with automatic compensation |
| R2 | Recoverable | Side effects with manual recovery possible |
| R3 | Critical | Destructive; requires approval |

Risk level and rollback class are related but distinct: rollback class is an operational classification, while risk level is the policy-facing label that triggers approval gates.

### Provenance vs Audit Log

FerrumGate records two related but distinct audit surfaces:

| Surface | What it records | Query path | Trust boundary |
|--------|----------------|------------|---------------|
| **Provenance** | Intent-to-execution lifecycle events (PolicyEvaluated, CapabilityMinted, ActionProposalSubmitted, SideEffectPrepared, ToolCallPrepared, ToolCallExecuted, SideEffectVerified, Terminal states) | `POST /v1/provenance/query`, `GET /v1/provenance/lineage/{execution_id}` | Gateway-issued; chain enforced by store-backed transitions |
| **Audit log** | Administrative mutations (token creation, policy activation, approval resolution) + their SHA-256 hash chain and Merkle roots | `GET /v1/admin/audit/verify`, `ferrumctl admin audit verify` | Append-only by design; tamper-evident via `previous_hash` linkage and recomputation checks |

**Key distinction:**
- **Provenance** answers *"What happened for this specific execution?"* — scoped to one intent's lifecycle.
- **Audit log** answers *"What administrative changes were made to the system?"* — scoped to operator-level mutations.

**What is NOT claimed:**
- This is **not** a WORM (Write Once Read Many) storage system.
- This is **not** a compliance-certified audit trail (SOC 2, ISO 27001, etc.).
- A privileged attacker with full database access can still rewrite history if they recompute the entire chain. External anchoring or WORM sinks would be required for stronger guarantees; these are not yet implemented.

See [`docs/architecture/tamper-evident-audit-design.md`](../architecture/tamper-evident-audit-design.md) for the SHA-256/Merkle/Ed25519 design.

---

```
┌─────────┐     ┌──────────┐     ┌─────────┐     ┌──────────┐
│  Intent │────→│ Proposal │────→│Capability│────→│ Execution│
│ Compile │     │ Evaluate │     │  Mint   │     │ Prepare  │
└─────────┘     └──────────┘     └─────────┘     └──────────┘
                                                      │
                      ┌──────────┐                   ▼
                      │ Provenance│←────────────── Execute
                      │  /Lineage │←────────────── Verify
                      └──────────┘←────────────── Evaluate Outcome
```

All execution paths pass through the gateway. No adapter is invoked directly without a capability.

## Related docs

- [`quickstart.md`](./quickstart.md) — Hands-on 10-minute tutorial.
- [`policy-authoring.md`](./policy-authoring.md) — How to write policy bundles.
- [`mcp-integration.md`](./mcp-integration.md) — Using FerrumGate via MCP.
- [`api.md`](./api.md) — Endpoint reference and lifecycle.
- [`operator.md`](./operator.md) — Config, health, backup, and incident response.
