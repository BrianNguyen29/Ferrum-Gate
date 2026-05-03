# AGENTS.md — FerrumGate Repository

## Workspace & Toolchain
- Rust edition 2024, resolver "2", 20 workspace members (see Cargo.toml lines 2-22)
- Toolchain: stable; clippy MSRV 1.85.0; rustfmt max_width=100, Unix newline, reorder_imports=true
- clippy.toml: msrv=1.85.0, too-many-arguments-threshold=8, type-complexity-threshold=350

## Makefile Commands
```
make check   # cargo check --workspace
make fmt     # cargo fmt --all
make lint    # cargo clippy --workspace --all-targets -- -D warnings
make test    # cargo test --workspace
```
- Check formatting without mutation: `cargo fmt --all -- --check`
- Layout/contract validation: `bash scripts/validate_repo_layout.sh && python3 scripts/check_contract_consistency.py`
- CI runs: layout validation, contract consistency, fmt check, cargo check, clippy, and cargo test (no `|| true` — failures are not swallowed)
- Pre-target gate (local only): `bash scripts/run_pre_target_gate.sh` — validates config examples, restore drill, evidence skeleton generator, docs present, bearer-auth smoke

## Current Verification Status (2026-05-03)
- Layout/contract validation: PASSES locally
- `cargo fmt --all -- --check`: PASSES locally
- `cargo check --workspace`: PASSES locally
- `cargo clippy --workspace --all-targets -- -D warnings`: PASSES locally
- `cargo test --workspace`: PASSES locally (all packages)
- Summary: layout=0 contract=0 fmt=0 check=0 clippy=0 test=0

## ferrumd Config Precedence
CLI args > env vars > config file > defaults.

Env vars: `FERRUMD_CONFIG`, `FERRUMD_BIND_ADDR`, `FERRUMD_STORE_DSN`, `FERRUMD_AUTH_MODE`, `FERRUMD_BEARER_TOKEN`, `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND`, `FERRUMD_LOG_FILTER`, `FERRUMD_STORE_SYNCHRONOUS`, `FERRUMD_STORE_WAL_AUTOCHECKPOINT`.

Dev config `configs/ferrumgate.dev.toml` auto-loads if no `--config` specified and file exists (auth=disabled, in-memory SQLite). Prod config requires bearer auth; generate token with `openssl rand -hex 32`.

## Critical Invariants (Do Not Break)
- intent-scoped execution, single-use capability, provenance-first lineage, rollback-by-default
- Do not bypass: gateway, policy, capability validation, rollback prepare, provenance emission
- R3 never auto-commit; output must sanitize; scope must not exceed intent
- Capabilities: ttl_max=300s, single-use only

## Minimum Lineage Chain Before Side Effect
ActionProposalSubmitted → PolicyEvaluated → CapabilityMinted → ToolCallPrepared → ToolCallExecuted → SideEffectPrepared → SideEffectVerified → Terminal (SideEffectCommitted | SideEffectCompensated | SideEffectRolledBack)

## Stale/Missing Doc Warning
README.md and CONTRIBUTING.md now correctly reference actual onboarding paths. Older documentation or artifacts may still contain historical stale references to non-existent docs (e.g., `docs/00-repo-map.md`, `docs/01-business-overview.md`). Current actual onboarding: `docs/implementation-path/00-start-here.md`, `docs/implementation-path/01-current-state.md`, `docs/implementation-path/06-guardrails-and-invariants.md`, `docs/PRODUCTION_NOTES.md`.

## Production Notes
- SQLite write queue enabled (eliminates lock thrash); PRAGMA: synchronous=NORMAL, wal_autocheckpoint=1000, cache_size=-64000, busy_timeout=5000ms
- PostgreSQL recommended for sustained high write throughput or multi-node deployment

## Production Readiness Roadmap
- Durable todo list with priorities, blockers, owners, evidence: `docs/implementation-path/67-production-readiness-roadmap.md`
- P0 blockers: CI swallow fixed; target-host evidence, G2 signoff, backup automation, operator signoff remain pending (operator-owned)
- P1 items: readiness semantics, configurable rate limit, structured logging, metrics/observability
- No production-ready claim; FerrumGate v1 is RC-ready/conditional; G2 requires operator action

## Contributing Rules
- Pick one crate or document boundary at a time
- Do not change contracts/schemas without updating docs and tests
- Preserve intent/capability/provenance/rollback invariants
- Conventional commits: feat:, fix:, refactor:, docs:, test:, chore:
