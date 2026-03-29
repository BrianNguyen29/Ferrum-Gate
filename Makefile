.PHONY: help check fmt lint test docs validate tree rc-evidence

help:
	@echo "make check         - cargo check workspace"
	@echo "make fmt           - cargo fmt --all"
	@echo "make lint          - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "make test          - cargo test --workspace"
	@echo "make docs           - build docs placeholder"
	@echo "make validate       - validate contracts/openapi/schemas here"
	@echo "make tree           - print repository tree"
	@echo "make rc-evidence    - generate reproducible v1 RC evidence"

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
	@echo "Validate OpenAPI / JSON Schema / YAML contracts here"
	python3 scripts/check_contract_consistency.py

tree:
	find . -maxdepth 4 | sort

rc-evidence:
	@echo "Generating v1 RC evidence..."
	python3 scripts/generate_rc_evidence.py
