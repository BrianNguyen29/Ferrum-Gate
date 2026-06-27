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
- [ ] `cargo check -p ferrumd --features s3` passes
- [ ] `make docs` passes (link validation)
- [ ] `make validate` passes (layout, contracts, MCP tools, evidence templates)
- [ ] `make audit` passes (cargo-deny / cargo-audit)
- [ ] `make pretarget` passes (config examples, restore drill, evidence skeleton, bearer-auth smoke)
- [ ] `make perf-gate` passes (advisory performance regression check; see ADR 011)
- [ ] `make coverage-threshold-hard` passes (optional hard coverage check for critical crates; see below)
- [ ] `CHANGELOG.md` is updated with the new version section (must include release date and non-trivial content)
- [ ] Version strings in `Cargo.toml` workspace packages are bumped if needed
- [ ] `docs/ROADMAP.md` status table is updated if any items changed status
- [ ] SBOM is generated (`cargo cyclonedx --all`) and attached to the release

## Release automation (preflight)

A conservative, manual-preflight script is provided to run the checklist safely without pushing or publishing anything. A CI workflow (`.github/workflows/release.yml`) is also available for CI-level preflight with mandatory SBOM generation and optional draft-release creation.

### Local preflight

```bash
# Dry-run mode (default): runs all checks, prints next steps, no side effects
make release-preflight

# With SBOM generation (still no push/publish)
make release-preflight-execute

# Optional: hard coverage threshold check for critical crates (local only; requires cargo-llvm-cov)
make coverage-threshold-hard
```

The script validates:
- `CHANGELOG.md` contains a section for the target version
- `Cargo.toml` workspace version matches the target version
- All cargo checks, formatting, clippy, and tests pass
- `make docs`, `make validate`, `make audit`, `make pretarget`, and `make perf-gate` pass
- Release profile smoke test passes

### Manual GitHub workflow

A `workflow_dispatch` workflow is available at `.github/workflows/release.yml` for CI-level preflight. It does **not** publish crates. It **mandatorily** generates and uploads a CycloneDX SBOM artifact. Optionally, it can create a **draft** GitHub release with the SBOM attached.

Steps:
1. Go to **Actions → release → Run workflow**.
2. Enter the target version (must match `Cargo.toml` workspace version).
3. Enable **Create a draft GitHub release** if you want a draft release created automatically.
4. The workflow will run all preflight checks and generate the SBOM artifact.
5. If draft release is enabled, the workflow creates a draft release and attaches the SBOM.
6. Download the SBOM artifact after the run completes (if not using draft release).

## Creating a release

1. Update `CHANGELOG.md` with the release date and summary.
2. Run the validation commands above (or `make release-preflight`).
3. Commit with a conventional commit: `chore(release): prepare v0.1.1`.
4. Tag the release locally:
   ```bash
   git tag -a v0.1.1 -m "Release v0.1.1"
   git push origin v0.1.1
   ```
5. Create a GitHub Release from the tag, copying the relevant `CHANGELOG.md` section into the release notes.
6. **Attach the SBOM artifact** to the GitHub Release (mandatory):
   - If generated via CI, download the artifact from the workflow run and upload it to the release.
   - Or generate locally: `cargo install cargo-cyclonedx && cargo cyclonedx --all`.
   - The SBOM files are located in `target/cyclonedx/`.
   - Releases without an SBOM artifact are not considered complete.

## Crate publishing

FerrumGate is a workspace with multiple crates. Crate publishing to crates.io is **not yet automated** and should be done manually when there is a clear consumer need:

```bash
# Example: publish a single crate
cargo publish -p ferrum-proto --dry-run
cargo publish -p ferrum-proto
```

> **Note:** Do not publish internal-only crates (`ferrum-testkit`, `ferrum-integration-tests`) to crates.io. They are workspace-internal only.

## Release automation next steps (proposed, not implemented)

- **Automated crate publishing** — A CI job that publishes crates in dependency order after a release tag is pushed. Requires crates.io API token management, dry-run validation, and rollback handling. See `docs/ROADMAP.md` for timeline.
- **Automated GitHub Release creation** — A workflow that creates the GitHub Release from the tag, copies the `CHANGELOG.md` section, and attaches the SBOM artifact. **Partially implemented**: the `release.yml` workflow can optionally create a draft release with SBOM attached; full automation (auto-generating release notes from CHANGELOG) is still manual.
- **Release branch automation** — Automated creation of a release branch, version bump PR, and tag push. Not yet implemented; manual process is documented above.

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
