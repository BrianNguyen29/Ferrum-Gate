#!/usr/bin/env python3
"""Validate README CI/badge sanity without external network calls."""

import os
import re
import sys

README = "README.md"


def main():
    if not os.path.exists(README):
        print(f"[MISSING] {README} not found")
        sys.exit(1)

    with open(README, "r", encoding="utf-8") as f:
        lines = f.readlines()

    ci_badge_found = False
    license_badge_found = False
    badge_count = 0
    errors = []

    for line in lines:
        # Count markdown badges: ![alt](url) or [![alt](url)](link)
        if re.search(r"!?\[.*\]\(.*\)", line):
            badge_count += 1

        if "actions/workflows/ci.yml/badge.svg" in line:
            ci_badge_found = True
            if "actions/workflows/ci.yml" in line:
                print("[OK] CI workflow badge found")
            else:
                errors.append(
                    "[WARN] CI badge image found but link may not point to workflow"
                )

        if "License" in line and "./LICENSE" in line:
            license_badge_found = True
            if os.path.exists("LICENSE"):
                print("[OK] License badge target exists")
            else:
                errors.append("[MISSING] LICENSE file does not exist")

    if badge_count < 2:
        errors.append(f"[WARN] Expected at least 2 badges, found {badge_count}")

    if not ci_badge_found:
        errors.append("[MISSING] CI badge not found in README")

    if not license_badge_found:
        errors.append("[WARN] License badge not found in README")

    if errors:
        print("\n".join(errors))
        sys.exit(1)

    print("[OK] Badge validation passed")


if __name__ == "__main__":
    main()
