#!/usr/bin/env python3
"""Validate monitoring configuration templates.

Default mode checks that templates retain an approved header marker.
``--production`` enables active unsafe-value drift detection while ignoring
commented values.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

MONITORING_DIR = Path("configs/monitoring")
MONITORING_FILES = [
    "prometheus-scrape-config.yaml",
    "alertmanager-config.yaml",
    "alertmanager-sendgrid-bridge.example.yaml",
    "ferrumgate-alerts.yaml",
]

APPROVED_MARKERS = (
    "TEMPLATE",
    "NON-PRODUCTION CLAIM",
    "LOCAL_ONLY NOTE",
)

UNSAFE_LITERALS = (
    "localhost:9093",
    "localhost:9090",
    "nip.io",
    "<your-domain>:443",
    "<monitor-name>",
    "http://localhost:9093/webhook",
    "CHANGE_ME_TO_A_SECURE_TOKEN",
)

UNSAFE_PREFIXES = ("REPLACE_WITH_",)

HEADER_LINE_LIMIT = 30

# YAML 1.1/1.2 truthy boolean scalars (case-insensitive).
_YAML_TRUE_VALUES = frozenset(("true", "yes", "on", "y"))


def _is_yaml_true(value: str) -> bool:
    """Return whether a raw YAML scalar value is a true boolean."""
    return value.strip().casefold() in _YAML_TRUE_VALUES


def strip_yaml_comment(line: str) -> str:
    """Return the active YAML portion of a line, ignoring comments.

    Comments are recognised only outside single- and double-quoted strings.
    This simple parser is sufficient for the known template YAML.
    """
    in_single = False
    in_double = False
    escaped = False
    result: list[str] = []
    for char in line:
        if escaped:
            result.append(char)
            escaped = False
            continue
        if char == "\\" and (in_single or in_double):
            result.append(char)
            escaped = True
            continue
        if char == "'" and not in_double:
            in_single = not in_single
        elif char == '"' and not in_single:
            in_double = not in_double
        elif char == "#" and not in_single and not in_double:
            break
        result.append(char)
    return "".join(result).rstrip()


def check_markers(path: Path) -> list[str]:
    """Verify the file header contains an approved template marker."""
    try:
        header = "\n".join(
            path.read_text(encoding="utf-8").splitlines()[:HEADER_LINE_LIMIT]
        )
    except Exception as exc:  # pragma: no cover - defensive
        return [f"{path}: could not read file: {exc}"]
    if any(marker in header for marker in APPROVED_MARKERS):
        return []
    return [
        f"{path}: missing approved template marker in header "
        f"(expected one of: {', '.join(APPROVED_MARKERS)})"
    ]


def check_production_drift(path: Path) -> list[str]:
    """Detect active (uncommented) unsafe values in a monitoring template."""
    errors: list[str] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except Exception as exc:  # pragma: no cover - defensive
        return [f"{path}: could not read file: {exc}"]
    for line_no, line in enumerate(lines, start=1):
        active = strip_yaml_comment(line)
        if not active.strip():
            continue
        # Check for active insecure_skip_verify set to a YAML truthy boolean.
        if ":" in active:
            key, _, value = active.partition(":")
            if key.strip().casefold() == "insecure_skip_verify" and _is_yaml_true(value):
                errors.append(
                    f"{path}:{line_no}: active unsafe setting: insecure_skip_verify: true"
                )
        active_folded = active.casefold()
        for literal in UNSAFE_LITERALS:
            if literal.casefold() in active_folded:
                errors.append(f"{path}:{line_no}: active unsafe value: {literal}")
        for prefix in UNSAFE_PREFIXES:
            if prefix.casefold() in active_folded:
                errors.append(f"{path}:{line_no}: active unsafe placeholder: {prefix}*")
    return errors


def validate(paths: list[Path], production: bool = False) -> list[str]:
    errors: list[str] = []
    for path in paths:
        if not path.exists():
            errors.append(f"{path}: file not found")
            continue
        errors.extend(check_markers(path))
        if production:
            errors.extend(check_production_drift(path))
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate FerrumGate monitoring configuration templates."
    )
    parser.add_argument(
        "--production",
        action="store_true",
        help="fail on active unsafe values (default is structural marker check only)",
    )
    parser.add_argument(
        "--monitoring-dir",
        type=Path,
        default=MONITORING_DIR,
        help="directory containing monitoring templates",
    )
    args = parser.parse_args()

    paths = [args.monitoring_dir / name for name in MONITORING_FILES]
    errors = validate(paths, production=args.production)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    mode = "production-drift" if args.production else "structural"
    print(f"Monitoring templates passed {mode} validation")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
