# Contributing to FerrumGate

Thank you for contributing to FerrumGate. This document covers conventions, testing, and performance expectations.

## Conventions

- **Conventional commits**: use `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:` prefixes.
- **Pick one crate or document boundary at a time**.
- **Do not change contracts/schemas** without updating docs and tests.
- **Preserve invariants**: intent-scoped execution, single-use capability, provenance-first lineage, rollback-by-default.

## Testing

```bash
make test      # cargo test --workspace
make lint      # cargo clippy --workspace --all-targets -- -D warnings
make fmt       # cargo fmt --all
make validate  # expanded local validation
```

## Performance Baselines

When your change touches the store layer, gateway handlers, or SQLite concurrency:

1. Run the advisory performance gate before pushing:
   ```bash
   make perf-gate
   ```
2. If the gate reports regressions, investigate before opening the PR.
3. If the regression is expected (e.g., a correctness fix that adds necessary serialization), document the trade-off in the PR description.

### Updating baselines

After a deliberate optimization PR, you may regenerate baselines:

```bash
make perf-baseline-update
```

**Important:**
- Review the generated `baselines/*.json` files before committing.
- Remove the `SAMPLE / NON-AUTHORITATIVE` label and update `last_validated_commit` to the actual commit SHA.
- Include evidence of the controlled run (runner type, CPU, disk) in the PR description.
- Do not auto-update baselines from CI runs to avoid threshold creep.

## Release Checklist

See [`RELEASE.md`](../RELEASE.md) for the full release checklist. The advisory `make perf-gate` target is included in the preflight checklist.

## Related docs

- [`docs/adr/011-performance-regression-gate.md`](./adr/011-performance-regression-gate.md)
- [`docs/PRODUCTION_NOTES.md`](./PRODUCTION_NOTES.md)
- [`RELEASE.md`](../RELEASE.md)
