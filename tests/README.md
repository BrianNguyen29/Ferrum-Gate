# tests

Integration and validation tests for FerrumGate.

## Coverage

- Contract conformance tests
- Gateway flow smoke tests
- Policy decision tests
- Rollback path tests
- Poisoned context regression tests

## Python validator tests

- Run with `make test-python-validators` or `python3 -m unittest discover -s tests -p 'test_validate_*.py'`
- Cover `scripts/validate_evidence_templates.py`, `scripts/validate_toml_configs.py`, `scripts/validate_openapi_yaml.py`, and `scripts/validate_docs_links.py`
