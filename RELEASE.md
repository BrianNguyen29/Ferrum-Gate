# Release Policy

FerrumGate uses [Semantic Versioning 2.0.0](https://semver.org/).

## Version scheme

- `0.1.0` — current development version
- Versions below `0.1.0` are unsupported

## Release checklist

Before tagging a release, verify:

- [ ] `cargo check --workspace` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo check --workspace --all-features` passes
- [ ] `cargo check -p ferrumd -p ferrum-migrate -p ferrum-store -p ferrum-gateway --features postgres` passes
- [ ] `make docs` passes (link validation)
- [ ] `make validate` passes (layout, contracts, MCP tools, evidence templates)
- [ ] `make audit` passes (cargo-deny / cargo-audit)
- [ ] `make pretarget` passes (config examples, restore drill, evidence skeleton, bearer-auth smoke)
- [ ] `CHANGELOG.md` is updated with the new version section
- [ ] Version strings in `Cargo.toml` workspace packages are bumped if needed
- [ ] `docs/ROADMAP.md` status table is updated if any items changed status

## Creating a release

1. Update `CHANGELOG.md` with the release date and summary.
2. Run the validation commands above.
3. Commit with a conventional commit: `chore(release): prepare v0.1.1`.
4. Tag the release:
   ```bash
   git tag -a v0.1.1 -m "Release v0.1.1"
   git push origin v0.1.1
   ```
5. Create a GitHub Release from the tag, copying the relevant `CHANGELOG.md` section into the release notes.

## Crate publishing

FerrumGate is a workspace with multiple crates. Crate publishing to crates.io is **not yet automated** and should be done manually when there is a clear consumer need:

```bash
# Example: publish a single crate
cargo publish -p ferrum-proto --dry-run
cargo publish -p ferrum-proto
```

> **Note:** Do not publish internal-only crates (`ferrum-testkit`, `ferrum-integration-tests`) to crates.io. They are workspace-internal only.

## Supported versions

| Version | Status |
|---------|--------|
| v0.1.0 | Current development; single-node SQLite focus |
| < v0.1.0 | Unsupported |

## Security releases

If a security fix is required:
1. Prepare the fix on a private branch or fork.
2. Run the full release checklist plus any security-specific validation.
3. Coordinate disclosure with the reporter (see [`SECURITY.md`](./SECURITY.md)).
4. Publish the release and advisory simultaneously.
