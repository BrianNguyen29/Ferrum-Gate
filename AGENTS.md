# AGENTS.md — FerrumGate Repository

## Workspace & Toolchain
- Rust edition 2024, resolver "2", 23 workspace members (see Cargo.toml)
- Toolchain: stable with rustfmt, clippy (see rust-toolchain.toml)
- rustfmt.toml: max_width=100, Unix newline, reorder_imports=true
- clippy.toml: msrv=1.85.0, too-many-arguments-threshold=8, type-complexity-threshold=350

## Binaries & Entrypoints
- `ferrumd` — gateway daemon
- `ferrumctl` — CLI for health, readiness, audit, policy, approvals, lifecycle, backup/restore
- `ferrum-migrate` — SQLite-to-PostgreSQL migration support
- `ferrum-stress` — machine-readable stress/smoke scenarios
- `ferrum-tui` — terminal operator dashboard

## Makefile Commands
```
make check     # cargo check --workspace
make fmt       # cargo fmt --all
make lint      # cargo clippy --workspace --all-targets -- -D warnings
make test      # cargo test --workspace
make audit     # local security audit gate (cargo-deny / cargo-audit)
make validate  # expanded local validation (layout, contracts, MCP tools, toml, openapi, docs links, evidence templates, Python validators)
make docs      # validate docs links and site scaffold
make secret-scan     # local hardcoded secrets scan
make restore-drill   # local SQLite backup/restore drill
make stress          # stress tests against a running service (requires BASE_URL)
make check-pilot-readiness  # pilot readiness probes (requires running server)
make domainless-tier1-fast  # lightweight gate (docs + validate, no heavy Docker drills)
make domainless-tier1-gate  # full gate (fast + PostgreSQL + HA drills)
make pretarget   # pre-target gate (config examples, restore drill, evidence skeleton, docs, bearer-auth smoke)
```
- Check formatting without mutation: `cargo fmt --all -- --check`
- Feature-gated package check: `cargo check --package ferrum-migrate --features postgres`
- Layout/contract validation: `bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py`

## CI / Local Gates
CI jobs (`ci.yml`): `release-governance` (secrets scan + security audit), `validate` (fmt/check/clippy/test + all-features + postgres-feature + promtool + Helm/docs/schema checks + release profile smoke + ferrum-stress smoke), `postgres-live-tests` (PostgreSQL 16 service container; no skips allowed).
- Manual gates (`workflow_dispatch` only): `.github/workflows/manual-gates.yml` — audit, pretarget, wal-drill, pg-batch, ha-drills, mcp-smoke. No automatic push/PR triggers.

## ferrumd Config Precedence & Runtime Gotchas
CLI args > env vars > config file > defaults.

Env vars: `FERRUMD_CONFIG`, `FERRUMD_BIND_ADDR`, `FERRUMD_STORE_DSN`, `FERRUMD_AUTH_MODE`, `FERRUMD_BEARER_TOKEN`, `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND`, `FERRUMD_LOG_FILTER`, `FERRUMD_LOG_FORMAT`, `FERRUMD_STORE_SYNCHRONOUS`, `FERRUMD_STORE_WAL_AUTOCHECKPOINT`, `FERRUMD_WRITE_QUEUE_THRESHOLD`, `FERRUMD_PG_MAX_CONNECTIONS`, `FERRUMD_PG_MIN_IDLE`, `FERRUMD_PG_ACQUIRE_TIMEOUT_SECS`, `FERRUMD_PG_STATEMENT_TIMEOUT_MS`, `FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS`, `FERRUMD_FS_WORKDIR`, `FERRUMD_GIT_REPO_ROOTS`, `FERRUMD_SQLITE_DB_ROOTS`, `FERRUMD_RATE_LIMIT_PER_SECOND`, `FERRUMD_RATE_LIMIT_BURST`.

OIDC env vars use `FERRUMD_OIDC_*`; see `configs/examples/ferrumd.env.example`.

Dev config `configs/ferrumgate.dev.toml` auto-loads if no `--config` specified and file exists (auth=disabled, in-memory SQLite). Prod config requires bearer auth; generate token with `openssl rand -hex 32`. Production uses `fs_workdir`, `git_repo_roots`, `sqlite_db_roots` allowlists.

Runtime:
- `/v1/healthz` and `/v1/readyz` are shallow 200s; `/v1/readyz/deep` checks store/queue.
- Filesystem adapter needs absolute `fs_workdir`.
- Git and SQLite adapters are disabled until their allowlists are configured.
- Structured JSON logs via `FERRUMD_LOG_FORMAT=json` or config.
- Rate limit defaults: 2 req/s sustained, burst 50.
- SQLite write queue enabled; PRAGMA: synchronous=NORMAL, wal_autocheckpoint=1000, cache_size=-64000, busy_timeout=5000ms.
- PostgreSQL available for sustained high write throughput or cross-process deployments; production HA/multi-node topology is not managed by this repo.

## Critical Invariants (Do Not Break)
- intent-scoped execution, single-use capability, provenance-first lineage, rollback-by-default
- Do not bypass: gateway, policy, capability validation, rollback prepare, provenance emission
- R3 never auto-commit; output must sanitize; scope must not exceed intent
- Capabilities: ttl_max=300s, single-use only

## Minimum Lineage Chain Before Side Effect
PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)

## Contracts / Schemas
Contract/schema surfaces: `contracts/ferrumgate-agent-contract.v1.yaml`, `contracts/ferrumgate-integrator-contract.v1.yaml`, `contracts/policy-bundle.example.yaml`, `schemas/jsonschema/`, `openapi/ferrumgate-control-api.v1.yaml`.

## Contributing Rules
- Pick one crate or document boundary at a time
- Do not change contracts/schemas without updating docs and tests
- Preserve intent/capability/provenance/rollback invariants
- Conventional commits: feat:, fix:, refactor:, docs:, test:, chore:

## Onboarding
Actual docs: `docs/guides/`, `docs/PRODUCTION_NOTES.md`.
