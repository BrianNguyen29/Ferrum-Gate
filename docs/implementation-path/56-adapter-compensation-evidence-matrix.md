# 56 — Adapter Compensation Evidence Matrix

> **Status**: Completed 2026-04-29 — documentation evidence matrix
> **Scope**: Current adapter implementations only: fs, git, http, sqlite, maildraft
> **Constraint**: This matrix does **not** claim uniform or full production-ready compensation. It classifies observed behavior and test evidence so operators can identify where compensation is real undo, replay-based, fail-closed, unsupported, or still workload-dependent.

---

## Purpose

This document closes the documentation audit item in `33-feature-completion-backlog.md` §P6 by mapping current compensation behavior per adapter/action.

The key result is intentionally conservative:

- Several adapters provide real undo for bounded local slices.
- HTTP compensation is replay-based, not true undo.
- Git remote push compensation is permission/remote-state dependent and fail-closed when rollback cannot be proven.
- Compensation semantics remain non-uniform across adapters, so this matrix does **not** upgrade FerrumGate to production-ready.

---

## Classification Legend

| Classification | Meaning |
|---|---|
| `real_undo` | Adapter has a bounded rollback/compensate path intended to restore the pre-execute state for the listed action. |
| `alias_to_rollback` | `compensate()` delegates to `rollback()`; classification depends on rollback behavior. |
| `replay_compensation` | Adapter replays a configured compensating request/action. This is not true state restore unless the external system honors idempotency/compensation semantics. |
| `fail_closed` | Adapter refuses or returns `recovered=false` when safe compensation cannot be proven. |
| `unsupported` | Action/shape is rejected as unsupported for rollback/compensate. |
| `noop_unknown` | No reliable undo evidence exists. This should not be treated as compensation. |

---

## Summary by Adapter

| Adapter | Overall classification | Evidence level | Key caveat |
|---|---|---|---|
| fs | `alias_to_rollback` + `real_undo` for bounded local actions | High for tested local actions | Snapshots are local temp artifacts; broader `O_NOFOLLOW`, mount-boundary, and permission/ownership surfaces remain post-v1. |
| git | `alias_to_rollback`; mostly `real_undo` for local/ref actions; `fail_closed` for remote-sensitive push | Medium-High | Remote rollback depends on permissions and remote state; dirty worktree/current-branch guards intentionally fail closed. |
| http | `replay_compensation` for strict `http.replay_v1`; `fail_closed` for unsupported shapes | Medium | Replay is not true undo; production use requires server-side idempotency/compensation support. |
| sqlite | `alias_to_rollback` + SQL compensation for bounded mutations | Medium | Correctness depends on the generated/persisted compensation plan and schema drift guards. |
| maildraft | `alias_to_rollback` + `real_undo` in in-memory draft store | Medium | Draft-only semantics; not sent-email recall/undo. |

---

## Per-Adapter / Per-Action Matrix

### Filesystem adapter (`ferrum-adapter-fs`)

| Action | `compensate()` behavior | Undo classification | Test evidence | Caveat |
|---|---|---|---|---|
| FileWrite | Aliases rollback | `real_undo` — restore snapshot or delete newly-created file | `test_compensation_audit_file_write_real_undo`, snapshot restore tests | Snapshot is local temp artifact. |
| FileDelete | Aliases rollback | `real_undo` — restore deleted file from snapshot | `test_compensation_audit_file_delete_real_undo`, deleted-file restore tests | Snapshot availability required. |
| FileMove | Aliases rollback | `real_undo` — move destination back to source or restore from snapshot for cross-filesystem path | `test_compensation_audit_file_move_real_undo`, move rollback tests | Destination/source state must still satisfy guards. |
| FileCopy | Aliases rollback | `real_undo` — delete new destination or restore previous destination snapshot | `test_compensation_audit_file_copy_real_undo`, copy rollback tests | Existing destination snapshot availability required. |
| FileAppend | Aliases rollback | `real_undo` — truncate to original length and verify hash where available | `test_compensation_audit_file_append_real_undo`, append rollback tests | Requires original length metadata. |
| FileChmod | Aliases rollback | `real_undo` — restore original permission bits | chmod rollback/compensate tests | Ownership/advanced permission semantics remain broader hardening. |
| DirCreate | Aliases rollback | `real_undo` — remove created directory | dir create rollback/compensate tests | Only bounded empty-created-dir semantics. |
| DirDelete | Aliases rollback | `real_undo` — recreate deleted empty directory | dir delete rollback/compensate tests | Non-empty directory deletion is rejected. |

**Current hardening note:** fs now denies symlinks by default at prepare and revalidates symlink/sandbox constraints before execute and rollback, including FileMove/FileCopy destination paths. Broader symlink-following support, `O_NOFOLLOW`, mount-boundary detection, and detailed ownership handling remain post-v1.

---

### Git adapter (`ferrum-adapter-git`)

| Action | `compensate()` behavior | Undo classification | Test evidence | Caveat |
|---|---|---|---|---|
| GitCommit | Aliases rollback | `real_undo` — reset local repository state | rollback restore/head tests | Dirty worktree and divergent state can fail closed. |
| GitBranchCreate | Aliases rollback | `real_undo` — delete created branch when safe | branch-create rollback tests | Fails closed if currently on created branch or unsafe delete. |
| GitTagCreate | Aliases rollback | `real_undo` — delete created tag | tag-create rollback tests | Local tag semantics only. |
| GitTagDelete | Aliases rollback | `real_undo` — recreate deleted tag from captured SHA | tag-delete rollback tests | Requires captured tag SHA. |
| GitBranchDelete | Aliases rollback | `real_undo` — recreate deleted branch at captured tip | branch-delete rollback tests | Requires captured branch tip SHA. |
| GitPull | Aliases rollback | `real_undo` — reset to original HEAD/ref where safe | pull rollback tests | Local ref safety guards apply. |
| GitFetch | Aliases rollback | `real_undo` for tracked local ref restoration; otherwise fail-closed/idempotent as implemented | fetch rollback tests | Fetch side effects on remote state are not reversed. |
| GitPush | Aliases rollback | `fail_closed` / remote-dependent attempted undo | push rollback failure tests | Remote permissions, branch protection, and remote state may prevent undo. |

**Production note:** git compensation is strongest for local/ref operations. Remote operations must be evaluated for the target repository and branch protection policy.

---

### HTTP adapter (`ferrum-adapter-http`)

| Action | `compensate()` behavior | Undo classification | Test evidence | Caveat |
|---|---|---|---|---|
| HttpMutation POST with strict `http.replay_v1` | Replays configured request with idempotency key and digest/url/method checks | `replay_compensation` | replay compensate success/failure tests | Not true undo; external server must implement idempotent compensation semantics. |
| HttpMutation PUT with strict `http.replay_v1` | Same replay path | `replay_compensation` | PUT replay rollback/compensate tests | Requires exact URL/digest binding and expected statuses. |
| HttpMutation PATCH with strict `http.replay_v1` | Same replay path | `replay_compensation` | PATCH replay rollback/compensate tests | Requires exact URL/digest binding and expected statuses. |
| GET/DELETE/unsupported replay shapes | Rejected | `fail_closed` / `unsupported` | GET/DELETE unsupported tests | No generic HTTP undo is claimed. |

**Important:** HTTP compensation should be documented as replay-based compensation, not rollback in the filesystem/database sense. It only works when the external service honors the agreed idempotency and compensation contract.

---

### SQLite adapter (`ferrum-adapter-sqlite`)

| Action | `compensate()` behavior | Undo classification | Test evidence | Caveat |
|---|---|---|---|---|
| SQL INSERT | Aliases rollback | `real_undo` via compensation SQL | insert rollback tests | Depends on compensation plan correctness. |
| SQL UPDATE | Aliases rollback | `real_undo` via compensation SQL | update rollback tests | Depends on captured previous values/plan. |
| SQL DELETE | Aliases rollback | `real_undo` via compensation SQL | delete rollback tests | Depends on captured deleted-row data/plan. |
| DDL/schema mutation slice | Aliases rollback with schema guard | `real_undo` when guard conditions match; otherwise `fail_closed` | DDL rollback/schema guard tests | Schema drift can intentionally block rollback. |

**Production note:** SQLite compensation is bounded to tested SQL mutation shapes and the stored compensation plan; it is not a generic database time-travel mechanism.

---

### Maildraft adapter (`ferrum-adapter-maildraft`)

| Action | `compensate()` behavior | Undo classification | Test evidence | Caveat |
|---|---|---|---|---|
| Draft create | Aliases rollback | `real_undo` — delete created draft | create rollback/compensate tests | Draft store only. |
| Draft update | Aliases rollback | `real_undo` — restore original draft | update rollback/compensate tests | Requires captured original draft. |
| Draft delete | Aliases rollback | `real_undo` — recreate deleted draft | delete rollback/compensate tests | Requires captured original draft. |

**Production note:** Maildraft compensation applies to drafts only. It does not undo sent email delivery.

---

## Gaps and Conservative Conclusions

| Gap | Status | Consequence |
|---|---|---|
| Uniform compensation guarantee across all adapters | Not provided | P6 remains non-uniform; operators must evaluate target adapter/action. |
| HTTP true undo | Not provided | HTTP uses replay compensation; external API contract is required. |
| Git remote rollback guarantee | Not provided | Remote protections/permissions can make rollback fail closed. |
| Durable/encrypted fs snapshots | Not provided | fs snapshot recovery is bounded/local, not a production backup substitute. |
| Generic database time travel | Not provided | SQLite relies on specific compensation plans and guards. |

Therefore, the compensation audit is **complete as evidence**, but production readiness remains **Partial/No** depending on adapter/action and target workload.

---

## Recommended Usage

Before using compensation in a pilot or production-like workflow:

1. Identify the exact adapter/action used by the workload.
2. Check this matrix for classification and caveats.
3. Run the relevant adapter tests or a workload-specific drill.
4. If the path is `replay_compensation` or `fail_closed`, document operator acceptance explicitly.
5. Do not treat `compensate` success as uniform proof of external undo across all adapters.

---

## Related Docs

- [33-feature-completion-backlog.md](./33-feature-completion-backlog.md) §P6 — backlog status for non-uniform adapter compensation
- [45-current-feature-audit.md](./45-current-feature-audit.md) — G2 gap tracking
- [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md) — adapter hardening priority
- [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) — operator acceptance for production pilot risk
