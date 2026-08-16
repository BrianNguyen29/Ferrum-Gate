#!/usr/bin/env python3
"""validate_adapter_maturity.py

Advisory drift check between adapter maturity labels (ROADMAP/docs) and the
source tree. This script is intentionally non-blocking: it emits warnings but
exits 0 unless required docs/files are missing or unreadable.

Checks:
- ROADMAP adapter rows are present for known adapters.
- P4 adapters do not contain literal "not implemented" placeholders in their
  source directories.
- P2 GCS source still declares the live SDK path as a seam / not yet implemented
  (or warns if the label may need review).
- scripts/adapter_smoke.sh exists for shape-only S3/GCS + MCP tool evidence.

This does not change labels or block CI.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Adapters we expect to see classified in ROADMAP. Values are (expected_min_level, name).
KNOWN_ADAPTERS: dict[str, str] = {
    "Filesystem adapter": "fs",
    "Git adapter": "git",
    "HTTP adapter": "http",
    "SQLite adapter": "sqlite",
    "Mail draft adapter": "maildraft",
    "S3 adapter": "s3",
    "GCS adapter": "gcs",
    "Azure Blob adapter": "azure",
}

P4_SOURCE_ADAPTERS = {"fs", "git", "http", "sqlite", "maildraft"}
P2_SHAPE_ONLY = {"s3", "gcs"}


def parse_roadmap_adapter_rows(path: Path) -> dict[str, int]:
    """Return a map of adapter short name -> parsed P level from ROADMAP table."""
    content = path.read_text(encoding="utf-8")
    levels: dict[str, int] = {}
    for line in content.splitlines():
        if "adapter" not in line.lower():
            continue
        # Match rows like "| S3 adapter | P2 | Experimental | ... |"
        m = re.match(r"\|\s*([^|]+?adapter[^|]*)\s*\|\s*P(\d)", line, re.IGNORECASE)
        if not m:
            continue
        name = m.group(1).strip()
        level = int(m.group(2))
        if name in KNOWN_ADAPTERS:
            levels[KNOWN_ADAPTERS[name]] = level
    return levels


def directory_contains_phrase(directory: Path, phrase: str) -> bool:
    """Return True if any .rs file under directory contains phrase (case-insensitive)."""
    if not directory.exists():
        return False
    for path in directory.rglob("*.rs"):
        try:
            text = path.read_text(encoding="utf-8")
            if phrase.lower() in text.lower():
                return True
        except Exception:
            continue
    return False


def main() -> int:
    warnings: list[str] = []
    roadmap = REPO_ROOT / "docs" / "ROADMAP.md"
    if not roadmap.exists():
        print("[FAIL] ROADMAP.md not found; cannot validate adapter maturity.", file=sys.stderr)
        return 1

    levels = parse_roadmap_adapter_rows(roadmap)
    missing = set(KNOWN_ADAPTERS.values()) - set(levels)
    if missing:
        warnings.append(f"ROADMAP missing adapter rows for: {sorted(missing)}")

    for adapter in sorted(P4_SOURCE_ADAPTERS):
        level = levels.get(adapter)
        if level is None:
            warnings.append(f"Cannot verify P4 source for '{adapter}' (not in ROADMAP)")
            continue
        if level >= 4:
            crate_dir = REPO_ROOT / "crates" / f"ferrum-adapter-{adapter}"
            if directory_contains_phrase(crate_dir, "not implemented"):
                warnings.append(
                    f"P{level} adapter '{adapter}' source still contains 'not implemented' placeholder"
                )

    for adapter in sorted(P2_SHAPE_ONLY):
        level = levels.get(adapter)
        if level is None:
            warnings.append(f"Cannot verify P2 shape-only evidence for '{adapter}' (not in ROADMAP)")
            continue
        if level <= 2:
            crate_dir = REPO_ROOT / "crates" / f"ferrum-adapter-{adapter}"
            has_shape_only = directory_contains_phrase(crate_dir, "live: false") or \
                             directory_contains_phrase(crate_dir, "shape-only")
            has_live_seam = directory_contains_phrase(crate_dir, "not implemented")
            if not (has_shape_only or has_live_seam):
                warnings.append(
                    f"P{level} adapter '{adapter}' source lacks 'live: false' / 'shape-only' "
                    "or live-path 'not implemented' evidence; label may need review"
                )

    adapter_smoke = REPO_ROOT / "scripts" / "adapter_smoke.sh"
    if not adapter_smoke.exists():
        warnings.append("scripts/adapter_smoke.sh missing; shape-only S3/GCS + MCP evidence has no entry point")
    else:
        text = adapter_smoke.read_text(encoding="utf-8")
        if "ferrum-adapter-s3" not in text or "ferrum-adapter-gcs" not in text:
            warnings.append("scripts/adapter_smoke.sh does not mention S3/GCS adapters")

    if warnings:
        print("[INFO] Advisory adapter-maturity checks found warnings (non-blocking):")
        for warning in warnings:
            print(f"  [WARN] {warning}")
    else:
        print("[OK] Advisory adapter-maturity checks: no warnings.")

    # Always exit 0 unless the script itself cannot parse required docs/files.
    return 0


if __name__ == "__main__":
    sys.exit(main())
