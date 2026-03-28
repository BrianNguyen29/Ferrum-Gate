# 15 -- ferrumctl more useful Execution Plan

Commit-by-commit plan for the first `ferrumctl` write-action slice.
Grounded in existing repo reality: the `ApprovalResolveRequest` gateway handler
lives at `crates/ferrum-gateway/src/server.rs:2028-2148`, the proto type at
`crates/ferrum-proto/src/approval.rs:28-32`, and `ferrumctl` already has a
`ServerClient` in `bins/ferrumctl/src/main.rs:533-663`.

ASCII only.

---

## Slice Goals

- Make `ferrumctl` a useful tool for operators by adding the first write-action
  command: `ferrumctl server resolve-approval`.
- Keep the core gateway behavior unchanged; only add CLI plumbing on top.
- Establish a pattern for typed decision flags (`--approve` xor `--deny`) that
  can be reused in future write commands.

---

## Guardrails (This Slice Only)

1. **No gateway behavior changes** -- only CLI changes; gateway logic untouched.
2. **No bulk mutation** -- single approval per invocation.
3. **No interactive TUI** -- single-shot command; explicit arguments only.
4. **No implicit latest selection** -- caller must supply explicit `approval_id`.
5. **Fail-closed** -- missing or conflicting flags produce a clear error before
   any network call is made.
6. **Auth model unchanged** -- bearer token from env or flag passed unchanged to gateway.

---

## Commit 1: Add execution-plan doc and update README

**Files:**
- `docs/implementation-path/15-ferrumctl-more-useful-execution-plan.md` (new)
- `docs/implementation-path/README.md` (update line 20 to add entry 15)

**Scope:**
- Add this doc with guardrails, commit plan, and slice status.
- Update README to include the new entry in order.

**Validation:**
- Doc exists at the expected path; README refers to it.

---

## Commit 2: Add `resolve_approval` to `ServerClient`

**File:**
- `bins/ferrumctl/src/main.rs`

**Scope:**
- Add `resolve_approval` async method to `ServerClient` (near `get_approval`).
- Method signature:
  ```rust
  async fn resolve_approval(
      &self,
      approval_id: &str,
      actor: &ActorRef,
      approve: bool,
      reason: Option<&str>,
  ) -> Result<ApprovalRequest>
  ```
- POST to `/v1/approvals/{approval_id}/resolve` with `ApprovalResolveRequest`
  body.
- Re-use `decode_json` helper for success; `render_error` for failures.

**Validation:**
- `cargo check -p ferrumctl` passes with zero warnings.

---

## Commit 3: Add `ResolveApproval` CLI variant and handler

**File:**
- `bins/ferrumctl/src/main.rs`

**New `ServerCommand` variant:**
```rust
/// Resolve a pending approval by ID.
ResolveApproval {
    /// Approval ID (UUID).
    approval_id: String,

    /// Grant the approval.
    #[arg(long)]
    approve: bool,

    /// Deny the approval.
    #[arg(long)]
    deny: bool,

    /// Actor type resolving this approval.
    #[arg(long, value_enum)]
    actor_type: ActorType,

    /// Actor ID (username, agent name, etc.).
    #[arg(long)]
    actor_id: String,

    /// Optional display name for the actor.
    #[arg(long)]
    actor_display_name: Option<String>,

    /// Reason for the decision. Required when --deny is set.
    #[arg(long)]
    reason: Option<String>,

    /// Server base URL (e.g. http://127.0.0.1:8080).
    #[arg(long, env = "FERRUMCTL_SERVER_URL")]
    server_url: Option<String>,

    /// Bearer token for authentication.
    #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
    bearer_token: Option<String>,

    /// Output the resolved approval as JSON.
    #[arg(long)]
    json: bool,
}
```

**Flag validation (before any network call):**
- Exactly one of `--approve` or `--deny` must be set.
- `--reason` is required when `--deny` is set.
- `approval_id` is required (no implicit "latest").

**Handler `run_resolve_approval`:**
- Build `ActorRef` from typed flags.
- Call `client.resolve_approval(approval_id, &actor, approve, reason)`.
- Print result human-readable or JSON per `--json` flag.

**Validation:**
- `cargo check -p ferrumctl` passes.
- Unit tests for flag validation logic.

---

## Commit 4: Add unit tests for resolve-approval slice

**File:**
- `bins/ferrumctl/src/main.rs`

**Tests:**
- Test that `--approve --deny` together produces a bail error.
- Test that `--deny` without `--reason` produces a bail error.
- Test that neither `--approve` nor `--deny` produces a bail error.
- Test that `ActorRef` is constructed correctly from typed flags.

**Validation:**
- `cargo test -p ferrumctl` passes.

---

## Slice Status: COMPLETE

- [x] Commit 1: Doc added
- [x] Commit 2: `resolve_approval` client method
- [x] Commit 3: CLI variant and handler
- [x] Commit 4: Unit tests

---

## Slice 15b: Bulk Approval Mutation (Single-Page, Confirm-Gated)

**File:**
- `bins/ferrumctl/src/main.rs`

**Scope:**
- Add `ResolveApprovalBulk` CLI variant (`ferrumctl server resolve-approval-bulk`).
- List one page of approvals using existing `list_approvals` API with filters and limit.
- Resolve each approval using the existing `resolve_approval` API.
- Reconcile non-2xx outcomes via follow-up `get_approval` read.
- Classify outcomes: `Resolved`, `MutationRejected`, `MutationConflicted`, `Unreadable`.
- Output per-item results clearly; exit non-zero on hard failures.

**Bulk Mode Guardrails:**
- At least one scope filter required: `--proposal-id` or `--execution-id`.
- Explicit `--limit` required (bound the mutation).
- Explicit confirmation required: `--yes` and `--expect-count` (must match actual count).
- Decision flags explicit and mutually exclusive: `--approve` xor `--deny`.
- `--reason` required when `--deny`.
- Single-page only — no all-pages automation.

**CLI Usage:**
```sh
# List pending approvals for a proposal
ferrumctl server inspect-approvals --proposal-id UUID --limit 10

# Bulk-approve all pending approvals for a proposal (exact count match required)
ferrumctl server resolve-approval-bulk \
  --proposal-id UUID \
  --limit 10 \
  --yes \
  --expect-count 3 \
  --approve

# Bulk-deny with reason
ferrumctl server resolve-approval-bulk \
  --execution-id UUID \
  --limit 5 \
  --yes \
  --expect-count 2 \
  --deny \
  --reason "Not authorized for this execution"
```

**Key Implementation Details:**
- `BulkResolutionOutcome` enum classifies each per-item result.
- `classify_resolve_outcome()` fetches final state on non-2xx to determine if mutation was applied.
- `is_pending_state()` helper filters the listing to only Pending approvals.
- `format_bulk_outcome()` renders human-readable per-item output.
- `extract_http_status()` walks the anyhow error chain for reqwest status codes.
- Exit is non-zero if any `MutationRejected` or `Unreadable` outcomes exist.

**Validation:**
- `cargo check -p ferrumctl` passes.
- `cargo test -p ferrumctl` passes (57 tests).
- Unit tests cover: `is_pending_state`, `format_bulk_outcome`, `BulkResolutionOutcome` JSON serialization, `extract_http_status`.

**Slice Status: COMPLETE**
- [x] `ResolveApprovalBulk` CLI variant and handler
- [x] Per-item outcome classification and output
- [x] Non-2xx reconciliation via follow-up read
- [x] Fail-closed guardrails (scope filter, limit, confirmation)
- [x] Unit tests for helpers and classification
- [x] Plan doc updated

---

## Slice 15e: `ferrumctl server watch-execution` (Read-Only Bounded Polling)

**File:**
- `bins/ferrumctl/src/main.rs`

**Scope:**
- Add a read-only bounded-polling watch command over the existing `get_execution` API.
- No gateway changes; only CLI plumbing reusing existing `ServerClient::get_execution`.

**CLI Design:**
```
ferrumctl server watch-execution <execution_id> \
  [--poll-interval-ms MS] \
  [--iterations N] \
  [--json] \
  [--require-terminal]
```

**Validation rules enforced locally before any network call:**
- `--poll-interval-ms` must be in range 100..=300_000 if provided (default: 2000).
- `--iterations` must be >= 1 if provided (default: 60, ~2 minutes at default interval).
- `execution_id` is required (no implicit selection).

**Terminal state detection:**
- `is_execution_terminal_state()` returns true for: `Completed`, `Committed`, `Approved`, `Denied`, `RolledBack`, `Error`, `Quarantined`, `Cancelled`, `TimedOut`.

**Output modes:**
- Default: deterministic human-readable summary per iteration with `[TERMINAL]` marker when state is terminal.
- `--json`: raw `ExecutionRecord` JSON per iteration.

**Key Implementation Details:**
- `format_execution_record_text()` produces deterministic output with iteration number, execution_id, state, and all record fields.
- Bounded loop via `--iterations` (prevents infinite loops in tests/scripts).
- Stops early when terminal state is reached.
- `--require-terminal`: exit non-zero if iteration cap reached without terminal state.

**Unit Tests Added:**
- `test_is_execution_terminal_state_terminal` — all 9 terminal states return true.
- `test_is_execution_terminal_state_non_terminal` — non-terminal states return false.
- `test_format_execution_record_text_non_terminal` — non-terminal iteration shows no [TERMINAL] marker.
- `test_format_execution_record_text_terminal` — terminal iteration shows [TERMINAL] marker.
- `test_format_execution_record_text_shows_all_fields` — all standard fields are present.

**Validation:**
- `cargo check -p ferrumctl` passes (zero warnings).
- `cargo test -p ferrumctl` passes (77+ tests).

**Slice Status: COMPLETE**
- [x] `WatchExecution` CLI variant with bounded polling
- [x] Handler `run_watch_execution`
- [x] `--json` raw record output + deterministic human-readable summary
- [x] `--require-terminal` for fail-closed iteration-cap enforcement
- [x] Local `poll_interval_ms` validation (100..=300_000, default 2000)
- [x] Unit tests for terminal detection and formatting
- [x] Plan doc updated

---

## Future Backlog (Out of Scope for This Slice)

- Interactive TUI for approval workflow
- `ferrumctl server cancel-execution <execution_id>`
- `ferrumctl server pause-execution <execution_id>`
- Automatic latest-approval selection with confirmation prompt

---

## Key Files

| File | Role |
|------|------|
| `crates/ferrum-gateway/src/server.rs:2028-2148` | `resolve_approval` gateway handler |
| `crates/ferrum-proto/src/approval.rs:28-32` | `ApprovalResolveRequest` proto |
| `crates/ferrum-proto/src/common.rs:105-120` | `ActorRef` and `ActorType` types |
| `bins/ferrumctl/src/main.rs:533-663` | `ServerClient` struct |
| `bins/ferrumctl/src/main.rs:1383-1675` | Bulk approval resolution (`ResolveApprovalBulk`, handlers, helpers) |
| `bins/ferrumctl/src/main.rs:3080-3201` | Bulk resolution unit tests |
| `openapi/ferrumgate-control-api.v1.yaml:415-433` | Resolve approval API spec |

---

## Recommended Next Slice

Interactive TUI for approval workflow or `ferrumctl server cancel-execution`.
Both are independent of the approval plumbing added in slices 15 and 15b.

---

## Slice 15c: `server inspect-lineage-query` (Read-Only Multi-Hop Lineage Traversal)

**File:**
- `bins/ferrumctl/src/main.rs`
- `bins/ferrumctl/Cargo.toml`

**Scope:**
- Add a thin read-only wrapper over `POST /v1/provenance/lineage` from the control API.
- No gateway changes; only CLI plumbing.

**CLI Design (fail-closed):**
```
ferrumctl server inspect-lineage-query \
  --execution-id UUID \
  --event-id UUID \
  --ancestry \
  [--descendants] \
  [--max-hops 1-32] \
  [--json]
```

**Validation rules enforced locally before any network call:**
- `--execution-id` and `--event-id` are required (no implicit selection).
- At least one of `--ancestry` or `--descendants` must be set (fail-closed: no silent default).
- `--max-hops` must be in range 1..32 if provided (server hard-caps at 32).

**Output modes:**
- `--json` — raw `LineageQueryResponse` JSON (events + edges, no transformation).
- Human-readable summary (default) — event count, edge list with provenance kinds, event list sorted deterministically by (occurred_at, event_id).

**Key Implementation Details:**
- `ServerClient::lineage_query(req: &LineageQueryRequest) -> Result<LineageQueryResponse>` POSTs to `/v1/provenance/lineage`.
- `LineageQueryRequest` and `LineageQueryResponse` re-used from `ferrum_proto::provenance`.
- UUID parsing via `uuid::Uuid::parse_str` with user-facing error messages.
- `validate_max_hops()` rejects values outside 1..32.
- `format_lineage_query_text()` produces deterministic output using RFC3339 timestamps and sorted event IDs.

**Validation:**
- `cargo check -p ferrumctl` passes (zero warnings).
- `cargo test -p ferrumctl` passes (64+ tests).

**Unit Tests Added:**
- `test_validate_max_hops_none` — None passes through.
- `test_validate_max_hops_valid_values` — 1, 8, 16, 32 accepted.
- `test_validate_max_hops_too_low` — 0 rejected with message.
- `test_validate_max_hops_too_high` — 33 rejected with message.
- `test_kind_label_all_variants` — all 24 ProvenanceEventKind variants return expected labels.
- `test_format_lineage_query_text_empty` — empty response formats correctly.
- `test_format_lineage_query_text_edge_rendering` — JSON fixture round-trips.
- `test_lineage_query_request_serialization` — full request with all fields.
- `test_lineage_query_request_minimal` — request with only direction set.

**Slice Status: COMPLETE**
- [x] `ServerClient::lineage_query` method
- [x] `InspectLineageQuery` CLI variant with fail-closed validation
- [x] Handler `run_inspect_lineage_query`
- [x] `--json` raw output + deterministic human-readable summary
- [x] Local `max_hops` validation (1..32)
- [x] Unit tests for validation, formatting, and request serialization
- [x] Plan doc updated

---

## Slice 15d: `ferrumctl server watch-approvals` (Read-Only Bounded Polling)

**File:**
- `bins/ferrumctl/src/main.rs`

**Scope:**
- Add a read-only bounded-polling watch command over the existing `list_approvals` API.
- No gateway changes; only CLI plumbing reusing existing `ServerClient::list_approvals`.

**CLI Design:**
```
ferrumctl server watch-approvals \
  [--proposal-id UUID] \
  [--execution-id UUID] \
  [--limit N] \
  [--cursor CURSOR] \
  [--poll-interval-ms MS] \
  [--iterations N] \
  [--json]
```

**Validation rules enforced locally before any network call:**
- `--poll-interval-ms` must be in range 100..=300_000 if provided (default: 5000).
- `--iterations` must be >= 1 if provided (default: 1 for single-shot).

**Output modes:**
- Default: deterministic human-readable summary per iteration with approval count and next_cursor.
- `--json`: raw `ApprovalListEnvelope` JSON per iteration.

**Key Implementation Details:**
- `validate_poll_interval_ms()` rejects values outside 100..=300_000 range.
- `format_watch_iteration_text()` produces deterministic output: Pending first, then created_at desc, then approval_id asc.
- Bounded loop via `--iterations` (prevents infinite loops in tests/scripts).
- Cursor is carried forward from `next_cursor` in each response envelope.
- Loop terminates when: iteration limit reached, or next_cursor is None.

**Unit Tests Added:**
- `test_validate_poll_interval_ms_none` — None returns default 5000.
- `test_validate_poll_interval_ms_valid_values` — 100, 1000, 5000, 60000, 300000 accepted.
- `test_validate_poll_interval_ms_too_low` — 99 rejected with message.
- `test_validate_poll_interval_ms_too_high` — 300001 rejected with message.
- `test_format_watch_iteration_text_empty` — empty envelope formats correctly.
- `test_format_watch_iteration_text_single_approval` — single approval shows all fields.
- `test_format_watch_iteration_text_deterministic_order` — Pending sorts before Approved.
- `test_format_watch_iteration_text_next_cursor_display` — cursor shown correctly.

**Validation:**
- `cargo check -p ferrumctl` passes (zero warnings).
- `cargo test -p ferrumctl` passes.

**Slice Status: COMPLETE**
- [x] `WatchApprovals` CLI variant with bounded polling
- [x] Handler `run_watch_approvals`
- [x] `--json` raw envelope output + deterministic human-readable summary
- [x] Local `poll_interval_ms` validation (100..=300_000)
- [x] Unit tests for validation and formatting
- [x] Plan doc updated

---

## Slice 4: compensate-execution and rollback-execution wrappers

**Files:**
- `bins/ferrumctl/src/main.rs`
- `docs/15-deployment-and-operations.md`

**Scope:**
- Add `CompensateExecution` and `RollbackExecution` CLI variants under `ferrumctl server`.
- Add `compensate_execution` and `rollback_execution` client methods to `ServerClient`.
- Both POST to existing gateway endpoints (`/v1/executions/{execution_id}/compensate` and `/v1/executions/{execution_id}/rollback`).
- Require explicit `execution_id`; support `--json` for JSON output.
- No new server-side behavior; gateway enforces terminal-state guards and rollback-contract precondition.

**Key gateway semantics (not changed, only documented):**
- Both endpoints reject if execution is already in a terminal state (Compensated, RolledBack, Denied, Failed, Quarantined).
- Committed state is accepted (can undo after commit).
- Both require the execution to have a rollback contract (`rollback_contract_id` must be present).

**Docs updated:**
- `docs/15-deployment-and-operations.md` now includes an "Execution control" section describing both commands and their semantics.

**Validation:**
- `cargo check -p ferrumctl` passes.
- `cargo test -p ferrumctl` passes (77 unit + 7 integration).
- `cargo run -p ferrumctl -- server compensate-execution --help` shows help.
- `cargo run -p ferrumctl -- server rollback-execution --help` shows help.

**Slice Status: COMPLETE**
- [x] `CompensateExecution` and `RollbackExecution` CLI variants
- [x] Client methods `compensate_execution` and `rollback_execution`
- [x] `--json` support
- [x] Documentation updated with correct semantics
