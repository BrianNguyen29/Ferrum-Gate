# Contributing

Thank you for your interest in contributing to FerrumGate. This project welcomes improvements that respect its intent-scoped, reversible-execution invariants.

## Before You Start

1. Read `docs/guides/README.md` for the guide index.
2. Read `docs/guides/concepts.md` for core architecture.
3. Review `docs/guides/security-model.md` to understand the security model.

## Development Flow

- Pick **one crate or one document boundary** at a time.
- Do not change contracts and schemas without updating docs and tests.
- Preserve intent / capability / provenance / rollback invariants.
- Match existing code style; run `cargo fmt --all` before submitting.

## Commit Style

Use conventional commits where possible:
- `feat:` — new feature
- `fix:` — bug fix
- `refactor:` — code change that neither fixes a bug nor adds a feature
- `docs:` — documentation change
- `test:` — test addition or fix
- `chore:` — maintenance, tooling, or dependency update

## Pull Request Checklist

- [ ] Workspace builds (`cargo check --workspace`)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] Docs updated if behavior changed
- [ ] Contracts updated if enforcement changed
- [ ] JSON Schemas updated if payload shape changed
- [ ] OpenAPI updated if API shape changed
- [ ] New tests added or existing tests updated

## Reporting Issues

Open a GitHub issue for bugs, feature requests, or documentation gaps. For security vulnerabilities, see [`SECURITY.md`](./SECURITY.md) and use private disclosure.

## Expected Behavior

This project does not currently include a separate Code of Conduct file. Contributors are expected to:
- Be respectful and constructive in all interactions.
- Accept feedback gracefully and give it thoughtfully.
- Focus changes on the stated scope; avoid unrelated refactoring.
- Avoid overclaiming capabilities in documentation.
