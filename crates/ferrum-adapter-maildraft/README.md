# ferrum-adapter-maildraft

Mail-draft adapter for FerrumGate.

## Responsibilities

- Create, update, and delete email drafts
- Capture previous content for rollback restore
- Draft-only operations; no external sending

## Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| Create draft | Validate recipient/format | Create draft | Draft exists | Delete draft |
| Update draft | Capture previous content | Update draft | Content matches | Restore previous content |
| Delete draft | Capture draft content | Delete draft | Draft gone | Recreate draft |

## Rollback and risk

- Create: deletes the draft.
- Update: restores previous content.
- Delete: recreates draft with captured content.
- Default risk class: R1.

## Configuration / allowlist gotchas

- The maildraft adapter is disabled until its allowlist is configured.
- Actual email sending is not supported by design (draft-only).

## Reference

Full details, examples, and risk class mapping: [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md)
