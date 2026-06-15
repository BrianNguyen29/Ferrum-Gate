# ferrum-adapter-sqlite

SQLite adapter for FerrumGate.

## Responsibilities

- SQL mutation execution within transactions
- Transaction rollback for compensation
- Row-count verification

## Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| SQL mutation | Validate SQL shape | Execute in transaction | Row counts match | ROLLBACK transaction |

## Rollback and risk

- Uses SQL transaction rollback. If the transaction already committed, manual restore may be required.
- Default risk class: R2.

## Configuration / allowlist gotchas

- The SQLite adapter is disabled until its allowlist is configured.
- Concurrent write access to the same database file may cause `SQLITE_BUSY` errors.

## Reference

Full details, examples, and risk class mapping: [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md)
