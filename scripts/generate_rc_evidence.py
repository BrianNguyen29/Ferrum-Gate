#!/usr/bin/env python3
"""
Generate RC evidence for Ferrum Gate v1 release candidate.

This script runs the verification commands and produces an honest pass/fail report
for each evidence item. It is the automation of the manual evidence record in
docs/implementation-path/25-v1-single-node-rc-evidence.md.

Usage: python3 scripts/generate_rc_evidence.py
"""

import subprocess
import sys
import os

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run_command(cmd, description, timeout=300):
    """Run a command and return (success, output)."""
    print(f"\n{'=' * 60}")
    print(f"Running: {description}")
    print(f"Command: {cmd}")
    print(f"{'=' * 60}")
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        success = result.returncode == 0
        output = result.stdout + result.stderr
        print(f"Result: {'PASS' if success else 'FAIL'}")
        if not success:
            print(f"Output (last 2000 chars):\n{output[-2000:]}")
        return success, output
    except subprocess.TimeoutExpired:
        print(f"Result: TIMEOUT (>{timeout}s)")
        return False, f"Command timed out after {timeout}s"
    except Exception as e:
        print(f"Result: ERROR - {e}")
        return False, str(e)


def main():
    print("Ferrum Gate v1 RC Evidence Generator")
    print("=" * 60)

    results = {}

    # Evidence 1: Workspace compiles
    success, output = run_command(
        "cargo check --workspace",
        "cargo check --workspace",
    )
    results["cargo_check"] = success

    success, output = run_command(
        "cargo fmt --all --check",
        "cargo fmt --all --check",
    )
    results["cargo_fmt"] = success

    success, output = run_command(
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo clippy --workspace --all-targets -- -D warnings",
    )
    results["cargo_clippy"] = success

    success, output = run_command(
        "cargo test --workspace",
        "cargo test --workspace (all tests)",
        timeout=600,
    )
    results["cargo_test"] = success

    # Contract consistency check
    success, output = run_command(
        "python3 scripts/check_contract_consistency.py",
        "python3 scripts/check_contract_consistency.py",
    )
    results["contract_consistency"] = success

    # Summary
    print(f"\n{'=' * 60}")
    print("EVIDENCE SUMMARY")
    print(f"{'=' * 60}")
    for item, passed in results.items():
        status = "PASS" if passed else "FAIL"
        print(f"  {item}: {status}")
    print(f"{'=' * 60}")

    all_passed = all(results.values())
    print(f"\nOverall: {'ALL PASS' if all_passed else 'SOME FAILURES'}")
    print(f"Exit code: {0 if all_passed else 1}")

    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
