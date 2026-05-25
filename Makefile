.PHONY: help check fmt lint test docs validate tree pretarget audit wal-drill pg-restart-drill site-build site-serve site-check slo-sustained-dry-run restore-drill stress check-pilot-readiness

help:
	@echo "make check     - cargo check workspace"
	@echo "make fmt       - cargo fmt --all"
	@echo "make lint      - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "make test      - cargo test --workspace"
	@echo "make docs      - validate docs links and site scaffold"
	@echo "make validate  - run expanded local validation (layout, contracts, templates, toml, openapi, docs links, MCP tools)"
	@echo "make tree      - print repository tree"
	@echo "make pretarget - local pre-target gate (config validation, restore drill, doc presence, expanded validators)"
	@echo "make audit     - local security audit gate (cargo-deny / cargo-audit)"
	@echo "make wal-drill      - local SQLite WAL crash-recovery drill"
	@echo "make pg-restart-drill - local PostgreSQL container restart recovery drill"
	@echo "make restore-drill  - local temp SQLite backup/restore drill (requires ferrumctl binary or cargo build)"
	@echo "make stress         - stress tests against a running service (requires BASE_URL env var)"
	@echo "make check-pilot-readiness - pilot readiness probes (requires running server via --server-url or FERRUMCTL_SERVER_URL)"
	@echo "make site-build - build static site with Zola (optional; requires zola binary)"
	@echo "make site-serve - serve static site locally with Zola (optional; requires zola binary)"
	@echo "make site-check - check site scaffold presence (no zola required)"
	@echo "make slo-sustained-dry-run - safe dry-run rehearsal for SLO sustained observation"

check:
	cargo check --workspace

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

docs:
	@echo "Running docs validation..."
	@python3 scripts/validate_docs_links.py
	@$(MAKE) site-check

validate:
	@echo "Running local validation (layout + contract consistency + MCP required-tools + evidence templates + toml + openapi + docs-links)..."
	@bash scripts/validate_repo_layout.sh
	@python3 scripts/check_contract_consistency.py
	@bash scripts/validate_mcp_required_tools.sh
	@python3 scripts/validate_evidence_templates.py
	@python3 scripts/validate_toml_configs.py
	@python3 scripts/validate_openapi_yaml.py
	@python3 scripts/validate_docs_links.py

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

pg-restart-drill:
	@echo "Running local PostgreSQL container restart recovery drill..."
	@bash scripts/run_pg_container_restart_drill.sh

restore-drill:
	@echo "Running local temp SQLite backup/restore drill..."
	@bash scripts/run_local_restore_drill.sh

stress:
	@echo "Running stress tests against $$BASE_URL..."
	@echo "Requires a running local or target service. Set BASE_URL, TOKEN, DURATION, WORKERS as needed."
	@bash scripts/stress/run-all.sh

check-pilot-readiness:
	@echo "Running pilot readiness checks..."
	@echo "Requires a running server. Use --server-url or FERRUMCTL_SERVER_URL env var."
	@python3 scripts/check_pilot_readiness.py

slo-sustained-dry-run:
	@echo "Running SLO sustained observation in dry-run mode..."
	@bash scripts/run_slo_sustained_observation.sh --dry-run

site-build:
	@echo "Building static site with Zola..."
	@if command -v zola >/dev/null 2>&1; then \
		cd site && zola build; \
		if grep -q "FerrumGate in 10 Minutes" public/guides/quickstart/index.html 2>/dev/null; then \
			echo "[OK] Guide content renders correctly (quickstart)"; \
		else \
			echo "[ERROR] Guide content missing or misresolved"; \
			exit 1; \
		fi; \
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
	@test -f site/templates/guide.html && echo "[OK] site/templates/guide.html" || echo "[MISSING] site/templates/guide.html"
	@test -f site/static/css/main.css && echo "[OK] site/static/css/main.css" || echo "[MISSING] site/static/css/main.css"
	@test -f site/content/_index.md && echo "[OK] site/content/_index.md" || echo "[MISSING] site/content/_index.md"
	@test -f site/content/guides/_index.md && echo "[OK] site/content/guides/_index.md" || echo "[MISSING] site/content/guides/_index.md"
	@test -n "$(shell ls site/content/guides/*.md 2>/dev/null)" && echo "[OK] guide pages found" || echo "[MISSING] guide pages"
	@for f in site/content/guides/*.md; do \
		target=$$(grep 'source_path = ' "$$f" 2>/dev/null | cut -d'"' -f2 || true); \
		if [ -n "$$target" ]; then \
			basename=$$(basename "$$target"); \
			if [ -f "docs/guides/$$basename" ]; then \
				echo "[OK] docs/guides/$$basename exists"; \
			else \
				echo "[MISSING] docs/guides/$$basename referenced by $$f"; \
				exit 1; \
			fi; \
		fi; \
	done
