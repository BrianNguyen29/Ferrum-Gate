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
#   python3 scripts/check_coverage_threshold.py <coverage.txt> [OPTIONS]
#
# Options:
#   --config PATH  Path to a TOML config file (default: coverage-thresholds.toml)
#   --hard         Exit non-zero if any threshold is missed (default: warn only)
#   --crate CRATE  Check a specific crate (overrides config to that crate)
#   --threshold PERCENT
#                  Override the threshold for the specified crate (requires --crate)
#
# Example:
#   cargo llvm-cov --workspace --text --output-path coverage.txt
#   python3 scripts/check_coverage_threshold.py coverage.txt
#   python3 scripts/check_coverage_threshold.py coverage.txt --hard
#   python3 scripts/check_coverage_threshold.py coverage.txt --crate ferrum-pdp --threshold 60.0

import argparse
import re
import sys
from pathlib import Path

# Critical crates and their aspirational thresholds.
# Used only when the TOML config cannot be loaded or no config is supplied.
DEFAULT_CRITICAL_CRATES = {
    "ferrum-pdp": 50.0,
    "ferrum-gateway": 45.0,
    "ferrum-store": 45.0,
    "ferrumd": 40.0,
}

DEFAULT_CONFIG_PATH = Path("coverage-thresholds.toml")


def load_config(path: Path | None) -> dict[str, float]:
    """Load crate thresholds from a TOML file.

    The file is expected to contain a ``[thresholds]`` table where each key is a
    crate name and each value is a coverage percentage. A value of 0.0 marks
    the crate as monitor-only (no warning/failure on low coverage). If the file
    is missing or cannot be parsed, fall back to the hard-coded defaults.
    """

    if path is None:
        return dict(DEFAULT_CRITICAL_CRATES)

    try:
        import tomllib
    except ImportError:  # pragma: no cover - Python <3.11 fallback
        print(
            "[WARN] tomllib unavailable (Python <3.11) and no external TOML parser "
            "configured; falling back to default thresholds.",
            file=sys.stderr,
        )
        return dict(DEFAULT_CRITICAL_CRATES)

    try:
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
    except FileNotFoundError:
        if path == DEFAULT_CONFIG_PATH:
            # Config not committed yet or not in cwd; keep going with defaults.
            print(
                f"[WARN] Config not found: {path}; using default thresholds.",
                file=sys.stderr,
            )
        else:
            print(f"[WARN] Config not found: {path}; using default thresholds.", file=sys.stderr)
        return dict(DEFAULT_CRITICAL_CRATES)
    except Exception as exc:
        print(f"[WARN] Could not parse config {path}: {exc}; using default thresholds.", file=sys.stderr)
        return dict(DEFAULT_CRITICAL_CRATES)

    thresholds = data.get("thresholds", {})
    if not isinstance(thresholds, dict):
        print(
            "[WARN] Config missing [thresholds] table; using default thresholds.",
            file=sys.stderr,
        )
        return dict(DEFAULT_CRITICAL_CRATES)

    result: dict[str, float] = {}
    for crate, value in thresholds.items():
        try:
            result[crate] = float(value)
        except (TypeError, ValueError):
            print(
                f"[WARN] Ignoring non-numeric threshold for '{crate}': {value!r}",
                file=sys.stderr,
            )

    return result


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
        "--config",
        type=Path,
        help="Path to TOML coverage thresholds config (default: coverage-thresholds.toml)",
    )
    parser.add_argument(
        "--hard", action="store_true", help="Exit non-zero on threshold miss"
    )
    parser.add_argument("--crate", help="Specific crate to check")
    parser.add_argument(
        "--threshold", type=float, help="Override threshold for the specified crate"
    )
    args = parser.parse_args()

    config_path = args.config
    if config_path is None:
        # Only use the default config file if it exists, otherwise fall back to
        # built-in defaults without printing a noisy warning. Explicit --config
        # still triggers a warning in load_config.
        config_path = DEFAULT_CONFIG_PATH if DEFAULT_CONFIG_PATH.exists() else None

    thresholds = load_config(config_path)

    if args.crate and args.threshold is not None:
        thresholds = {args.crate: args.threshold}
    elif args.crate:
        if args.crate not in thresholds:
            print(f"[WARN] Crate {args.crate} not in config; no threshold configured.")
            thresholds = {}
        else:
            thresholds = {args.crate: thresholds[args.crate]}

    coverage = parse_coverage_text(args.coverage_file)

    total_warnings = 0
    total_passes = 0
    monitor_only = {crate for crate, threshold in thresholds.items() if threshold <= 0.0}

    for crate, threshold in thresholds.items():
        actual = coverage.get(crate)
        if threshold <= 0.0:
            # Monitor-only: report observed coverage if present but never warn/fail.
            if actual is not None:
                print(f"[INFO] {crate}: {actual:.2f}% (monitor-only, no threshold)")
            continue

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

    if monitor_only:
        print(f"[INFO] {len(monitor_only)} crate(s) configured as monitor-only (threshold 0.0).")

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
