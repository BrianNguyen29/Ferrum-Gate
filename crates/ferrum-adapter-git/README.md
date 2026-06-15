# ferrum-adapter-git

Local git repository rollback adapter.

## Supported Operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| `GitCommit` | Capture HEAD ref | Commit | Ref matches | `git reset --hard` to captured HEAD |
| `GitBranchCreate` | Validate base_ref | Create branch | Branch exists | Delete branch |
| `GitBranchDelete` | Capture branch SHA | Delete branch | Branch gone | Recreate branch at SHA |
| `GitTagCreate` | Validate tag name | Create lightweight tag | Tag exists | Delete tag |
| `GitTagDelete` | Capture tag SHA | Delete tag | Tag gone | Recreate tag at SHA |

See [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md) for full examples, risk classes, and limitations.

## Metadata Keys

- `repo_path`: Absolute path to the git repository work tree
- `before_ref`: SHA of HEAD at prepare time
- `after_ref`: SHA captured at execute time
- `branch_name`: Target branch for `GitBranchCreate` / `GitBranchDelete`
- `tag_name`: Target tag for `GitTagCreate` / `GitTagDelete`
- `current_ref`: SHA of current HEAD at verify/rollback time

## Limitations

- No gateway/runtime wiring in this crate — adapter is invoked via `ferrum-gateway`.
- No HTTP adapter integration.
- Remote push/pull/submodule are out of scope.
- Uses local `git` CLI only.
- `GitBranchDelete` verify fails closed if branch is currently checked out.
- `GitCommit` rollback fails closed if the worktree is dirty.

## Rollback and risk

- `GitCommit` rollback uses `git reset --hard` to the captured HEAD; fails closed if the worktree is dirty.
- `GitBranchDelete` verify fails closed if the branch is currently checked out.
- Default risk class: R2 for commit/branch-delete/tag-delete; R1 for branch-create/tag-create.

## Configuration / allowlist gotchas

- The git adapter is disabled until its allowlist is configured.
