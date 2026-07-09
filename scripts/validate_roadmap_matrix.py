#!/usr/bin/env python3
"""validate_roadmap_matrix.py

Advisory ROADMAP consistency check.

The ROADMAP maturity matrix assigns P levels to subsystems. A subsystem that is
labeled P3 or higher should generally not be described only with experimental/advisory
phrases such as "advisory", "in-memory", "does not change PDP decisions", or "opt-in".
This script warns when such wording appears in P3+ rows, because it may indicate a
label/evidence mismatch. Current P2 (and lower) rows are expected to contain such
language and are not flagged.

This validator is advisory: it prints warnings but exits 0 unless ROADMAP cannot be
read/parsed.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ROADMAP_PATH = REPO_ROOT / "docs" / "ROADMAP.md"

# Phrases that suggest a subsystem is still experimental/advisory only.
ADVISORY_PHRASES = [
    "advisory",
    "in-memory",
    "does not change PDP decisions",
    "opt-in",
    "not implemented",
]


@dataclass
class RoadmapRow:
    name: str
    level: int
    description: str


def parse_roadmap_matrix(path: Path) -> list[RoadmapRow]:
    """Parse the ROADMAP.md feature matrix into rows with name, level, and description."""
    content = path.read_text(encoding="utf-8")
    rows: list[RoadmapRow] = []
    for line in content.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        parts = [p.strip() for p in line.split("|")]
        # Filter out header separator rows like |---|---|---|---|
        if any("---" in p for p in parts):
            continue
        # Expected: empty leading, name, level, status, description, empty trailing
        if len(parts) < 5:
            continue
        name = parts[1]
        level_text = parts[2]
        description = parts[4]
        if not name or not level_text:
            continue
        m = re.match(r"P(\d+)", level_text)
        if not m:
            continue
        level = int(m.group(1))
        rows.append(RoadmapRow(name=name, level=level, description=description))
    return rows


def main() -> int:
    if not ROADMAP_PATH.exists():
        print(f"[FAIL] {ROADMAP_PATH} not found; cannot validate roadmap matrix.", file=sys.stderr)
        return 1

    try:
        rows = parse_roadmap_matrix(ROADMAP_PATH)
    except Exception as exc:
        print(f"[FAIL] Could not parse ROADMAP matrix: {exc}", file=sys.stderr)
        return 1

    warnings = []
    for row in rows:
        if row.level < 3:
            continue
        desc_lower = row.description.lower()
        matched = [p for p in ADVISORY_PHRASES if p.lower() in desc_lower]
        if matched:
            warnings.append(
                f"P{row.level} row '{row.name}' contains advisory-only wording: {matched}"
            )

    if warnings:
        print("[INFO] Advisory roadmap matrix warnings (non-blocking):")
        for warning in warnings:
            print(f"  [WARN] {warning}")
    else:
        print("[OK] No P3+ ROADMAP rows contain advisory-only wording.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
