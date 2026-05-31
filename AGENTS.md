# AGENTS.md — FerrumGate Repository

## Workspace & Toolchain
- Rust edition 2024, resolver "2", 22 workspace members (see Cargo.toml lines 3-24)
- Toolchain: stable; clippy MSRV 1.85.0; rustfmt max_width=100, Unix newline, reorder_imports=true
- clippy.toml: msrv=1.85.0, too-many-arguments-threshold=8, type-complexity-threshold=350

## Makefile Commands
```
make check   # cargo check --workspace
make fmt     # cargo fmt --all
make lint    # cargo clippy --workspace --all-targets -- -D warnings
make test    # cargo test --workspace
make audit   # local security audit gate (cargo-deny / cargo-audit)
```
- Check formatting without mutation: `cargo fmt --all -- --check`
- Feature-gated package check (e.g., ferrum-migrate): `cargo check --package ferrum-migrate --features postgres`
- Layout/contract validation: `bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py`
- CI runs: layout validation, contract consistency, fmt check, cargo check, clippy, and cargo test (no `|| true` — failures are not swallowed)
- Pre-target gate (local only): `bash scripts/run_pre_target_gate.sh` — validates config examples, restore drill, evidence skeleton generator, docs present, bearer-auth smoke
- Manual gates (workflow_dispatch only): `.github/workflows/manual-gates.yml` — audit, pretarget, wal-drill, mcp-smoke. Triggered manually; may incur GitHub Actions minutes when run. No automatic push/PR triggers.

## ferrumd Config Precedence
CLI args > env vars > config file > defaults.

Env vars: `FERRUMD_CONFIG`, `FERRUMD_BIND_ADDR`, `FERRUMD_STORE_DSN`, `FERRUMD_AUTH_MODE`, `FERRUMD_BEARER_TOKEN`, `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND`, `FERRUMD_LOG_FILTER`, `FERRUMD_STORE_SYNCHRONOUS`, `FERRUMD_STORE_WAL_AUTOCHECKPOINT`, `FERRUMD_PG_MAX_CONNECTIONS`, `FERRUMD_PG_MIN_IDLE`, `FERRUMD_PG_ACQUIRE_TIMEOUT_SECS`.

Dev config `configs/ferrumgate.dev.toml` auto-loads if no `--config` specified and file exists (auth=disabled, in-memory SQLite). Prod config requires bearer auth; generate token with `openssl rand -hex 32`.

## Critical Invariants (Do Not Break)
- intent-scoped execution, single-use capability, provenance-first lineage, rollback-by-default
- Do not bypass: gateway, policy, capability validation, rollback prepare, provenance emission
- R3 never auto-commit; output must sanitize; scope must not exceed intent
- Capabilities: ttl_max=300s, single-use only

## Minimum Lineage Chain Before Side Effect
PolicyEvaluated → CapabilityMinted → ActionProposalSubmitted → SideEffectPrepared → ToolCallPrepared → ToolCallExecuted → SideEffectVerified → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)

## Stale/Missing Doc Warning
README.md and CONTRIBUTING.md now correctly reference actual onboarding paths. Current actual onboarding: `docs/guides/`, `docs/PRODUCTION_NOTES.md`.

## Runtime Notes
- SQLite write queue enabled (eliminates lock thrash); PRAGMA: synchronous=NORMAL, wal_autocheckpoint=1000, cache_size=-64000, busy_timeout=5000ms
- PostgreSQL recommended for sustained high write throughput or multi-node deployment

## Contributing Rules
- Pick one crate or document boundary at a time
- Do not change contracts/schemas without updating docs and tests
- Preserve intent/capability/provenance/rollback invariants
- Conventional commits: feat:, fix:, refactor:, docs:, test:, chore:
