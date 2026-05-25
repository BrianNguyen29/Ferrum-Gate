#!/usr/bin/env python3
"""Validate internal docs links in docs/guides/ — guide-to-guide links must resolve."""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUIDES_DIR = ROOT / "docs" / "guides"
DOCS_DIR = ROOT / "docs"

LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")

EXTERNAL_PREFIXES = (
    "http://",
    "https://",
    "mailto:",
    "tel:",
)


def find_guide_files() -> list[Path]:
    if not GUIDES_DIR.exists():
        return []
    return sorted(GUIDES_DIR.glob("*.md"))


def resolve_link(source: Path, link_target: str) -> Path | None:
    target = link_target.split("#")[0]
    if not target:
        return None

    if target.startswith("/"):
        resolved = ROOT / target.lstrip("/")
    else:
        resolved = source.parent / target

    try:
        resolved = resolved.resolve()
    except (OSError, ValueError):
        pass

    return resolved


def check_file(path: Path) -> list[str]:
    errors: list[str] = []
    text = path.read_text(encoding="utf-8")

    for match in LINK_RE.finditer(text):
        link_text, link_target = match.groups()

        if link_target.startswith(EXTERNAL_PREFIXES):
            continue
        if link_target.startswith("#"):
            continue

        resolved = resolve_link(path, link_target)
        if resolved is None:
            continue

        # Only validate links that stay within docs/ (repo-internal docs)
        try:
            in_docs = DOCS_DIR in resolved.parents or resolved == DOCS_DIR
        except ValueError:
            in_docs = False

        if not in_docs:
            continue

        if not resolved.exists():
            errors.append(
                f"{path.relative_to(ROOT)}: broken link to '{link_target}'"
                f" (resolved: {resolved.relative_to(ROOT)})"
            )

    return errors


def main() -> int:
    files = find_guide_files()
    if not files:
        print("DOCS LINK VALIDATION SKIPPED: no guide files found")
        return 0

    all_errors: list[str] = []
    for path in files:
        all_errors.extend(check_file(path))

    if all_errors:
        print("DOCS LINK VALIDATION FAILED")
        for error in all_errors:
            print(f" - {error}")
        return 1

    print(f"DOCS LINK VALIDATION PASSED ({len(files)} guide files checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
