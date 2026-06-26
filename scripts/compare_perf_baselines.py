#!/usr/bin/env python3
"""compare_perf_baselines.py — Compare ferrum-stress JSON output against baselines.

Usage:
    python3 scripts/compare_perf_baselines.py \
        --stress-json /tmp/ferrum-stress.json \
        --baselines-dir baselines/ \
        [--relative-threshold 0.20] \
        [--dry-run]

Exit codes:
    0 — all checked scenarios pass (or dry-run)
    1 — one or more thresholds exceeded and not in dry-run

Baseline JSON format (example):
    {
      "format_version": "1.0",
      "scenario": "health",
      "concurrency": 50,
      "duration_secs": 5,
      "metrics": {
        "req_per_sec": { "baseline": 30000.0, "unit": "req/s", "min_ratio": 0.80 },
        "p95_ms":      { "baseline": 5.0,     "unit": "ms",   "max_ratio": 1.50 },
        "p99_ms":      { "baseline": 10.0,    "unit": "ms",   "max_ratio": 2.00 },
        "error_rate":  { "baseline": 0.0,     "unit": "ratio","max_absolute": 0.01 }
      },
      "meta": {
        "last_validated_commit": "sample",
        "validated_at": "2026-06-25T00:00:00Z",
        "note": "SAMPLE / NON-AUTHORITATIVE"
      }
    }

Rules:
    - req_per_sec: actual >= baseline * min_ratio
    - p95_ms, p99_ms: actual <= baseline * max_ratio
    - error_rate: actual <= max_absolute (if defined) or baseline * max_ratio
"""

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any


def load_baselines(baselines_dir: str) -> dict[str, dict]:
    """Load all baseline JSON files keyed by scenario name."""
    baselines: dict[str, dict] = {}
    dir_path = Path(baselines_dir)
    if not dir_path.is_dir():
        print(f"[WARN] Baselines directory not found: {baselines_dir}")
        return baselines

    for file_path in sorted(dir_path.glob("*.json")):
        try:
            with file_path.open(encoding="utf-8") as fh:
                data = json.load(fh)
            scenario = data.get("scenario")
            if not scenario:
                print(f"[WARN] Baseline file missing 'scenario': {file_path}")
                continue
            baselines[scenario] = data
        except json.JSONDecodeError as exc:
            print(f"[WARN] Invalid JSON in baseline {file_path}: {exc}")
        except Exception as exc:
            print(f"[WARN] Failed to read baseline {file_path}: {exc}")
    return baselines


def compare_metric(
    scenario: str,
    metric_name: str,
    actual: float,
    spec: dict[str, Any],
) -> tuple[bool, str]:
    """Compare a single metric against its baseline spec. Returns (pass, message)."""
    baseline = spec.get("baseline")
    if baseline is None:
        return True, f"  {metric_name}: no baseline (skipped)"

    unit = spec.get("unit", "")
    min_ratio = spec.get("min_ratio")
    max_ratio = spec.get("max_ratio")
    max_absolute = spec.get("max_absolute")

    # Latency metrics: lower is better; use max_ratio
    if max_ratio is not None and metric_name in ("p95_ms", "p99_ms", "p50_ms", "mean_ms"):
        threshold = baseline * max_ratio
        passed = actual <= threshold
        status = "PASS" if passed else "FAIL"
        return passed, (
            f"  {metric_name}: {actual:.3f} {unit} (baseline={baseline:.3f}, "
            f"max={threshold:.3f}) [{status}]"
        )

    # Throughput metrics: higher is better; use min_ratio
    if min_ratio is not None and metric_name == "req_per_sec":
        threshold = baseline * min_ratio
        passed = actual >= threshold
        status = "PASS" if passed else "FAIL"
        return passed, (
            f"  {metric_name}: {actual:.1f} {unit} (baseline={baseline:.1f}, "
            f"min={threshold:.1f}) [{status}]"
        )

    # Error rate: use max_absolute if present, otherwise max_ratio
    if metric_name == "error_rate":
        if max_absolute is not None:
            passed = actual <= max_absolute
            status = "PASS" if passed else "FAIL"
            return passed, (
                f"  {metric_name}: {actual:.4f} (max_absolute={max_absolute:.4f}) [{status}]"
            )
        if max_ratio is not None:
            threshold = baseline * max_ratio
            passed = actual <= threshold
            status = "PASS" if passed else "FAIL"
            return passed, (
                f"  {metric_name}: {actual:.4f} (baseline={baseline:.4f}, "
                f"max={threshold:.4f}) [{status}]"
            )

    # Generic fallback: exact match with 10% tolerance
    diff = abs(actual - baseline) / baseline if baseline != 0 else abs(actual)
    passed = diff <= 0.10
    status = "PASS" if passed else "FAIL"
    return passed, (
        f"  {metric_name}: {actual:.3f} {unit} (baseline={baseline:.3f}, "
        f"diff={diff:.2%}) [{status}]"
    )


def compare_scenario(
    scenario_result: dict[str, Any],
    baseline: dict[str, Any],
    relative_threshold: float,
) -> tuple[bool, list[str]]:
    """Compare a single scenario result against its baseline."""
    messages: list[str] = []
    passed = True

    scenario_name = scenario_result.get("scenario", "unknown")
    metrics_spec = baseline.get("metrics", {})

    messages.append(f"Scenario: {scenario_name}")
    messages.append(
        f"  config: concurrency={scenario_result.get('concurrency')}, "
        f"duration={scenario_result.get('duration_secs')}s"
    )
    note = baseline.get("meta", {}).get("note", "")
    if note:
        messages.append(f"  note: {note}")

    # Map result keys to metric specs
    for metric_name, spec in metrics_spec.items():
        actual = scenario_result.get(metric_name)
        if actual is None:
            messages.append(f"  {metric_name}: missing from stress output (skipped)")
            continue

        metric_passed, msg = compare_metric(scenario_name, metric_name, actual, spec)
        messages.append(msg)
        if not metric_passed:
            passed = False

    return passed, messages


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare ferrum-stress JSON output against baselines."
    )
    parser.add_argument(
        "--stress-json",
        required=True,
        help="Path to ferrum-stress JSON output file.",
    )
    parser.add_argument(
        "--baselines-dir",
        default="baselines",
        help="Directory containing baseline JSON files (default: baselines).",
    )
    parser.add_argument(
        "--relative-threshold",
        type=float,
        default=0.20,
        help="Default relative threshold for metrics without explicit ratios (default: 0.20).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print comparison but always exit 0 (advisory mode).",
    )
    args = parser.parse_args()

    # Load stress output
    stress_path = Path(args.stress_json)
    if not stress_path.is_file():
        print(f"[ERROR] Stress JSON file not found: {args.stress_json}")
        return 0 if args.dry_run else 1

    try:
        with stress_path.open(encoding="utf-8") as fh:
            stress_data = json.load(fh)
    except json.JSONDecodeError as exc:
        print(f"[ERROR] Invalid stress JSON: {exc}")
        return 0 if args.dry_run else 1

    scenarios = stress_data.get("scenarios", [])
    if not scenarios:
        print("[WARN] No scenarios found in stress output.")
        return 0 if args.dry_run else 1

    # Load baselines
    baselines = load_baselines(args.baselines_dir)
    if not baselines:
        print("[WARN] No baselines loaded. Comparison skipped.")
        return 0 if args.dry_run else 1

    # Compare each scenario
    all_passed = True
    print("═══════════════════════════════════════════════════════════════")
    print("  PERFORMANCE REGRESSION GATE")
    print("═══════════════════════════════════════════════════════════════")
    print()

    for scenario in scenarios:
        scenario_name = scenario.get("scenario", "unknown")
        baseline = baselines.get(scenario_name)
        if not baseline:
            print(f"[SKIP] No baseline for scenario '{scenario_name}'")
            print()
            continue

        passed, messages = compare_scenario(scenario, baseline, args.relative_threshold)
        for msg in messages:
            print(msg)
        print()
        if not passed:
            all_passed = False

    # Summary
    print("───────────────────────────────────────────────────────────────")
    if all_passed:
        print("[PASS] All scenarios within baseline thresholds.")
    else:
        if args.dry_run:
            print("[ADVISORY] Thresholds exceeded, but dry-run mode prevents failure.")
        else:
            print("[FAIL] One or more scenarios exceeded baseline thresholds.")
    print("───────────────────────────────────────────────────────────────")

    if args.dry_run:
        return 0
    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
