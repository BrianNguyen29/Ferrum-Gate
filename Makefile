.PHONY: help check fmt lint test docs validate tree pretarget audit wal-drill site-build site-serve site-check

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
	@echo "make site-build - build static site with Zola (optional; requires zola binary)"
	@echo "make site-serve - serve static site locally with Zola (optional; requires zola binary)"
	@echo "make site-check - check site scaffold presence (no zola required)"

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

site-build:
	@echo "Building static site with Zola..."
	@if command -v zola >/dev/null 2>&1; then \
		cd site && zola build; \
	else \
		echo "zola not found; skipping build. Install Zola to use this target."; \
		echo "See https://www.getzola.org/documentation/getting-started/installation/"; \
	fi

site-serve:
	@echo "Serving static site locally with Zola..."
	@if command -v zola >/dev/null 2>&1; then \
		cd site && zola serve; \
	else \
		echo "zola not found; skipping serve. Install Zola to use this target."; \
		echo "See https://www.getzola.org/documentation/getting-started/installation/"; \
	fi

site-check:
	@echo "Checking site scaffold presence..."
	@test -f site/config.toml && echo "[OK] site/config.toml" || echo "[MISSING] site/config.toml"
	@test -f site/templates/base.html && echo "[OK] site/templates/base.html" || echo "[MISSING] site/templates/base.html"
	@test -f site/templates/index.html && echo "[OK] site/templates/index.html" || echo "[MISSING] site/templates/index.html"
	@test -f site/static/css/main.css && echo "[OK] site/static/css/main.css" || echo "[MISSING] site/static/css/main.css"
	@test -f site/content/_index.md && echo "[OK] site/content/_index.md" || echo "[MISSING] site/content/_index.md"
