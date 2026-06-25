.PHONY: help check fmt lint test docs test-python-validators validate tree pretarget audit secret-scan wal-drill pg-restart-drill pg-restore-drill pg-migration-drill pg-backup-retention-drill pg-partial-failure-drill pg-sustained-workload-drill pg-sustained-workload-extended pg-scheduled-timer-simulation pg-local-batch ha-local-setup ha-local-failover-drill ha-local-ferrumd-reconnect-drill ha-local-teardown site-build site-serve site-check slo-sustained-dry-run restore-drill stress check-pilot-readiness domainless-tier1-fast domainless-tier1-gate s3-test release-preflight release-preflight-execute perf-gate perf-baseline-update

help:
	@echo "make check     - cargo check workspace"
	@echo "make fmt       - cargo fmt --all"
	@echo "make lint      - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "make test      - cargo test --workspace"
	@echo "make coverage  - generate test coverage report (requires cargo-tarpaulin or cargo-llvm-cov)"
	@echo "make docs      - validate docs links and site scaffold"
	@echo "make validate  - run expanded local validation (layout, contracts, templates, toml, openapi, docs links, CI badges, MCP tools)"
	@echo "make tree      - print repository tree"
	@echo "make pretarget - local pre-target gate (config validation, restore drill, doc presence, expanded validators)"
	@echo "make audit     - local security audit gate (cargo-deny / cargo-audit)"
	@echo "make secret-scan - local hardcoded secrets scan (dependency-free)"
	@echo "make wal-drill      - local SQLite WAL crash-recovery drill"
	@echo "make pg-restart-drill - local PostgreSQL container restart recovery drill"
	@echo "make pg-restore-drill - local PostgreSQL populated backup/restore drill"
	@echo "make pg-migration-drill - local SQLite to PostgreSQL migration drill"
	@echo "make pg-backup-retention-drill - local PostgreSQL backup/retention/offsite drill"
	@echo "make pg-partial-failure-drill - local PostgreSQL resume/partial-failure drill"
	@echo "make pg-sustained-workload-drill - local PostgreSQL sustained workload drill (short default, env override for longer)"
	@echo "make pg-sustained-workload-extended - local PostgreSQL sustained workload drill (extended 120s @ 1 rps, env override supported)"
	@echo "make pg-scheduled-timer-simulation - local text-only systemd timer due/skip simulation (no install)"
	@echo "make pg-local-batch - run all local PostgreSQL drills + sustained workload + timer simulation in deterministic order"
	@echo "make ha-local-setup         - start local HA primary/standby PostgreSQL simulation"
	@echo "make ha-local-failover-drill - run local HA failover drill (requires ha-local-setup first)"
	@echo "make ha-local-ferrumd-reconnect-drill - run local HA ferrumd reconnect drill (setup if needed, measures app-level RTO)"
	@echo "make ha-local-teardown      - stop and remove local HA simulation containers/volumes"
	@echo "make domainless-tier1-fast  - lightweight Tier 1 gate (docs/validate + syntax/dry-run/light checks, no heavy Docker drills)"
	@echo "make domainless-tier1-gate  - full domainless Tier 1 gate (docs/validate + pg-local-batch + HA setup/failover/reconnect/teardown)"
	@echo "make restore-drill  - local temp SQLite backup/restore drill (requires ferrumctl binary or cargo build)"
	@echo "make s3-test   - run S3 adapter MinIO integration tests (requires local MinIO at localhost:9000)"
	@echo "make stress    - stress tests against a running service (requires BASE_URL env var)"
	@echo "make check-pilot-readiness - pilot readiness probes (requires running server via --server-url or FERRUMCTL_SERVER_URL)"
	@echo "make perf-gate           - advisory performance regression gate (short-duration, non-blocking)"
	@echo "make perf-baseline-update - regenerate sample performance baselines (developer use)"
	@echo "make site-build - build static site with Zola (optional; requires zola binary)"
	@echo "make site-serve - serve static site locally with Zola (optional; requires zola binary)"
	@echo "make site-check - check site scaffold presence (no zola required)"
	@echo "make release-preflight - run conservative release preflight checks (dry-run, no push/publish)"
	@echo "make release-preflight-execute - run release preflight with SBOM generation (still no push/publish)"
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

test-python-validators:
	@echo "Running Python validator tests..."
	@python3 -m unittest discover -s tests -p 'test_validate_*.py' -v

validate:
	@echo "Running local validation (layout + contract consistency + MCP required-tools + evidence templates + toml + openapi + docs-links + CI badges + python-validator-tests)..."
	@bash scripts/validate_repo_layout.sh
	@python3 scripts/check_contract_consistency.py
	@bash scripts/validate_mcp_required_tools.sh
	@python3 scripts/validate_evidence_templates.py
	@python3 scripts/validate_toml_configs.py
	@python3 scripts/validate_openapi_yaml.py
	@python3 scripts/validate_docs_links.py
	@python3 scripts/validate_ci_badges.py
	@$(MAKE) test-python-validators
	@$(MAKE) site-check

tree:
	find . -maxdepth 4 | sort

pretarget:
	@echo "Running local pre-target gate..."
	@bash scripts/run_pre_target_gate.sh

audit:
	@bash scripts/run_security_audit.sh

secret-scan:
	@bash scripts/run_secret_scan.sh

wal-drill:
	@echo "Running local SQLite WAL crash-recovery drill..."
	@bash scripts/run_wal_crash_recovery_drill.sh

pg-restart-drill:
	@echo "Running local PostgreSQL container restart recovery drill..."
	@bash scripts/run_pg_container_restart_drill.sh

pg-restore-drill:
	@echo "Running local PostgreSQL populated backup/restore drill..."
	@bash scripts/run_pg_restore_drill.sh

pg-migration-drill:
	@echo "Running local SQLite to PostgreSQL migration drill..."
	@bash scripts/run_pg_migration_drill.sh

pg-backup-retention-drill:
	@echo "Running local PostgreSQL backup/retention/offsite drill..."
	@bash scripts/run_pg_backup_retention_drill.sh

pg-partial-failure-drill:
	@echo "Running local PostgreSQL partial-failure/resume drill..."
	@bash scripts/run_pg_partial_failure_drill.sh

pg-sustained-workload-drill:
	@echo "Running local PostgreSQL sustained workload drill..."
	@bash scripts/run_pg_sustained_workload_drill.sh

pg-sustained-workload-extended:
	@echo "Running local PostgreSQL sustained workload drill (extended)..."
	@FERRUMD_RATE_LIMIT_PER_SECOND=1000 FERRUMD_RATE_LIMIT_BURST=10000 SUSTAINED_PHASES='[{"name":"extended","duration_sec":120,"rate_rps":1.0}]' bash scripts/run_pg_sustained_workload_drill.sh

pg-scheduled-timer-simulation:
	@echo "Running local PostgreSQL scheduled timer simulation..."
	@bash scripts/run_pg_scheduled_timer_simulation.sh

pg-local-batch:
	@echo "Running local PostgreSQL batch: migration, restore, backup/retention, partial-failure, sustained workload, timer simulation..."
	@$(MAKE) pg-migration-drill && \
	$(MAKE) pg-restore-drill && \
	$(MAKE) pg-backup-retention-drill && \
	$(MAKE) pg-partial-failure-drill && \
	$(MAKE) pg-sustained-workload-drill && \
	$(MAKE) pg-scheduled-timer-simulation
	@echo "PG LOCAL BATCH: ALL TARGETS PASSED"

ha-local-setup:
	@echo "Setting up local HA PostgreSQL simulation..."
	@bash scripts/setup_ha_local.sh

ha-local-failover-drill:
	@echo "Running local HA failover drill..."
	@bash scripts/run_ha_local_failover_drill.sh

ha-local-ferrumd-reconnect-drill:
	@echo "Running local HA ferrumd reconnect drill..."
	@bash scripts/run_ha_local_ferrumd_reconnect_drill.sh

ha-local-teardown:
	@echo "Tearing down local HA PostgreSQL simulation..."
	@bash scripts/teardown_ha_local.sh

restore-drill:
	@echo "Running local temp SQLite backup/restore drill..."
	@bash scripts/run_local_restore_drill.sh

s3-test:
	@echo "Running S3 adapter MinIO integration tests..."
	@if curl -sSf http://localhost:9000/minio/health/live >/dev/null 2>&1; then \
		echo "[OK] MinIO detected at localhost:9000"; \
		cargo test -p ferrum-adapter-s3 --features s3-client --test minio_integration -- --ignored; \
	else \
		echo "[SKIP] MinIO not running at localhost:9000."; \
		echo "To run these tests, start MinIO with:"; \
		echo "  docker run -d -p 9000:9000 -p 9001:9001 -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin minio/minio server /data --console-address \":9001\""; \
		echo "Then create a versioned bucket:"; \
		echo "  mc alias set local http://localhost:9000 minioadmin minioadmin"; \
		echo "  mc mb local/ferrum-test-bucket"; \
		echo "  mc version enable local/ferrum-test-bucket"; \
		echo "Then re-run: make s3-test"; \
		exit 1; \
	fi
stress:
	@echo "Running stress tests against $$BASE_URL..."
	@echo "Requires a running local or target service. Set BASE_URL, TOKEN, DURATION, WORKERS as needed."
	@bash scripts/stress/run-all.sh

check-pilot-readiness:
	@echo "Running pilot readiness checks..."
	@echo "Requires a running server. Use --server-url or FERRUMCTL_SERVER_URL env var."
	@FERRUMCTL_PATH=$$( \
		if [ -n "$$FERRUMCTL" ]; then printf '%s' "$$FERRUMCTL"; \
		elif [ -x target/release/ferrumctl ]; then printf '%s' "target/release/ferrumctl"; \
		elif [ -x target/debug/ferrumctl ]; then printf '%s' "target/debug/ferrumctl"; \
		else printf '%s' "ferrumctl"; fi \
	); \
	echo "Using FERRUMCTL=$$FERRUMCTL_PATH"; \
	FERRUMCTL="$$FERRUMCTL_PATH" python3 scripts/check_pilot_readiness.py

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

coverage:
	@echo "Running coverage report..."
	@if command -v cargo-tarpaulin >/dev/null 2>&1; then \
		cargo tarpaulin --workspace --out Html --out Stdout; \
	elif command -v cargo-llvm-cov >/dev/null 2>&1; then \
		cargo llvm-cov --workspace --html; \
	else \
		echo "cargo-tarpaulin not found. Install it to run coverage locally:"; \
		echo "  cargo install --locked cargo-tarpaulin"; \
		echo "Or use cargo-llvm-cov:"; \
		echo "  cargo install --locked cargo-llvm-cov"; \
		echo "  cargo llvm-cov --workspace --html"; \
		exit 1; \
	fi

domainless-tier1-fast:
	@echo "Running domainless Tier 1 fast gate..."
	@$(MAKE) docs
	@$(MAKE) validate
	@echo "[OK] Tier 1 fast gate passed (docs + validate only; no heavy Docker drills)"

domainless-tier1-gate:
	@echo "Running full domainless Tier 1 gate..."
	@$(MAKE) domainless-tier1-fast
	@$(MAKE) pg-local-batch
	@$(MAKE) ha-local-setup
	@$(MAKE) ha-local-failover-drill
	@$(MAKE) ha-local-ferrumd-reconnect-drill
	@$(MAKE) ha-local-teardown
	@echo "DOMAINLESS TIER 1 GATE: ALL TARGETS PASSED"

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

perf-gate:
	@echo "Running advisory performance regression gate..."
	@bash scripts/run_perf_gate.sh --dry-run --duration 5 --scenarios "health,intent-compile,sqlite-contention"

perf-baseline-update:
	@echo "Regenerating sample performance baselines (advisory only)..."
	@echo "This runs ferrum-stress and overwrites baselines/*.json with current results."
	@echo "You must manually review and label the generated baselines before committing."
	@bash scripts/run_perf_gate.sh --duration 5 --scenarios "health,intent-compile,sqlite-contention"
	@echo "[INFO] Baselines regenerated. Review them in baselines/ before removing the SAMPLE label."

release-preflight:
	@echo "Running release preflight (dry-run, no push/publish)..."
	@bash scripts/prepare_release.sh --dry-run

release-preflight-execute:
	@echo "Running release preflight with SBOM generation (still no push/publish)..."
	@bash scripts/prepare_release.sh --execute
