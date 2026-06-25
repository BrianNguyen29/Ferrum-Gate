# AGENTS.md — FerrumGate Repository

## Workspace & Toolchain
- Rust edition 2024, resolver "2", 24 workspace members (see `Cargo.toml`)
- Toolchain: stable with rustfmt, clippy (see `rust-toolchain.toml`)
- `rustfmt.toml`: max_width=100, Unix newline, reorder_imports=true
- `clippy.toml`: msrv=1.85.0, too-many-arguments-threshold=8, type-complexity-threshold=350
- `Cargo.toml`: `sqlx` with `default-features = false`; `schemars` with non-standard features (`chrono`, `uuid1`, `indexmap`)

## Binaries & Entrypoints
- `ferrumd` — gateway daemon
- `ferrumctl` — CLI for health, readiness, audit, policy, approvals, lifecycle, backup/restore
- `ferrum-migrate` — SQLite-to-PostgreSQL migration support
- `ferrum-stress` — machine-readable stress/smoke scenarios
- `ferrum-tui` — terminal operator dashboard
- `ferrum-mcp-server` — MCP stdio server (default, stable); HTTP transport is experimental and requires `--features http`

## Daily Commands (see `Makefile` for full list)
```
make check     # cargo check --workspace
make fmt       # cargo fmt --all
make lint      # cargo clippy --workspace --all-targets -- -D warnings
make test      # cargo test --workspace
make audit     # local security audit gate (cargo-deny / cargo-audit)
make validate  # layout, contracts, MCP tools, toml, openapi, docs links, evidence templates, Python validators
make s3-test  # run S3 adapter MinIO integration tests (requires local MinIO at localhost:9000)
make pretarget # pre-target gate (config examples, restore drill, evidence skeleton, docs, bearer-auth smoke)
```
- Check formatting without mutation: `cargo fmt --all -- --check`
- Feature-gated package check: `cargo check --package ferrum-migrate --features postgres`
- Layout/contract validation: `bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py`
- Architecture decisions: see `docs/adr/README.md`

## CI / Local Gates
- CI (`ci.yml`): `release-governance` (secrets + audit), `validate` (fmt/check/clippy/test + all-features + postgres-feature + promtool + Helm/docs/schema checks + release profile smoke + ferrum-stress smoke), `postgres-live-tests` (PostgreSQL 16 service container; store-level tests + full `ferrumd` boot with Postgres backend and health/readiness probes; **no skips allowed**), `coverage-and-sbom` (workspace coverage via `cargo-llvm-cov` as LCOV/text artifacts + CycloneDX JSON SBOM artifact uploaded; no external secrets, **no threshold enforcement yet**), `s3-live-tests` (MinIO container; S3 adapter integration tests with `--features s3-client`)
- Manual gates (`.github/workflows/manual-gates.yml`): `workflow_dispatch` and **nightly schedule** — audit, pretarget, wal-drill, pg-batch, ha-drills, mcp-smoke. Drills use `continue-on-error: true` initially for visibility without blocking. **Docker required for pg-batch and ha-drills.**
- Release profile blocks `unsafe-unbounded-adapters`; see `scripts/validate_release_feature_profile.sh`
- Local profile evidence / manual gates **do not** constitute production-ready signoff (G2/pilot/RC-ready remain operator actions)

## ferrumd Config & Runtime Gotchas
- Precedence: CLI args > env vars > config file > defaults.
- Dev config `configs/ferrumgate.dev.toml` auto-loads if no `--config` specified and it exists (auth=disabled, in-memory SQLite).
- Prod requires bearer auth; generate token with `openssl rand -hex 32`.
- Env vars: see `configs/examples/ferrumd.env.example` (OIDC vars use `FERRUMD_OIDC_*`).
- Runtime:
  - `/v1/healthz` and `/v1/readyz` are shallow 200s; `/v1/readyz/deep` checks store/queue.
  - Filesystem adapter needs absolute `fs_workdir`.
  - Git and SQLite adapters are disabled until their allowlists are configured.
  - S3 adapter is gated by `live: true` in `s3_config`; when enabled, S3 SDK calls fail closed (not silently).
  - `allow_insecure_nonlocal_bind` is guarded; only enable for local dev.
  - Structured JSON logs via `FERRUMD_LOG_FORMAT=json` or config.
  - Rate limit defaults: 2 req/s sustained, burst 50.
  - SQLite: write queue enabled; PRAGMA `synchronous=NORMAL`, `wal_autocheckpoint=1000`, `cache_size=-64000`, `busy_timeout=5000ms`.
  - PostgreSQL: feature-gated (`--features postgres`); available for sustained high write throughput or cross-process deployments. Production HA/multi-node topology is **not** managed by this repo.
- Security audit: `cargo audit --ignore RUSTSEC-2023-0071` (sqlx-mysql optional RSA dependency; FerrumGate builds sqlx with `default-features = false` and does not enable mysql).
- Docker/demo configs are **not** production-ready.

## Critical Invariants (Do Not Break)
- Intent-scoped execution, single-use capability, provenance-first lineage, rollback-by-default.
- Do not bypass: gateway, policy, capability validation, rollback prepare, provenance emission.
- R3 never auto-commit; output must sanitize; scope must not exceed intent.
- Capabilities: ttl_max=300s, single-use only.

## Minimum Lineage Chain Before Side Effect
PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)

## Contracts / Schemas
- `contracts/ferrumgate-agent-contract.v1.yaml`
- `contracts/ferrumgate-integrator-contract.v1.yaml`
- `contracts/policy-bundle.example.yaml`
- `schemas/jsonschema/`
- `openapi/ferrumgate-control-api.v1.yaml`

## Contributing Rules
- Pick one crate or document boundary at a time.
- Do not change contracts/schemas without updating docs and tests.
- Preserve intent/capability/provenance/rollback invariants.
- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`

## Onboarding
- Actual docs: `docs/guides/`, `docs/PRODUCTION_NOTES.md`.
- Agent system prompt: `prompts/agent_system.md`.
