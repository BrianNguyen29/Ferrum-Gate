# tests

Mục tiêu:
- contract conformance tests
- gateway flow smoke tests
- policy decision tests
- rollback path tests
- poisoned context regression tests

Python validator tests:
- Run with `make test-python-validators` or `python3 -m unittest discover -s tests -p 'test_validate_*.py'`
- Cover `scripts/validate_evidence_templates.py`, `scripts/validate_toml_configs.py`, `scripts/validate_openapi_yaml.py`, and `scripts/validate_docs_links.py`
