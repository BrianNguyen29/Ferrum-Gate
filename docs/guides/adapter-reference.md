# Adapter Reference

> **Status**: Expanded. Adapter slices are implemented; not all surfaces are production-verified.
> **Parent**: [`guides/README.md`](./README.md)

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

### Example: FileWrite with rollback

```json
{
  "action": "fs.FileWrite",
  "target": "/tmp/config.yaml",
  "parameters": {
    "content": "key: value\n"
  }
}
```

If the file already exists, prepare captures a snapshot. If verification fails or compensation is triggered, the snapshot is restored.

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

### Example: GitCommit with rollback

```json
{
  "action": "git.GitCommit",
  "target": "/repo/path",
  "parameters": {
    "message": "Update configuration"
  }
}
```

If verification fails, the adapter resets hard to the captured HEAD. This fails closed if the worktree is dirty.

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

### Example: HttpMutation

```json
{
  "action": "http.HttpMutation",
  "target": "https://api.example.com/v1/items",
  "parameters": {
    "method": "POST",
    "headers": {"Content-Type": "application/json"},
    "body": "{\"name\":\"test\"}"
  }
}
```

### Rollback behavior

- Rollback/compensate succeeds only for strict one-step `http.replay_v1` POST/PUT/PATCH with exact URL/digest binding and strict `expected_statuses`.
- Fails closed otherwise.

### Limitations

- Broader replay/idempotency is post-v1 scope.
- DELETE is not replay-backed.
- External API availability is outside FerrumGate control; verify step may fail due to transient errors.

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

### Example: SQL mutation

```json
{
  "action": "sqlite.SQL",
  "target": "/data/app.db",
  "parameters": {
    "sql": "INSERT INTO events (type, payload) VALUES (?, ?)",
    "params": ["login", "{\"user\":\"alice\"}"]
  }
}
```

### Rollback behavior

- Uses SQL transaction rollback. If transaction already committed, manual restore may be required.

### Limitations

- Not production-verified beyond local tests.
- Complex schema changes may require manual recovery.
- Concurrent write access to the same database file may cause `SQLITE_BUSY` errors.

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

### Example: Create draft

```json
{
  "action": "maildraft.CreateDraft",
  "target": "primary",
  "parameters": {
    "to": "team@example.com",
    "subject": "Status update",
    "body": "All systems nominal."
  }
}
```

### Rollback behavior

- Create: deletes the draft.
- Update: restores previous content.
- Delete: recreates draft with captured content.

### Limitations

- Actual email sending is not supported by design (draft-only).
- Integration with external mail providers (Gmail API, Outlook, etc.) is not implemented.

### Risk class mapping

| Operation | Default class |
|-----------|---------------|
| maildraft.* | R1 |

---

## Rollback and risk summary

| Adapter | Automatic rollback | Manual recovery possible | External dependency |
|---------|-------------------|-------------------------|---------------------|
| fs | Yes (snapshot/restore) | Yes | None |
| git | Yes (reset/delete/recreate) | Yes | None |
| http | Limited (replay only for POST/PUT/PATCH) | No | Target API must support replay |
| sqlite | Yes (transaction rollback) | Yes | None |
| maildraft | Yes (delete/restore/recreate) | Yes | None |

### When rollback fails

Rollback is not guaranteed in all cases:

- **Competing writers**: Another process modifying the same file/database while FerrumGate operates may leave the system in an inconsistent state.
- **External APIs**: HTTP replay depends on the target API maintaining consistent state.
- **Resource exhaustion**: Disk full or memory pressure may prevent snapshot creation or restoration.
- **Permission changes**: If file permissions change between prepare and compensate, restore may fail.

In all cases, the gateway **fails closed**: if rollback cannot be verified, the execution is marked `SideEffectCompensated` or `SideEffectRolledBack` with an error annotation in provenance.

---

## Status caveat

> **production-ready = NO**. Adapter slices are verified locally. Target-host verification and broader surface completion are planned.

## Related docs

- [`concepts.md`](./concepts.md) — Rollback class definitions.
- [`mcp-integration.md`](./mcp-integration.md) — How to invoke adapters via MCP.
