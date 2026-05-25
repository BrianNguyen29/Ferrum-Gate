#!/usr/bin/env python3
"""
validate_evidence_templates.py

Scans docs/implementation-path/artifacts/TEMPLATE-*.md and verifies baseline
required sections/phrases appropriate for existing evidence templates.

Rules are pragmatic and trace to the actual template content.
Does not force unrelated template rewrites.
"""

import glob
import os
import sys

ARTIFACTS_DIR = "docs/implementation-path/artifacts"
TEMPLATE_PATTERN = os.path.join(ARTIFACTS_DIR, "TEMPLATE-*.md")

# Baseline required sections present in every valid template
REQUIRED_SECTIONS = [
    "## Metadata",
    "## Signoff",
    "## Non-Claims",
    "## Related Docs",
]

# Required phrase (case-insensitive substring)
REQUIRED_PHRASE = "THIS IS A TEMPLATE"


def validate_template(path: str) -> list[str]:
    """Return a list of validation errors for a single template file."""
    errors = []
    basename = os.path.basename(path)

    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    # Rule 1: Title must start with "# TEMPLATE —"
    lines = content.splitlines()
    first_line = lines[0] if lines else ""
    if not first_line.startswith("# TEMPLATE"):
        errors.append(f"{basename}: first line does not start with '# TEMPLATE'")

    # Rule 2: Must contain the template warning
    if REQUIRED_PHRASE not in content:
        errors.append(f"{basename}: missing required phrase '{REQUIRED_PHRASE}'")

    # Rule 3: Must contain all required sections
    for section in REQUIRED_SECTIONS:
        if section not in content:
            errors.append(f"{basename}: missing required section '{section}'")

    return errors


def main() -> int:
    template_paths = sorted(glob.glob(TEMPLATE_PATTERN))

    if not template_paths:
        print(f"[ERROR] No templates found matching {TEMPLATE_PATTERN}")
        return 1

    all_errors = []
    passed = 0
    failed = 0

    for path in template_paths:
        basename = os.path.basename(path)
        errors = validate_template(path)
        if errors:
            failed += 1
            all_errors.extend(errors)
            for err in errors:
                print(f"[FAIL] {err}")
        else:
            passed += 1
            print(f"[PASS] {basename}")

    print("")
    print("========================================")
    print("Evidence Template Validation Summary")
    print("========================================")
    print(f"Passed:  {passed}")
    print(f"Failed:  {failed}")
    print(f"Total:   {passed + failed}")

    if all_errors:
        print("")
        print("Errors:")
        for err in all_errors:
            print(f"  - {err}")
        return 1

    print("")
    print("All evidence templates passed validation.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
