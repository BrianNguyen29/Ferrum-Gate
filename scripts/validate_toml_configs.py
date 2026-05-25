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


def check_safety(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        with path.open("rb") as f:
            data = tomllib.load(f)
    except Exception:
        return errors  # parse errors handled elsewhere

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

    return errors


def main() -> int:
    files = find_toml_files()
    if not files:
        print("No TOML config files found")
        return 1

    all_errors: list[str] = []
    for path in files:
        all_errors.extend(check_parsable(path))
        all_errors.extend(check_safety(path))

    if all_errors:
        print("TOML VALIDATION FAILED")
        for error in all_errors:
            print(f" - {error}")
        return 1

    print(f"TOML VALIDATION PASSED ({len(files)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
