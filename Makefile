.PHONY: help check fmt lint test docs validate tree

help:
	@echo "make check     - cargo check workspace"
	@echo "make fmt       - cargo fmt --all"
	@echo "make lint      - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "make test      - cargo test --workspace"
	@echo "make docs      - build docs placeholder"
	@echo "make validate  - validate contracts/openapi/schemas placeholder"
	@echo "make tree      - print repository tree"

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

tree:
	find . -maxdepth 4 | sort
