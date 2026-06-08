#!/usr/bin/env python3
"""Validate that documented route scopes cover the gateway route scope map."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROUTE_SOURCES = [
    ROOT / "crates" / "ferrum-gateway" / "src" / "routes.rs",
    ROOT / "crates" / "ferrum-gateway" / "src" / "server.rs",
]
DOC = ROOT / "docs" / "security" / "scoped-tokens-rbac.md"


def main() -> int:
    route_source = next((path for path in ROUTE_SOURCES if path.exists()), None)
    if route_source is None:
        print("No gateway route scope source found", file=sys.stderr)
        return 1
    route_text = route_source.read_text(encoding="utf-8")
    if route_source.name == "server.rs":
        start = route_text.find("fn required_scope_for_path")
        end = route_text.find("// ---------------------------------------------------------------------------", start)
        if start == -1 or end == -1:
            print("Could not isolate required_scope_for_path in server.rs", file=sys.stderr)
            return 1
        route_text = route_text[start:end]
    doc_text = DOC.read_text(encoding="utf-8")

    enforced = set(re.findall(r'Some\("([^"]+)"\)', route_text))
    documented = set(re.findall(r"`([a-z*][a-z0-9:*_-]+)`", doc_text))

    missing = sorted(enforced - documented)
    if missing:
        print("Route scope documentation is missing enforced scopes:", file=sys.stderr)
        for scope in missing:
            print(f" - {scope}", file=sys.stderr)
        return 1

    print(f"Route scope docs cover {len(enforced)} enforced scopes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
