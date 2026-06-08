#!/usr/bin/env python3
"""Parse all fenced TOML snippets in docs to prevent stale config examples."""

from __future__ import annotations

import re
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
SNIPPET_RE = re.compile(r"```toml\n(.*?)\n```", re.DOTALL)


def main() -> int:
    errors: list[str] = []
    count = 0
    for path in sorted((ROOT / "docs").rglob("*.md")):
        text = path.read_text(encoding="utf-8")
        for index, match in enumerate(SNIPPET_RE.finditer(text), start=1):
            count += 1
            snippet = match.group(1)
            try:
                tomllib.loads(snippet)
            except Exception as exc:
                rel = path.relative_to(ROOT)
                errors.append(f"{rel} TOML snippet #{index}: {exc}")

    if errors:
        print("Documentation TOML snippet validation failed", file=sys.stderr)
        for error in errors:
            print(f" - {error}", file=sys.stderr)
        return 1

    print(f"Documentation TOML snippets parse cleanly ({count} snippets)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
