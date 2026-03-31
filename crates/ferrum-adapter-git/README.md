# ferrum-adapter-git

Local git rollback adapter primitives.

Trạng thái:
- prepare captures the current local `HEAD` as `before_ref`
- rollback/compensate reset the repo back to `before_ref`
- verify checks the current `HEAD` against `after_ref` or `before_ref`
- full git mutation execution and gateway wiring remain out of scope for this slice

## GitBranchCreate Support

This slice also supports local branch creation with automatic switch and rollback cleanup:

- **prepare**: Captures `before_ref`, `original_branch`, and validates:
  - Repo is not dirty (fail-closed if uncommitted changes exist)
  - Target branch does not already exist (fail-closed if branch exists)
- **execute**: Creates new branch and switches to it via `git branch <name> && git checkout <name>`
- **verify**: Confirms current branch matches expected branch and HEAD matches expected ref
- **rollback/compensate**: Switches back to original branch and deletes the created branch

All operations are local-only; no remote operations (push/fetch/pull).
