.PHONY: help check fmt lint test docs validate tree pretarget audit wal-drill

help:
	@echo "make check     - cargo check workspace"
	@echo "make fmt       - cargo fmt --all"
	@echo "make lint      - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "make test      - cargo test --workspace"
	@echo "make docs      - build docs placeholder"
	@echo "make validate  - validate contracts/openapi/schemas placeholder"
	@echo "make tree      - print repository tree"
	@echo "make pretarget - local pre-target gate (config validation, restore drill, doc presence)"
	@echo "make audit     - local security audit gate (cargo-deny / cargo-audit)"
	@echo "make wal-drill - local SQLite WAL crash-recovery drill"

check:
	cargo check --workspace

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

docs:
	@echo "Docs live in ./docs"

validate:
	@echo "Running local validation (layout + contract consistency + MCP required-tools)..."
	@bash scripts/validate_repo_layout.sh
	@python3 scripts/check_contract_consistency.py
	@bash scripts/validate_mcp_required_tools.sh

tree:
	find . -maxdepth 4 | sort

pretarget:
	@echo "Running local pre-target gate..."
	@bash scripts/run_pre_target_gate.sh

audit:
	@bash scripts/run_security_audit.sh

wal-drill:
	@echo "Running local SQLite WAL crash-recovery drill..."
	@bash scripts/run_wal_crash_recovery_drill.sh
