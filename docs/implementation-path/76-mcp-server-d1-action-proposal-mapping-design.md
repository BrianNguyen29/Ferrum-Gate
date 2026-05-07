# 76 — MCP Server D-1 ActionProposal Mapping Design

> **Status**: Design documentation only. D-1.3.2 implementation is blocked — requires this design packet review and explicit approval gate before any ActionProposal mapping implementation begins.
> **D1.3.1 Status**: Types-only (`IntentCompileRequest` struct, 6 fields) — no HTTP calls, no pipeline logic. These placeholder types will be replaced/supplemented during D1.3.2 (see §1.4).
> **D1.3.2 Blocker**: This document identifies 7 mapping blockers (B-MAP-1 through B-MAP-7) that must be resolved before ActionProposal mapping can proceed.
> **D1.3.2a Update**: Pure mapping helpers are now implemented and verified. D-1.3.2b remains blocked by the reconciliation gates in doc 78.
> **Purpose**: Define the complete field mapping chain from MCP tool call arguments through `ToolCallAction` to gateway `IntentCompileRequest` to internal `ActionProposal`, and identify missing-field derivation rules.
> **Constraint**: Do not claim D1.3.2 is ready. Do not implement ActionProposal mapping. Do not call any governance endpoint. Preserve no production/G2/operator claims.

---

## Explicit Non-Claims

- **No D1.3.2 ready claim.** ActionProposal mapping design is incomplete — blocked by B-MAP-1 through B-MAP-7.
- **No field stability claim.** All mapping rules and inference logic are draft until gateway integration testing confirms actual field shapes.
- **No execution_id lifecycle claim corrected.** Doc 75 §2.1 incorrectly shows `execution_id` returned from eval/prepare. This document corrects that (see §5).
- **No ActionProposalSubmitted timing claim.** Provenance event emission timing is uncertain until B-MAP-7 is resolved.
- **No approval tool claim.** Approval/reject tools remain blocked per doc 75 §1.3 (B1).

---

## 0. Overview

### 0.1 Why This Document Exists

D1.3.1 (types-only) defined `IntentCompileRequest` as a struct with 6 fields matching `ToolCallAction`. However, explorer findings reveal the **actual gateway `IntentCompileRequest` has 12 fields**, and the gateway's internal `ActionProposal` has 13 fields.

This creates a **6-field gap** problem: 6 of 12 gateway `IntentCompileRequest` fields have no MCP source, and 6 of 13 `ActionProposal` fields need derivation/defaults.

D1.3.2 cannot proceed until these gaps have explicit derivation rules or are acknowledged as open questions.

### 0.2 Mapping Chain

> **E1: Gateway IntentCompileRequest has 12 fields (not 6, not 11).** D1.3.1 defined a 6-field placeholder. The actual gateway DTO has 12 fields: `principal_id`, `session_id`, `channel_id`, `title`, `goal`, `agent_plan_summary`, `trusted_context`, `raw_inputs`, `requested_resource_scope`, `requested_risk_tier`, `approval_mode`, `metadata`. Six of these have no MCP source and are classified in §2.1.

```
MCP Tool Call Arguments
        │
        ▼
ToolCallAction (6 fields, MCP-internal)
  - intent_id
  - action_type
  - scope
  - target
  - parameters
  - actor_id
        │
        ▼
Gateway IntentCompileRequest (12 fields, REST API DTO)
  - principal_id       ← needs derivation
  - session_id         ← needs derivation
  - channel_id         ← needs derivation
  - title              ← needs derivation
  - goal               ← needs derivation
  - agent_plan_summary ← needs derivation
  - trusted_context    ← needs derivation
  - raw_inputs         ← from parameters
  - requested_resource_scope ← from scope (parsing required, B-MAP-2)
  - requested_risk_tier     ← needs inference (B-MAP-3)
  - approval_mode      ← needs derivation (B-MAP-3)
  - metadata           ← needs derivation
        │
        ▼ (gateway internal)
ActionProposal (13 fields, gateway-internal)
  - proposal_id              ← gateway-assigned
  - intent_id                ← from ToolCallAction.intent_id
  - step_index               ← needs derivation
  - title                    ← from IntentCompileRequest.title
  - tool_name                ← from ToolCallAction.action_type
  - server_name              ← needs derivation (B-MAP-4)
  - raw_arguments            ← from ToolCallAction.parameters
  - expected_effect          ← needs inference (B-MAP-5)
  - estimated_risk           ← needs inference (B-MAP-5)
  - requested_rollback_class ← needs inference (B-MAP-6)
  - taint_inputs             ← needs derivation
  - metadata                 ← from IntentCompileRequest.metadata
  - created_at               ← gateway-assigned
```

### 0.3 Why D1.3.2 is Blocked

D1.3.2 is blocked because:

1. **B-MAP-1**: 6 of 12 gateway `IntentCompileRequest` fields have no MCP source (see §2.1 for optional/required classification).
2. **B-MAP-2**: `requested_resource_scope` requires parsing `scope` into a `ResourceSelector` — parsing rules undefined.
3. **B-MAP-3**: `requested_risk_tier` (RiskTier: Low/Medium/High/Critical) and `approval_mode` (ApprovalMode: None/Required/DraftOnly/TwoPhaseCommit) require inference from `action_type`.
4. **B-MAP-4**: `server_name` (ActionProposal) requires resolving from `target` or `action_type` — resolution rules undefined.
5. **B-MAP-5**: `expected_effect` and `estimated_risk` (ActionProposal) require inference from `action_type` and `parameters` — inference rules undefined.
6. **B-MAP-6**: `requested_rollback_class` (RollbackClass: R0NativeReversible/R1SnapshotRecoverable/R2Compensatable/R3IrreversibleHighConsequence) requires inference from `action_type` and `target`.
7. **B-MAP-7**: `ActionProposalSubmitted` provenance event timing is uncertain — may emit during `authorize` not `compile`.

---

## 1. Field Mapping Analysis

### 1.1 MCP ToolCallAction Fields (6 fields)

Source: `stage2_types.rs` and doc 74 §3.3.

| Field | Type | Source in MCP call | Notes |
|-------|------|-------------------|-------|
| `intent_id` | String (UUID) | Required from MCP client | Must be pre-generated by MCP client |
| `action_type` | String | Required from tool name | e.g., `"fs_write"`, `"git_push"` |
| `scope` | String | Required from MCP arguments | e.g., `"files:write:/tmp"` |
| `target` | String | Required from MCP arguments | Resource URI or identifier |
| `parameters` | JsonValue | Required from MCP arguments | Tool-specific JSON |
| `actor_id` | String | From `ActorIdentity::resolve()` | Resolved from env/init/fallback |

### 1.2 Gateway IntentCompileRequest Fields (12 fields)

Source: Explorer findings. **UNVERIFIED** — field names, types, and required/optional status may drift.

| Field | Type | MCP Source | Derivation Status |
|-------|------|-----------|-------------------|
| `principal_id` | String | None | **B-MAP-1**: Derive from `actor_id`? Use `actor_id` directly? |
| `session_id` | String | None | **B-MAP-1**: What is a session? How created? |
| `channel_id` | String | None | **B-MAP-1**: What is a channel? How identified? |
| `title` | String | None | **B-MAP-1**: Derive from `action_type` + `target`? |
| `goal` | String | None | **B-MAP-1**: Derive from `parameters`? What format? |
| `agent_plan_summary` | String | None | **B-MAP-1**: What is this? MCP has no plan concept. |
| `trusted_context` | Object | None | **B-MAP-1**: What structure? When set? |
| `raw_inputs` | Array | `parameters` | ✅ Direct mapping — parameters is JSON, needs conversion to array |
| `requested_resource_scope` | ResourceSelector | `scope` | **B-MAP-2**: Parse `"files:write:/tmp"` into `ResourceSelector` (tagged enum) |
| `requested_risk_tier` | RiskTier | None | **B-MAP-3**: Infer from `action_type` (enum: Low/Medium/High/Critical) |
| `approval_mode` | ApprovalMode | None | **B-MAP-3**: Derive from `requested_risk_tier` (enum: None/Required/DraftOnly/TwoPhaseCommit) |
| `metadata` | Object | None | **B-MAP-1**: Can be empty `{}` |

### 1.3 ActionProposal Fields (13 fields)

Source: Explorer findings. **UNVERIFIED** — field names and types may drift.

| Field | Type | Source | Derivation Status |
|-------|------|--------|-------------------|
| `proposal_id` | String | Gateway-assigned | ✅ Gateway assigns on compile |
| `intent_id` | String | `ToolCallAction.intent_id` | ✅ Direct pass-through |
| `step_index` | Integer | None | **B-MAP-1**: Default to `0`? Sequential per intent? |
| `title` | String | `IntentCompileRequest.title` | ✅ Pass-through |
| `tool_name` | String | `ToolCallAction.action_type` | ✅ Map `"fs_write"` → `"ferrum_gate_fs_write"`? |
| `server_name` | String | None | **B-MAP-4**: Resolve from `target` or `action_type` |
| `raw_arguments` | Object | `ToolCallAction.parameters` | ✅ Pass-through (parameters is JSON) |
| `expected_effect` | String | None | **B-MAP-5**: Inference from `action_type` + `parameters` |
| `estimated_risk` | RiskTier | None | **B-MAP-5**: Inference from `action_type` (enum: Low/Medium/High/Critical) |
| `requested_rollback_class` | RollbackClass | None | **B-MAP-6**: Inference from `action_type` (enum: R0NativeReversible/R1SnapshotRecoverable/R2Compensatable/R3IrreversibleHighConsequence) |
| `taint_inputs` | Array | None | **B-MAP-1**: What is this? Default empty? |
| `metadata` | Object | `IntentCompileRequest.metadata` | ✅ Pass-through |
| `created_at` | Timestamp | Gateway-assigned | ✅ Gateway assigns on creation |

### 1.4 IntentCompileResponse Shape (E6 Correction)

> **E6: IntentCompileResponse is `{ envelope: IntentEnvelope, warnings: Vec<String> }`, not `{ proposal_id }`.**

The D1.3.1 `IntentCompileResponse` struct (which only has `proposal_id`) is a **placeholder** that does not match the actual gateway response shape.

**Actual gateway IntentCompileResponse shape:**

```json
{
  "envelope": {
    "intent_id": "...",
    "proposal_id": "...",  // Note: proposal_id is nested inside envelope
    "status": "...",
    "created_at": "..."
  },
  "warnings": ["warning1", "warning2"]
}
```

**D1.3.1 placeholder vs actual:**

| Field | D1.3.1 Placeholder | Actual Gateway |
|-------|-------------------|---------------|
| `proposal_id` | Top-level field | Nested inside `envelope` |
| `warnings` | Not present | `Vec<String>` at top level |
| `envelope` | Not present | Wrapper object with intent metadata |

**Impact on D1.3.2:** The `IntentCompileResponse` parsing logic must be updated to handle the nested `envelope` structure and extract `proposal_id` from within it.

### 1.5 D1.3.1 Placeholder Types Must Be Replaced/Supplemented

> **E8: D1.3.1 types are placeholders.** The `IntentCompileRequest` struct defined in `stage2_types.rs` during D1.3.1 has only 6 fields (matching `ToolCallAction` 1:1). This is insufficient for the actual gateway API which requires 12 fields. During D1.3.2, these placeholder types must be:
>
> 1. **Replaced**: The 6-field `IntentCompileRequest` must be replaced with the full 12-field version including `principal_id`, `session_id`, `channel_id`, `title`, `goal`, `agent_plan_summary`, `trusted_context`, `requested_resource_scope` (ResourceSelector), `requested_risk_tier` (RiskTier), `approval_mode` (ApprovalMode), and `metadata`.
> 2. **Supplemented**: New types must be added: `RiskTier` enum (Low/Medium/High/Critical), `ApprovalMode` enum (None/Required/DraftOnly/TwoPhaseCommit), `ResourceSelector` tagged enum (FilesystemPath/GitRepository/SqliteDatabase/HttpEndpoint/EmailDraft/McpTool), and `RollbackClass` enum (R0NativeReversible/R1SnapshotRecoverable/R2Compensatable/R3IrreversibleHighConsequence).
> 3. **Validated**: All new types must be verified against real gateway schemas before D1.3.2 implementation proceeds.

---

## 2. Missing-Field Derivation Rules

### 2.1 B-MAP-1: Gateway IntentCompileRequest Missing Fields

> **E7: B-MAP-1 must distinguish optional from required/trivial/default fields.** Not all 6 missing fields are true blockers. Some may be optional, have defaults, or be trivially derived.

**Classification of 6 missing fields:**

| Field | Derivation Candidate | Classification | Status | Notes |
|-------|---------------------|----------------|--------|-------|
| `principal_id` | `actor_id` | **True blocker** | **Open** | No obvious default; gateway needs principal context |
| `session_id` | Auto-generate UUID | **Trivial** | **Resolvable** | Auto-generate UUID v4 per intent compile call |
| `channel_id` | `"default"` or `channel_id` from init | **Trivial** | **Resolvable** | Use constant `"default"` or from MCP init params |
| `title` | `"action_type: target"` | **Trivial** | **Resolvable** | Simple string concatenation; truncation rule needed |
| `goal` | `action_type` description | **True blocker** | **Open** | What format? Plain text? Markdown? MCP has no goal concept |
| `agent_plan_summary` | `""` (empty) | **Trivial** | **Resolvable** | Use empty string `""` — MCP has no plan concept |
| `trusted_context` | `null` or `{}` | **Optional** | **Likely resolvable** | May be optional; use `null` if not required |
| `approval_mode` | Derived from RiskTier | **Trivial** | **Resolvable** | Map RiskTier → ApprovalMode: Critical→Required, others→None |
| `metadata` | `{}` | **Trivial** | **Resolvable** | Empty object `{}` is a valid default |

**True blockers in B-MAP-1:**
1. `principal_id`: No default; needs mapping from `actor_id` to gateway principal
2. `goal`: No clear derivation; MCP has no goal/planning concept

**Likely resolvable (need confirmation):**
- `session_id`, `channel_id`, `title`, `agent_plan_summary`, `trusted_context`, `approval_mode`, `metadata`

**Open Questions:**
- Do optional fields have **default values** when omitted? If so, we can skip them.
- Is `trusted_context` actually required?
- Does gateway accept empty strings for `goal` and `agent_plan_summary`?

### 2.2 B-MAP-2: Scope Parsing to ResourceSelector

> **E5: ResourceSelector is a tagged enum**, not a flat object. The `scope` field (e.g., `"files:write:/tmp"`) must be parsed into the correct `ResourceSelector` variant.

**ResourceSelector tagged enum variants:**

```rust
enum ResourceSelector {
    FilesystemPath { path: String, access_mode: String },
    GitRepository { repo: String, branch: String, remote: String },
    SqliteDatabase { path: String, table: Option<String> },
    HttpEndpoint { url: String, method: String },
    EmailDraft { to: String, subject: Option<String> },
    McpTool { tool_name: String, adapter: String },
}
```

**Scope-to-ResourceSelector parsing rules:**

| scope pattern | ResourceSelector variant | Example |
|---------------|-------------------------|---------|
| `fs:read:/path` | `FilesystemPath { path: "/path", access_mode: "read" }` | `fs:read:/tmp/test.txt` |
| `fs:write:/path` | `FilesystemPath { path: "/path", access_mode: "write" }` | `fs:write:/tmp/test.txt` |
| `git:push:origin` | `GitRepository { repo: "origin", branch: "...", remote: "origin" }` | `git:push:origin` |
| `git:fetch:origin` | `GitRepository { repo: "origin", ... }` | `git:fetch:origin` |
| `sql:mutate:db` | `SqliteDatabase { path: "db", table: None }` | `sql:mutate:mydb.db` |
| `http:post:url` | `HttpEndpoint { url: "url", method: "POST" }` | `http:post:https://api.example.com` |
| `email:send:to` | `EmailDraft { to: "to", subject: None }` | `email:send:alice@example.com` |
| `mcp:tool:name` | `McpTool { tool_name: "name", adapter: "..." }` | `mcp:tool:ferrum_gate_fs_write` |

**Open Question**: Explorer did not confirm the exact `ResourceSelector` schema. The tagged enum structure above is the best available information. Q-MAP-1 must be resolved before B-MAP-2 is marked done.

### 2.3 B-MAP-3: RiskTier and ApprovalMode Inference

> **E3: RiskTier variants are Low, Medium, High, Critical.**
> **E4: ApprovalMode variants are None, Required, DraftOnly, TwoPhaseCommit.**

`requested_risk_tier` (RiskTier enum) and `approval_mode` (ApprovalMode enum) must be inferred from `action_type`.

**RiskTier enum (per E3):**

```rust
enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
}
```

**ApprovalMode enum (per E4):**

```rust
enum ApprovalMode {
    None,           // No approval required
    Required,       // Manual approval needed
    DraftOnly,     // Only draft intents need approval
    TwoPhaseCommit, // Two-phase commit approval process
}
```

**Proposed mapping from action_type to RiskTier:**

| action_type | RiskTier | Rationale |
|-------------|----------|-----------|
| `fs_read`, `git_fetch`, `http_get`, `db_query` | `Low` | Read-only, no state change |
| `fs_write` (non-critical path like /tmp) | `Low` | Limited blast radius |
| `http_post`, `git_push` (non-main branch) | `Medium` | Can affect external systems |
| `fs_write` (critical path like /etc, /usr) | `High` | System-level changes |
| `git_push` (main branch) | `High` | Production branch mutation |
| `sql_mutate`, `db_mutate`, `fs_delete` | `High` | Destructive operations |
| `git_force_push`, actions on production DBs | `Critical` | Potentially irrecoverable |

**Proposed mapping from RiskTier to ApprovalMode:**

| RiskTier | ApprovalMode | Rationale |
|----------|--------------|-----------|
| `Low` | `None` | No approval needed for low-risk actions |
| `Medium` | `DraftOnly` | Only draft intents need review |
| `High` | `Required` | Manual approval required |
| `Critical` | `TwoPhaseCommit` | Two-phase commit for critical actions |

**Open Questions:**
- Does `ApprovalMode::DraftOnly` apply to MCP-initiated intents? MCP has no "draft" concept.
- Does the gateway require `approval_mode` to be explicitly set, or does `None` mean "auto"?
- What is the behavior when `approval_mode` is `None` but the policy bundle requires approval?

### 2.4 B-MAP-4: server_name Resolution

`server_name` in `ActionProposal` identifies which adapter/server should handle execution.

**Proposed resolution rules:**

| `action_type` pattern | `server_name` |
|-----------------------|---------------|
| `fs_*` | `"filesystem"` |
| `git_*` | `"git"` |
| `http_*` | `"http"` |
| `sql_*`, `db_*` | `"database"` |
| `email_*` | `"email"` |
| `mcp:*` | From MCP adapter config |
| Unknown | **Open** — fallback? Error? |

**Open Question**: Q-MAP-3 — What is the valid `server_name` vocabulary? Does the gateway have a fixed set of server names, or is it dynamic?

### 2.5 B-MAP-5: expected_effect and estimated_risk Inference

These fields describe what the action will do and its risk level. `estimated_risk` uses `RiskTier` enum (Low/Medium/High/Critical).

**Proposed `expected_effect` values:**

| `action_type` | `expected_effect` |
|---------------|-------------------|
| `fs_write` | `"Create or overwrite file at target path"` |
| `fs_delete` | `"Delete file at target path"` |
| `git_push` | `"Push commits to remote"` |
| `git_force_push` | `"Force push to remote (may overwrite history)"` |
| `http_post` | `"Send POST request to target URL"` |
| `sql_mutate` | `"Execute mutation on database"` |

**`estimated_risk` values:** Uses `RiskTier` enum — same mapping as B-MAP-3.

### 2.6 B-MAP-6: Rollback Class Inference

> **E2: RollbackClass variants are R0NativeReversible, R1SnapshotRecoverable, R2Compensatable, R3IrreversibleHighConsequence.**

`requested_rollback_class` identifies the rollback strategy using the `RollbackClass` enum.

**RollbackClass enum (per E2):**

```rust
enum RollbackClass {
    R0NativeReversible,        // Action is natively reversible (e.g., read, fetch)
    R1SnapshotRecoverable,    // Recoverable via snapshot/backup (e.g., file write)
    R2Compensatable,          // Compensatable via reverse operation (e.g., git push, http post)
    R3IrreversibleHighConsequence, // Irreversible or high-consequence (e.g., force push, sql mutate)
}
```

**Proposed mapping from action_type to RollbackClass:**

| action_type | RollbackClass | Rationale |
|-------------|---------------|-----------|
| `fs_read`, `git_fetch`, `http_get`, `db_query` | `R0NativeReversible` | Read-only; no rollback needed |
| `fs_write` (create new file) | `R1SnapshotRecoverable` | File can be deleted |
| `fs_write` (overwrite existing) | `R1SnapshotRecoverable` | Recoverable from backup/snapshot |
| `fs_delete` | `R1SnapshotRecoverable` | May be recoverable from backup |
| `git_push` | `R2Compensatable` | Revert commit via git revert |
| `git_force_push` | `R3IrreversibleHighConsequence` | May overwrite remote history permanently |
| `http_post` | `R2Compensatable` | Compensating HTTP request (DELETE or revert) |
| `sql_mutate` | `R3IrreversibleHighConsequence` | SQL mutations may be irreversible |
| `db_mutate` | `R3IrreversibleHighConsequence` | Database mutations may be irreversible |

**Open Question**: Q-MAP-4 — What is the exact rollback class vocabulary? Does the gateway support all these classes? Are there additional classes?

---

## 3. Submit vs Evaluate Semantics

### 3.1 Doc 74 Design

Doc 74 §3.3 defines two separate MCP tools:
- `submit_intent`: Submits a new intent (compiles + evaluates?)
- `evaluate_intent`: Evaluates an existing intent

### 3.2 Explorer Findings

Explorer findings show:
- `POST /v1/intents/compile`: Compiles intent → returns `proposal_id`
- `POST /v1/proposals/{proposal_id}/evaluate`: Evaluates proposal → may return `execution_id`

### 3.3 Semantic Ambiguity

| Question | Impact | Status |
|----------|--------|--------|
| Does `submit_intent` compile ONLY, or compile + auto-evaluate? | Affects D1.3.2/D1.3.3 scope | **Open** (B3 in doc 75) |
| Is `evaluate_intent` for re-evaluating a denied proposal? | Affects tool semantics | **Open** (B3 in doc 75) |
| Can a proposal be evaluated multiple times? | Affects idempotency | **Open** |
| Does eval return `execution_id` directly or only after prepare? | Affects sequential ID flow | **Open** (Q3 in doc 75) |

**This ambiguity is B3 from doc 75 and remains unresolved.**

---

## 4. Sequential ID Flow Correction

### 4.1 Doc 75 Error

Doc 75 §2.1 shows this sequential flow:

```
POST /v1/intents/compile → returns proposal_id
POST /v1/proposals/{proposal_id}/evaluate → returns execution_id  ← INCORRECT
POST /v1/capabilities/mint → returns capability_id
...
```

### 4.2 Corrected Flow

Explorer findings indicate `execution_id` is created during `authorize_execution`, NOT during eval or prepare.

**Corrected sequential ID flow:**

```
POST /v1/intents/compile
  → Gateway creates ActionProposal
  → Returns { proposal_id: "uuid-1" }

POST /v1/proposals/{proposal_id}/evaluate
  → Gateway evaluates policy
  → Returns { decision: "allow", ... }  [NOTE: may NOT return execution_id here]

POST /v1/capabilities/mint
  → Gateway creates single-use capability
  → Returns { capability_id: "uuid-2", ttl: 300 }

POST /v1/executions/authorize
  → Gateway authorizes execution
  → Creates execution record
  → Returns { authorized: true, execution_id: "uuid-3" }  ← execution_id created HERE

POST /v1/executions/{execution_id}/prepare
  → Gateway prepares rollback contract
  → Returns { rollback_contract_id: "uuid-4", ... }

POST /v1/executions/{execution_id}/execute
  → Gateway executes action
  → Returns { status: "completed" | "pending" }
```

### 4.3 Impact on D1.3.2 Design

If `execution_id` is NOT returned from eval, then:
- D1.3.4 (implement eval call) cannot return `execution_id`
- The MCP server must call `authorize` before `prepare`
- D1.3.5 (implement prepare) must accept `execution_id` as input from authorize step

---

## 5. Provenance Event Timing

### 5.1 Expected Lineage Chain (per AGENTS.md)

```
ActionProposalSubmitted → PolicyEvaluated → CapabilityMinted → ToolCallPrepared → ToolCallExecuted → SideEffectPrepared → SideEffectVerified → Terminal
```

### 5.2 Explorer Findings on Event Emission

Explorer did not return explicit provenance event emission. Based on REST endpoint analysis:

| REST Call | Expected Provenance Event | Timing Uncertainty |
|-----------|--------------------------|-------------------|
| POST /v1/intents/compile | `ActionProposalSubmitted` | **Uncertain** — may emit here or at authorize |
| POST /v1/proposals/{id}/evaluate | `PolicyEvaluated` | Likely here |
| POST /v1/capabilities/mint | `CapabilityMinted` | Likely here |
| POST /v1/executions/authorize | `ActionProposalSubmitted`? | **Uncertain** — may emit here instead of compile |
| POST /v1/executions/{id}/prepare | `ToolCallPrepared` | Likely here |
| POST /v1/executions/{id}/execute | `ToolCallExecuted` | Likely here |

### 5.3 B-MAP-7: ActionProposalSubmitted Timing Mismatch

If `ActionProposalSubmitted` emits during `authorize` instead of `compile`:
- The lineage chain starts later than expected
- MCP server cannot rely on compile call triggering the first provenance event
- Provenance logging must account for this uncertainty

**Required**: Gateway team must confirm when `ActionProposalSubmitted` actually emits.

---

## 6. Blocker Summary

### 6.1 🔴 Mapping Blockers (B-MAP-1 through B-MAP-7)

| Blocker | Description | Impact | Owner |
|---------|-------------|--------|-------|
| **B-MAP-1** | 6 gateway IntentCompileRequest fields have no MCP source (2 true blockers: `principal_id`, `goal`; 4 trivially resolvable) | Cannot construct valid IntentCompileRequest | Engineering + Gateway team |
| **B-MAP-2** | `requested_resource_scope` (ResourceSelector tagged enum) parsing rules undefined | Cannot convert `scope` string to FilesystemPath/GitRepository/etc. | Engineering |
| **B-MAP-3** | `requested_risk_tier` (RiskTier: Low/Medium/High/Critical) and `approval_mode` (ApprovalMode: None/Required/DraftOnly/TwoPhaseCommit) inference rules | Cannot determine risk tier from `action_type` | Engineering + Gateway team |
| **B-MAP-4** | `server_name` resolution rules undefined | Cannot populate ActionProposal.server_name | Engineering |
| **B-MAP-5** | `expected_effect` (string) and `estimated_risk` (RiskTier) inference undefined | Cannot populate ActionProposal effect/risk fields | Engineering |
| **B-MAP-6** | `requested_rollback_class` (RollbackClass: R0NativeReversible/R1SnapshotRecoverable/R2Compensatable/R3IrreversibleHighConsequence) inference | Cannot determine rollback strategy | Engineering + Gateway team |
| **B-MAP-7** | ActionProposalSubmitted event timing uncertain | Provenance chain start is ambiguous | Gateway team |

### 6.2 ⚠️ Open Questions (Should Resolve Before D1.3.2)

| # | Question | Impact | Status |
|---|----------|--------|--------|
| Q-MAP-1 | What is the exact `ResourceSelector` tagged enum schema? | B-MAP-2 parsing | Unknown |
| Q-MAP-2 | What are the valid `requested_risk_tier` (RiskTier) values? | B-MAP-3 risk taxonomy | Unknown |
| Q-MAP-3 | What is the valid `server_name` vocabulary? | B-MAP-4 resolution | Unknown |
| Q-MAP-4 | What is the valid `requested_rollback_class` (RollbackClass) vocabulary? | B-MAP-6 rollback class | Unknown |
| Q-MAP-5 | Do gateway IntentCompileRequest fields have defaults when omitted? | B-MAP-1 may be moot if optional | Unknown |
| Q-MAP-6 | Does eval return `execution_id` or only decision? | Sequential ID flow correction | Unknown (Q3 in doc 75) |
| Q-MAP-7 | Is `submit_intent` auto-eval or compile-only? | Tool semantics | Open (B3 in doc 75) |

---

## 7. Non-Goals (Explicit)

The following are **NOT** in scope for this design:

| Item | Reason |
|------|--------|
| **Implementing ActionProposal mapping** | This is design only — D1.3.2 is blocked |
| **Implementing submit/evaluate tool logic** | D1.3.3+ is blocked by B-MAP-1–B-MAP-7 |
| **Implementing approval/reject tools** | Blocked by B1 in doc 75 — no approval endpoint exists |
| **Implementing provenance emission** | Blocked by B-MAP-7 — timing uncertain |
| **Implementing rollback execution** | D1.5+ blocked by D1.3.2+ |
| **Implementing rate limiting** | D1.9 blocked by Stage 2 complete |
| **Implementing output sanitization** | D1.8 blocked by Stage 2 complete |
| **Defining production G2 criteria** | Operator-owned — out of MCP scope |

---

## 8. Tests Required (When D1.3.2 Unblocked)

### 8.1 Unit Tests

| Test | Description | Blocker it Validates |
|------|-------------|---------------------|
| `test_scope_parsing_files_write` | Parse `"files:write:/tmp/test.txt"` → ResourceSelector | B-MAP-2 |
| `test_scope_parsing_git_push` | Parse `"git:push:origin"` → ResourceSelector | B-MAP-2 |
| `test_risk_tier_inference_low` | `fs_read` → `"read-only"` | B-MAP-3 |
| `test_risk_tier_inference_high` | `sql_mutate` → `"high-risk-write"` | B-MAP-3 |
| `test_server_name_resolution_fs` | `fs_write` → `"filesystem"` | B-MAP-4 |
| `test_server_name_resolution_git` | `git_push` → `"git"` | B-MAP-4 |
| `test_expected_effect_inference` | `fs_write` → effect description | B-MAP-5 |
| `test_rollback_class_inference_file` | `fs_write` → `"file_delete"` | B-MAP-6 |
| `test_rollback_class_inference_git` | `git_push` → `"git_revert"` | B-MAP-6 |
| `test_tool_call_action_to_intent_compile_request_full` | All 6 MCP fields → 6 matching gateway fields | B-MAP-1 partial |
| `test_intent_compile_request_missing_fields_noted` | Document all 7 missing fields with TODO markers | B-MAP-1 |

### 8.2 Integration Tests (Gated Until Blockers Resolved)

| Test | Description | Blocker |
|------|-------------|---------|
| `test_intent_compile_request_serialization_with_missing_fields` | Serialize IntentCompileRequest and note 7 fields needing derivation | B-MAP-1 |
| `test_action_proposal_mapping_roundtrip` | Map ToolCallAction → IntentCompileRequest → ActionProposal and verify field coverage | All B-MAP-* |
| `test_execution_id_from_authorize_not_eval` | Verify execution_id comes from authorize endpoint | B-MAP-6 (Q3) |
| `test_provenance_event_timing` | Verify ActionProposalSubmitted emits at correct step | B-MAP-7 |

---

## 9. Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| D1.3.2 blocked until B-MAP-1–B-MAP-7 resolved | 2026-05-07 | Cannot construct valid IntentCompileRequest or ActionProposal without derivation rules |
| Doc 75 sequential ID flow needs correction | 2026-05-07 | execution_id created at authorize, not eval/prepare |
| B-MAP-7 (provenance timing) requires gateway team input | 2026-05-07 | Cannot confirm ActionProposalSubmitted emission timing from explorer alone |
| Scope parsing rules are proposed, not confirmed | 2026-05-07 | Q-MAP-1 must be resolved before B-MAP-2 can be marked done |

---

## 10. Cross-Reference Index

| From | To | Purpose |
|------|-----|---------|
| This doc | [`74-mcp-server-phase-d1-governance-design.md`](74-mcp-server-phase-d1-governance-design.md) | ToolCallAction definition in §3.3 |
| This doc | [`75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md`](75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md) | D1.3.2 is blocked by B-MAP-1–B-MAP-7; sequential ID correction |
| This doc | [`77-mcp-server-d1-3-2a-pure-mapping-helpers-plan.md`](77-mcp-server-d1-3-2a-pure-mapping-helpers-plan.md) | D1.3.2a pure helpers plan (blocked by D1.3.2 unblock) |
| This doc | [`stage2_types.rs`](../../crates/ferrum-integrations-mcp/src/stage2_types.rs) | ToolCallAction and IntentCompileRequest types (D1.3.1) |
| This doc | [`73-mcp-server-phase-d-implementation-plan.md`](73-mcp-server-phase-d-implementation-plan.md) | D-0 complete; D-1.3.2 blocked |
| This doc | [`67-production-readiness-roadmap.md`](67-production-readiness-roadmap.md) | MCP server is post-v1 scope |

---

*Document created: 2026-05-07. Design documentation only. D-1.3.2 implementation is blocked by B-MAP-1 through B-MAP-7. No production-ready claim. MCP server is post-v1 scope (v1.4 MCP Governance Beta).*
