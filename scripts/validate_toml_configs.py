#!/usr/bin/env python3
"""Validate TOML configs: parseable and basic safety checks."""

import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
CONFIG_DIRS = [ROOT / "configs", ROOT / "configs" / "examples"]

# Files that should never have insecure defaults
PROD_LIKE_PATTERNS = ["*.prod.toml", "*nonprod*.toml", "*production*.toml"]

# Production-required controls introduced by P0-1; must be enabled in prod.
PROD_REQUIRED_CONTROLS = [
    "lifecycle_reconciliation_enabled",
    "approval_timeout_enabled",
    "audit_fail_closed",
]


def find_toml_files() -> list[Path]:
    files: list[Path] = []
    for directory in CONFIG_DIRS:
        if directory.exists():
            files.extend(directory.rglob("*.toml"))
    return sorted(files)


def _rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def check_parsable(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        with path.open("rb") as f:
            tomllib.load(f)
    except Exception as exc:
        errors.append(f"{_rel(path)}: parse error: {exc}")
    return errors


def check_safety(path: Path) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    try:
        with path.open("rb") as f:
            data = tomllib.load(f)
    except Exception:
        return errors, warnings  # parse errors handled elsewhere

    server = data.get("server", {})
    filename = path.name.lower()
    is_prod_like = any(
        Path(filename).match(p) for p in PROD_LIKE_PATTERNS
    )

    # Prod-like configs must use bearer auth
    if is_prod_like:
        auth_mode = server.get("auth_mode", "").lower()
        if auth_mode == "disabled":
            errors.append(
                f"{_rel(path)}: prod-like config has auth_mode=disabled"
            )
        if server.get("allow_insecure_nonlocal_bind", False):
            errors.append(
                f"{_rel(path)}: prod-like config has allow_insecure_nonlocal_bind=true"
            )
        for field in ("git_repo_roots", "sqlite_db_roots"):
            roots = server.get(field, [])
            if not isinstance(roots, list):
                errors.append(f"{_rel(path)}: {field} must be an array")
                continue
            for root in roots:
                if not isinstance(root, str) or not Path(root).is_absolute():
                    errors.append(
                        f"{_rel(path)}: {field} entries must be absolute paths"
                    )

    # P0-7: production-required controls
    if filename == "ferrumgate.prod.toml":
        for control in PROD_REQUIRED_CONTROLS:
            if control not in server:
                errors.append(
                    f"{_rel(path)}: production-required control '{control}' must be explicitly set to true"
                )
            elif server[control] is not True:
                errors.append(
                    f"{_rel(path)}: production-required control '{control}' must be set to true"
                )
    elif filename == "ferrumgate.dev.toml":
        if server.get("auth_mode", "").lower() != "disabled":
            errors.append(
                f"{_rel(path)}: dev config must keep auth_mode=disabled"
            )
        if server.get("store_dsn") != "sqlite::memory:":
            errors.append(
                f"{_rel(path)}: dev config must keep store_dsn='sqlite::memory:'"
            )
        for control in PROD_REQUIRED_CONTROLS:
            if server.get(control) is True:
                errors.append(
                    f"{_rel(path)}: dev config must not enable production-required control '{control}'"
                )
    elif "nonprod" in filename:
        # Nonprod may legitimately test prod-like controls; warn, don't fail.
        for control in PROD_REQUIRED_CONTROLS:
            if server.get(control) is True:
                warnings.append(
                    f"{_rel(path)}: nonprod config enables production-required control '{control}' (allowed for testing, but verify intent)"
                )

    return errors, warnings


def main() -> int:
    files = find_toml_files()
    if not files:
        print("No TOML config files found")
        return 1

    all_errors: list[str] = []
    all_warnings: list[str] = []
    for path in files:
        all_errors.extend(check_parsable(path))
        safety_errors, safety_warnings = check_safety(path)
        all_errors.extend(safety_errors)
        all_warnings.extend(safety_warnings)

    if all_warnings:
        print("TOML VALIDATION WARNINGS")
        for warning in all_warnings:
            print(f" - {warning}")

    if all_errors:
        print("TOML VALIDATION FAILED")
        for error in all_errors:
            print(f" - {error}")
        return 1

    print(f"TOML VALIDATION PASSED ({len(files)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
