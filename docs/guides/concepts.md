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

- `ActionProposalSubmitted`
- `PolicyEvaluated`
- `CapabilityMinted`
- `ToolCallPrepared`
- `ToolCallExecuted`
- `SideEffectPrepared`
- `SideEffectVerified`
- Terminal (`SideEffectCommitted` / `SideEffectCompensated` / `SideEffectRolledBack`)

Provenance events are queryable via `POST /v1/provenance/query` and `GET /v1/provenance/lineage/{execution_id}`.

### Lineage

The directed graph of provenance events for an execution.

- Query via `/v1/provenance/lineage/{execution_id}`
- Supports ancestor/descendant traversal
- Cycle-protected

The minimum lineage chain before a side effect is committed:

```
ActionProposalSubmitted
  → PolicyEvaluated
  → CapabilityMinted
  → SideEffectPrepared
  → ToolCallPrepared
  → ToolCallExecuted
  → SideEffectVerified
  → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)
```

If any required step is missing, the gateway fails closed and will not commit.

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

---

## Architecture at a glance

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
