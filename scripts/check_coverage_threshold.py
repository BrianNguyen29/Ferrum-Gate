#!/usr/bin/env python3
# scripts/check_coverage_threshold.py
# Critical-crate coverage threshold checker.
#
# This script parses cargo-llvm-cov text output and checks per-crate coverage
# against a configured threshold. It is designed to be used as a pre-flight
# check, but is NOT wired as a blocking CI gate until there is sufficient
# evidence that the thresholds are stable.
#
# Usage:
#   python3 scripts/check_coverage_threshold.py <coverage.txt> [--hard] [--crate CRATE] [--threshold PERCENT]
#
#   --hard        Exit non-zero if any threshold is missed (default: warn only)
#   --crate       Check a specific crate (default: all configured critical crates)
#   --threshold   Override the default threshold for the specified crate
#
# Example:
#   cargo llvm-cov --workspace --text --output-path coverage.txt
#   python3 scripts/check_coverage_threshold.py coverage.txt
#   python3 scripts/check_coverage_threshold.py coverage.txt --hard --crate ferrum-pdp --threshold 60.0

import argparse
import re
import sys

# Critical crates and their aspirational thresholds.
# These are conservative starting points. Adjust based on evidence.
DEFAULT_CRITICAL_CRATES = {
    "ferrum-pdp": 50.0,
    "ferrum-gateway": 45.0,
    "ferrum-store": 45.0,
    "ferrumd": 40.0,
}


def parse_coverage_text(path: str) -> dict[str, float]:
    """Parse cargo-llvm-cov text output and return a map of crate -> coverage %."""
    coverage = {}
    with open(path, encoding="utf-8") as fh:
        content = fh.read()

    # The cargo-llvm-cov text output lists files with coverage.
    # Crate names appear in the path like "crates/ferrum-pdp/src/...".
    # We also look for the TOTAL line at the end.
    #
    # Strategy: group lines by crate prefix and average the coverage.
    # This is a heuristic because cargo-llvm-cov does not always emit per-crate
    # summaries in text mode. For more accurate per-crate numbers, use
    #   cargo llvm-cov --workspace --text --output-path coverage.txt --lcov
    # and parse the LCOV, or run per-crate coverage jobs.
    #
    # Here we do a best-effort parse of the file-level coverage table.

    crate_lines = {}
    for line in content.splitlines():
        # Match lines like: crates/ferrum-pdp/src/engine.rs ... 85.20% ...
        m = re.match(r"^\s*(crates/([^/]+)/.*?)\s+.*\s+(\d+\.\d+)%", line)
        if m:
            crate = m.group(2)
            pct = float(m.group(3))
            crate_lines.setdefault(crate, []).append(pct)

    for crate, values in crate_lines.items():
        if values:
            coverage[crate] = sum(values) / len(values)

    # Also try to extract the TOTAL workspace coverage
    total_match = re.search(r"TOTAL\s+.*?([\d.]+)%", content, re.MULTILINE)
    if total_match:
        coverage["TOTAL"] = float(total_match.group(1))
    else:
        # Fallback: last percentage in the file
        all_matches = re.findall(r"([\d.]+)%", content)
        if all_matches:
            coverage["TOTAL"] = float(all_matches[-1])

    return coverage


def main():
    parser = argparse.ArgumentParser(
        description="Check coverage thresholds for critical crates."
    )
    parser.add_argument("coverage_file", help="Path to cargo-llvm-cov text output")
    parser.add_argument(
        "--hard", action="store_true", help="Exit non-zero on threshold miss"
    )
    parser.add_argument("--crate", help="Specific crate to check")
    parser.add_argument(
        "--threshold", type=float, help="Override threshold for the specified crate"
    )
    args = parser.parse_args()

    coverage = parse_coverage_text(args.coverage_file)

    thresholds = dict(DEFAULT_CRITICAL_CRATES)
    if args.crate and args.threshold is not None:
        thresholds = {args.crate: args.threshold}
    elif args.crate:
        if args.crate not in thresholds:
            print(f"[WARN] Crate {args.crate} not in default critical list; no threshold configured.")
            thresholds = {}
        else:
            thresholds = {args.crate: thresholds[args.crate]}

    total_warnings = 0
    total_passes = 0

    for crate, threshold in thresholds.items():
        actual = coverage.get(crate)
        if actual is None:
            print(f"[WARN] No coverage data found for crate '{crate}'")
            total_warnings += 1
            continue
        if actual >= threshold:
            print(f"[PASS] {crate}: {actual:.2f}% >= {threshold:.2f}%")
            total_passes += 1
        else:
            if args.hard:
                print(f"[FAIL] {crate}: {actual:.2f}% < {threshold:.2f}% (hard mode)")
            else:
                print(f"[WARN] {crate}: {actual:.2f}% < {threshold:.2f}% (soft mode)")
            total_warnings += 1

    total = coverage.get("TOTAL")
    if total is not None:
        print(f"[INFO] Workspace TOTAL: {total:.2f}%")
    else:
        print("[WARN] Could not parse TOTAL coverage")

    print("")
    if total_warnings == 0:
        print("[OK] All checked crates meet their coverage thresholds.")
        sys.exit(0)
    else:
        if args.hard:
            print(f"[FAIL] {total_warnings} threshold(s) missed (hard mode).")
            sys.exit(1)
        else:
            print(f"[WARN] {total_warnings} threshold(s) missed (soft mode). Not failing CI.")
            sys.exit(0)


if __name__ == "__main__":
    main()
