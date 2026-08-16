#!/usr/bin/env python3
"""validate_runbook_adr_status.py

Advisory/structural validator for the ADR status table in docs/operations/runbook.md.

It parses runbook rows like:

    | Control | [ADR 009](../adr/009-...md) | Accepted (P2-1) |

extracts the ADR number, locates the corresponding `docs/adr/<num>-*.md` file, reads
its `## Status` line, and checks that the leading keyword of the runbook status
(e.g., "Accepted", "Proposed") is present at the start of the ADR doc status.

Exits non-zero only on:
- a referenced ADR doc that does not exist or has no `## Status` line;
- a genuine status-keyword mismatch between the runbook table and the ADR doc.

Exits 0 when all referenced ADRs exist and their status keywords align.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUNBOOK_PATH = REPO_ROOT / "docs" / "operations" / "runbook.md"
ADR_DIR = REPO_ROOT / "docs" / "adr"

ADR_REF_RE = re.compile(r"\[ADR (\d{3})\]\([^)]+\)")
ROW_RE = re.compile(r"^\|\s*[^|]+\|\s*\[ADR \d{3}\][^|]*\|\s*([^|]+)\|")


@dataclass
class AdrRow:
    number: str
    runbook_status: str


def parse_runbook_adr_rows(path: Path) -> list[AdrRow]:
    """Parse ADR rows from the runbook status table."""
    rows: list[AdrRow] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        adr_match = ADR_REF_RE.search(line)
        row_match = ROW_RE.match(line)
        if not adr_match or not row_match:
            continue
        number = adr_match.group(1)
        status = row_match.group(1).strip()
        rows.append(AdrRow(number=number, runbook_status=status))
    return rows


def find_adr_doc(number: str) -> Path | None:
    """Return the path to docs/adr/<number>-*.md if present."""
    candidates = sorted(ADR_DIR.glob(f"{number}-*.md"))
    return candidates[0] if candidates else None


def read_adr_doc_status(path: Path) -> str | None:
    """Return the first non-empty line after '## Status' in the ADR doc."""
    lines = path.read_text(encoding="utf-8").splitlines()
    in_status = False
    for line in lines:
        if line.strip() == "## Status":
            in_status = True
            continue
        if in_status and line.strip():
            return line.strip()
    return None


def main() -> int:
    if not RUNBOOK_PATH.exists():
        print(f"[FAIL] {RUNBOOK_PATH} not found; cannot validate ADR status.", file=sys.stderr)
        return 1

    try:
        rows = parse_runbook_adr_rows(RUNBOOK_PATH)
    except Exception as exc:
        print(f"[FAIL] Could not parse runbook ADR rows: {exc}", file=sys.stderr)
        return 1

    errors: list[str] = []
    for row in rows:
        doc = find_adr_doc(row.number)
        if doc is None:
            errors.append(f"ADR {row.number}: referenced in runbook but no matching docs/adr/{row.number}-*.md")
            continue
        doc_status = read_adr_doc_status(doc)
        if doc_status is None:
            errors.append(f"ADR {row.number}: {doc} has no '## Status' line")
            continue
        # Compare the leading keyword of the runbook status to the doc status.
        runbook_keyword = row.runbook_status.split()[0].lower() if row.runbook_status else ""
        doc_keyword = doc_status.split()[0].lower().rstrip(".,") if doc_status else ""
        if runbook_keyword and runbook_keyword != doc_keyword:
            errors.append(
                f"ADR {row.number}: runbook status '{row.runbook_status}' does not match "
                f"ADR doc status '{doc_status}'"
            )

    if errors:
        print("[FAIL] ADR status drift detected:")
        for error in errors:
            print(f"  [ERR] {error}")
        return 1

    print(f"[OK] All {len(rows)} runbook ADR status entries match their ADR docs.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
