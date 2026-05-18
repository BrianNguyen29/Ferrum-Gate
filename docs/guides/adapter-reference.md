# Adapter Reference

> **Status**: Scaffold. Adapter slices are implemented; not all surfaces are production-verified.
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## fs — Filesystem adapter

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| FileWrite | Snapshot existing file | Write content | Hash matches | Restore snapshot or cleanup |
| FileDelete | Capture file content | Delete file | File gone | Restore file |
| FileMove | Snapshot dest if exists | Rename | Hash at dest matches | Restore dest, rename back |
| FileCopy | Snapshot dest if exists | Copy | Hash matches | Remove copy or restore dest |
| DirCreate | Validate parent exists | mkdir | Directory exists | Remove directory |
| DirDelete | Validate empty | rmdir empty | Directory gone | Recreate directory |
| FileAppend | Capture original hash + length | Append data | File grew correctly | Truncate to original length |
| FileChmod | Capture current permissions | Change mode bits | Permissions match | Restore original permissions |

### Rollback behavior

- **Existing file overwrite**: Restores original content from deterministic snapshot path.
- **New file write**: Deletes the created file.
- **FileMove**: Restores destination snapshot, renames source back.

### Limitations

- Cross-filesystem moves may not be atomic.
- Symlinks are not fully supported.
- Permission/symlink edge cases are post-v1 scope.

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| FileWrite | R1 |
| FileDelete | R2 |
| FileMove | R2 |
| FileCopy | R1 |
| DirCreate | R1 |
| DirDelete | R2 |
| FileAppend | R1 |
| FileChmod | R1 |

---

## git — Git adapter

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| GitCommit | Capture HEAD ref | Commit | Ref matches | Reset hard to captured HEAD |
| GitBranchCreate | Validate base_ref | Create branch | Branch exists | Delete branch |
| GitBranchDelete | Capture branch SHA | Delete branch | Branch gone | Recreate branch at SHA |
| GitTagCreate | Validate tag name | Create lightweight tag | Tag exists | Delete tag |
| GitTagDelete | Capture tag SHA | Delete tag | Tag gone | Recreate tag at SHA |

### Rollback behavior

- **GitCommit**: `git reset --hard` to captured HEAD. Fails closed if dirty worktree.
- **GitBranchCreate**: Deletes the created branch.
- **GitTagDelete**: Recreates tag at captured SHA.

### Limitations

- Remote push/pull/submodule are post-v1 scope.
- GitBranchDelete verify fails closed if branch is currently checked out.

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| GitCommit | R2 |
| GitBranchCreate | R1 |
| GitBranchDelete | R2 |
| GitTagCreate | R1 |
| GitTagDelete | R2 |

---

## http — HTTP adapter

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| HttpMutation | Validate target/method/URL | Send request | Status/code matches | Replay with idempotency key (POST/PUT/PATCH only) |

### Rollback behavior

- Rollback/compensate succeeds only for strict one-step `http.replay_v1` POST/PUT/PATCH with exact URL/digest binding and strict `expected_statuses`.
- Fails closed otherwise.

### Limitations

- Broader replay/idempotency is post-v1 scope.
- DELETE is not replay-backed.

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| HttpMutation | R2 |

---

## sqlite — SQLite adapter

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| SQL mutation | Validate SQL shape | Execute in transaction | Row counts match | ROLLBACK transaction |

### Rollback behavior

- Uses SQL transaction rollback. If transaction already committed, manual restore may be required.

### Limitations

- Not production-verified beyond local tests.
- Complex schema changes may require manual recovery.

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| SQL mutation | R2 |

---

## maildraft — Mail draft adapter

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| Create draft | Validate recipient/format | Create draft | Draft exists | Delete draft |
| Update draft | Capture previous content | Update draft | Content matches | Restore previous content |
| Delete draft | Capture draft content | Delete draft | Draft gone | Recreate draft |

### Rollback behavior

- Create: deletes the draft.
- Update: restores previous content.
- Delete: recreates draft with captured content.

### Limitations

- Actual email sending is not supported by design (draft-only).

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| maildraft.* | R1 |

## Status caveat

> **production-ready = NO**. Adapter slices are verified locally. Target-host verification and broader surface completion are planned. See [`docs/ROADMAP.md`](../../ROADMAP.md) §3.2 and §4 Phase 3.

## Related docs

- [`concepts.md`](./concepts.md) — Rollback class definitions.
- [`mcp-integration.md`](./mcp-integration.md) — How to invoke adapters via MCP.
- [`docs/implementation-path/56-adapter-compensation-evidence-matrix.md`](../../implementation-path/56-adapter-compensation-evidence-matrix.md) — Detailed compensation evidence.
