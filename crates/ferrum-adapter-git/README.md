# ferrum-adapter-git

Local git repository rollback adapter primitive.

## Status

Initial implementation slice complete. Provides local git ref capture and reset primitives.

## Supported Operations

| Operation  | Behavior                                                                 |
|------------|--------------------------------------------------------------------------|
| `prepare`  | Captures current HEAD SHA as `before_ref` in adapter metadata           |
| `rollback` | Hard resets repository to `before_ref`                                    |
| `verify`   | Returns true if current HEAD matches `after_ref` (or `before_ref`)       |
| `compensate` | Alias for `rollback` in this slice                                     |
| `execute`  | Captures `after_ref` from payload when provided; errors on other inputs |

## Metadata Keys

- `repo_path`: Absolute path to the git repository work tree
- `before_ref`: SHA of HEAD at prepare time
- `after_ref`: SHA captured at execute time (when payload provides it)
- `current_ref`: SHA of current HEAD at verify/rollback time

## Limitations (This Slice)

- No gateway/runtime wiring
- No ferrumd registration
- No HTTP adapter integration
- No branch creation or commit mutation
- Uses local `git` CLI only
