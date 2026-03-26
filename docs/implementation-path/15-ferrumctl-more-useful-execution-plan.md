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

## Future Backlog (Out of Scope for This Slice)

- Bulk approval mutation (`--all-pending`)
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
| `openapi/ferrumgate-control-api.v1.yaml:415-433` | Resolve approval API spec |

---

## Recommended Next Slice

Bulk approval mutation (`ferrumctl server resolve-approval --all-pending`) grounded in
the same execution plan. The single-approval CLI plumbing from this slice provides the
foundation for bulk operations.