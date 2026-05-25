#!/usr/bin/env python3
"""Validate OpenAPI YAML: parseable and required top-level keys present."""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OPENAPI_PATH = ROOT / "openapi" / "ferrumgate-control-api.v1.yaml"

REQUIRED_TOP_KEYS = {"openapi", "info", "paths"}
REQUIRED_INFO_KEYS = {"title", "version"}


def validate_openapi_dict(data) -> list[str]:
    """Return a list of validation errors for parsed OpenAPI data."""
    if not isinstance(data, dict):
        return ["root is not a mapping"]

    errors: list[str] = []
    missing_top = REQUIRED_TOP_KEYS - set(data.keys())
    if missing_top:
        errors.append(f"missing top-level keys: {', '.join(sorted(missing_top))}")

    info = data.get("info", {})
    if not isinstance(info, dict):
        errors.append("'info' is not a mapping")
    else:
        missing_info = REQUIRED_INFO_KEYS - set(info.keys())
        if missing_info:
            errors.append(f"missing info keys: {', '.join(sorted(missing_info))}")

    paths = data.get("paths", {})
    if not isinstance(paths, dict):
        errors.append("'paths' is not a mapping")
    elif not paths:
        errors.append("'paths' is empty")

    return errors


def main() -> int:
    if not OPENAPI_PATH.exists():
        print(f"OPENAPI VALIDATION FAILED: missing {OPENAPI_PATH.relative_to(ROOT)}")
        return 1

    try:
        import yaml
    except ImportError:
        print("OPENAPI VALIDATION SKIPPED: PyYAML not available")
        return 0

    try:
        with OPENAPI_PATH.open("r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
    except Exception as exc:
        print(f"OPENAPI VALIDATION FAILED: parse error: {exc}")
        return 1

    errors = validate_openapi_dict(data)
    if errors:
        print("OPENAPI VALIDATION FAILED")
        for error in errors:
            print(f" - {error}")
        return 1

    print("OPENAPI VALIDATION PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
