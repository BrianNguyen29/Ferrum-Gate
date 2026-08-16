#!/usr/bin/env python3
"""validate_adr_catalog.py

Blocking validator for the ADR catalog.

Checks:
1. docs/adr/NNN-*.md files are canonical ADR documents with parseable H1 and status.
2. docs/adr/README.md index table inventories exactly those ADRs, with correct
   canonical relative link targets, titles, and status-leading-keyword.
3. docs/ROADMAP.md references only existing ADRs and uses canonical ADR links.
4. Noncanonical mentions (e.g. ``ADR-like`` or ``ADR 19``) are ignored.

Exits non-zero on any drift. A zero-row index table is treated as a failure.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ADR_DIR = REPO_ROOT / "docs" / "adr"
README_PATH = REPO_ROOT / "docs" / "adr" / "README.md"
ROADMAP_PATH = REPO_ROOT / "docs" / "ROADMAP.md"

# Canonical 3-digit ADR filename: NNN-title.md.
ADR_FILE_RE = re.compile(r"^(\d{3})-(.+)\.md$")

# H1 variants:
#   # ADR 000 — Title
#   # ADR-014: Title
#   > # ADR-015: Title
H1_RE = re.compile(r"^>?\s*#\s*ADR\s*[\-\s](\d{3})\s*[:\u2014]\s*(.+)$")

# Inline status line (used by ADR-015 in blockquote):
#   > **Status:** Accepted
INLINE_STATUS_RE = re.compile(r"^(?:\s*>\s*)?\*\*Status:\*\*\s*(.+)$", re.IGNORECASE)

README_ROW_RE = re.compile(
    r"^\|\s*\[(\d{3})\]\s*\(([^)]+)\)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$"
)


@dataclass
class AdrDoc:
    number: str
    title: str
    status: str
    status_keyword: str
    path: Path


def _keyword(status: str) -> str:
    """Return the leading status keyword, lowercased and stripped of punctuation."""
    if not status:
        return ""
    return status.split()[0].lower().rstrip(".,;:")


def parse_adr_doc(path: Path) -> AdrDoc:
    """Parse a canonical ADR document into AdrDoc."""
    filename = path.name
    m = ADR_FILE_RE.match(filename)
    if not m:
        raise ValueError(f"{path} is not a canonical ADR filename (NNN-title.md)")

    number = m.group(1)
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    title: str | None = None
    status: str | None = None

    in_status_section = False
    for i, line in enumerate(lines):
        h1 = H1_RE.match(line)
        if h1:
            h1_number = h1.group(1)
            title = h1.group(2).strip()
            if h1_number != number:
                raise ValueError(
                    f"ADR {number}: H1 number {h1_number} does not match filename"
                )
            continue

        inline = INLINE_STATUS_RE.match(line)
        if inline:
            status = inline.group(1).strip()
            continue

        heading_status = re.match(r"^\s*##\s*Status\s*(?::\s*(.+))?$", line)
        if heading_status:
            if heading_status.group(1):
                status = heading_status.group(1).strip()
            else:
                in_status_section = True
            continue

        if in_status_section:
            stripped = line.strip()
            if stripped:
                if stripped.startswith("##"):
                    break
                status = stripped
                break

    if title is None:
        raise ValueError(f"ADR {number}: missing H1 title in {path}")
    if status is None:
        raise ValueError(f"ADR {number}: missing '## Status' or '**Status:**' in {path}")

    return AdrDoc(
        number=number,
        title=title,
        status=status,
        status_keyword=_keyword(status),
        path=path,
    )


def discover_adr_docs(adr_dir: Path) -> dict[str, AdrDoc]:
    """Discover canonical ADR docs in the given directory."""
    docs: dict[str, AdrDoc] = {}
    for path in sorted(adr_dir.glob("*.md")):
        if not ADR_FILE_RE.match(path.name):
            continue
        doc = parse_adr_doc(path)
        if doc.number in docs:
            raise ValueError(
                f"Duplicate ADR number {doc.number}: {docs[doc.number].path} and {doc.path}"
            )
        docs[doc.number] = doc
    return docs


@dataclass
class AdrIndexRow:
    number: str
    target: str
    title: str
    status: str


def parse_readme_index(readme_path: Path) -> list[AdrIndexRow]:
    """Parse ADR rows from the README index table."""
    rows: list[AdrIndexRow] = []
    for line in readme_path.read_text(encoding="utf-8").splitlines():
        m = README_ROW_RE.match(line)
        if not m:
            continue
        rows.append(
            AdrIndexRow(
                number=m.group(1),
                target=m.group(2).strip(),
                title=m.group(3).strip(),
                status=m.group(4).strip(),
            )
        )
    return rows


def validate_readme_index(
    readme_path: Path, adr_dir: Path, docs: dict[str, AdrDoc]
) -> list[str]:
    """Validate the README index against the canonical ADR documents."""
    rows = parse_readme_index(readme_path)
    if not rows:
        return ["README index table has no ADR rows"]

    errors: list[str] = []
    seen_numbers: set[str] = set()

    for row in rows:
        if row.number in seen_numbers:
            errors.append(f"README index has duplicate row for ADR {row.number}")
            continue
        seen_numbers.add(row.number)

        doc = docs.get(row.number)
        if doc is None:
            errors.append(
                f"README index references unknown ADR {row.number} ({row.target})"
            )
            continue

        target_basename = Path(row.target).name
        if not ADR_FILE_RE.match(target_basename):
            errors.append(
                f"ADR {row.number}: link target {row.target} is not canonical (NNN-title.md)"
            )
            continue

        if ".." in row.target or row.target.startswith(("/", "\\")):
            errors.append(
                f"ADR {row.number}: link target {row.target} is not canonical relative path"
            )
            continue

        target_path = (readme_path.parent / row.target).resolve()
        canonical_path = (adr_dir / doc.path.name).resolve()
        if target_path != canonical_path:
            errors.append(
                f"ADR {row.number}: link target {row.target} resolves to {target_path}, "
                f"expected canonical path {canonical_path}"
            )
            continue

        if row.title != doc.title:
            errors.append(
                f"ADR {row.number}: README title '{row.title}' does not match "
                f"ADR title '{doc.title}'"
            )

        row_keyword = _keyword(row.status)
        if row_keyword != doc.status_keyword:
            errors.append(
                f"ADR {row.number}: README status '{row.status}' leading keyword "
                f"'{row_keyword}' does not match ADR status '{doc.status}' "
                f"keyword '{doc.status_keyword}'"
            )

    for number in sorted(docs):
        if number not in seen_numbers:
            errors.append(f"ADR {number} ({docs[number].path.name}) is missing from README index")

    return errors


def _line_number(text: str, pos: int) -> int:
    return text.count("\n", 0, pos) + 1


def validate_roadmap(
    roadmap_path: Path, adr_dir: Path, docs: dict[str, AdrDoc]
) -> list[str]:
    """Validate ADR references in the roadmap."""
    text = roadmap_path.read_text(encoding="utf-8")
    errors: list[str] = []
    seen: set[str] = set()

    # Plain mentions: ADR NNN or ADR-NNN (exactly 3 digits, word boundary).
    for pattern in (r"ADR\s+(\d{3})\b", r"ADR-(\d{3})\b"):
        for m in re.finditer(pattern, text):
            number = m.group(1)
            if number not in docs:
                line = _line_number(text, m.start())
                errors.append(f"ROADMAP line {line}: references unknown ADR {number}")
            seen.add(number)

    # Markdown links: [ADR NNN](target) or [ADR-NNN](target)
    for m in re.finditer(r"\[ADR\s*[\-\s](\d{3})\]\(([^)]+)\)", text):
        number = m.group(1)
        target = m.group(2).strip()
        target_basename = Path(target).name
        line = _line_number(text, m.start())

        if not ADR_FILE_RE.match(target_basename):
            errors.append(
                f"ROADMAP line {line}: ADR {number} link target {target} is not canonical"
            )
            continue

        if ".." in target or target.startswith(("/", "\\")):
            errors.append(
                f"ROADMAP line {line}: ADR {number} link target {target} is not canonical relative path"
            )
            continue

        target_path = (roadmap_path.parent / target).resolve()
        canonical_path = (adr_dir / target_basename).resolve()
        if target_path != canonical_path:
            errors.append(
                f"ROADMAP line {line}: ADR {number} link target {target} resolves to {target_path}, "
                f"expected canonical path {canonical_path}"
            )
            continue

        doc = docs.get(number)
        if doc is None:
            errors.append(f"ROADMAP line {line}: ADR {number} link to {target} references unknown ADR")
        elif doc.path.name != target_basename:
            errors.append(
                f"ROADMAP line {line}: ADR {number} link target {target_basename} "
                f"does not match canonical doc {doc.path.name}"
            )

    return errors


def main() -> int:
    if not README_PATH.exists():
        print(f"[FAIL] {README_PATH} not found", file=sys.stderr)
        return 1
    if not ROADMAP_PATH.exists():
        print(f"[FAIL] {ROADMAP_PATH} not found", file=sys.stderr)
        return 1

    errors: list[str] = []
    try:
        docs = discover_adr_docs(ADR_DIR)
    except Exception as exc:
        print(f"[FAIL] Could not discover ADR documents: {exc}", file=sys.stderr)
        return 1

    errors.extend(validate_readme_index(README_PATH, ADR_DIR, docs))
    errors.extend(validate_roadmap(ROADMAP_PATH, ADR_DIR, docs))

    if errors:
        print("[FAIL] ADR catalog drift detected:")
        for err in errors:
            print(f"  [ERR] {err}")
        return 1

    print(
        f"[OK] ADR catalog valid: {len(docs)} ADR docs, "
        "README index complete, ROADMAP references consistent."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
