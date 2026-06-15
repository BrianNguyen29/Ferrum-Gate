# ferrum-adapter-fs

Filesystem adapter for FerrumGate.

## Responsibilities

- File write, delete, move, copy operations with snapshot-based rollback
- Directory create and delete with validation
- File append and chmod with capture/restore

## Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| FileWrite | Snapshot existing file | Write content | Hash matches | Restore snapshot or delete |
| FileDelete | Capture file content | Delete file | File gone | Restore file |
| FileMove | Snapshot dest if exists | Rename | Hash at dest matches | Restore dest, rename back |
| FileCopy | Snapshot dest if exists | Copy | Hash matches | Remove copy or restore dest |
| DirCreate | Validate parent exists | mkdir | Directory exists | Remove directory |
| DirDelete | Validate empty | rmdir empty | Directory gone | Recreate directory |
| FileAppend | Capture original hash + length | Append data | File grew correctly | Truncate to original length |
| FileChmod | Capture current permissions | Change mode bits | Permissions match | Restore original permissions |

## Rollback and risk

- Rollback is automatic via snapshot/restore for existing files, or cleanup for new files.
- Risk class: R1 for most operations; R2 for FileDelete, FileMove, DirDelete.
- Cross-filesystem moves may not be atomic.

## Configuration / allowlist gotchas

- Filesystem adapter requires absolute `fs_workdir` in configuration.
- The adapter is disabled until its allowlist is configured.

## Reference

Full details, examples, and risk class mapping: [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md)
