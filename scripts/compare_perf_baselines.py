#!/usr/bin/env python3
"""compare_perf_baselines.py — Compare ferrum-stress JSON output against baselines.

Usage:
    python3 scripts/compare_perf_baselines.py \
        --stress-json /tmp/ferrum-stress.json \
        --baselines-dir baselines/ \
        [--relative-threshold 0.20] \
        [--dry-run] \
        [--enforce]

Exit codes:
    0 — all checked scenarios pass (or dry-run, or advisory mode)
    1 — one or more thresholds exceeded or enforcement preconditions failed

Baselines are accepted as authoritative only when `meta.authoritative` is true
and the baseline contains required validation metadata. Non-authoritative or
sample baselines are rejected in enforced mode. In dry-run/advisory mode they
continue to warn/skip.

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
        "authoritative": true,
        "last_validated_commit": "abc123",
        "validated_at": "2026-06-25T00:00:00Z",
        "note": "Authoritative baseline"
      }
    }

Rules:
    - req_per_sec: actual >= baseline * min_ratio
    - p95_ms, p99_ms: actual <= baseline * max_ratio
    - error_rate: actual <= max_absolute (if defined) or baseline * max_ratio
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def load_baselines(baselines_dir: str) -> tuple[dict[str, dict], list[str]]:
    """Load all baseline JSON files keyed by scenario name.

    Returns a tuple of (baselines, errors). In enforce mode callers should
    treat errors as fatal; in advisory/dry-run mode they are warnings.
    """
    baselines: dict[str, dict] = {}
    errors: list[str] = []
    dir_path = Path(baselines_dir)
    if not dir_path.is_dir():
        msg = f"Baselines directory not found: {baselines_dir}"
        errors.append(msg)
        return baselines, errors

    for file_path in sorted(dir_path.glob("*.json")):
        try:
            with file_path.open(encoding="utf-8") as fh:
                data = json.load(fh)
            if not isinstance(data, dict):
                errors.append(f"Baseline {file_path} is not a JSON object")
                continue
            scenario = data.get("scenario")
            if not scenario:
                errors.append(f"Baseline file missing 'scenario': {file_path}")
                continue
            data["_source_path"] = str(file_path)
            baselines[scenario] = data
        except json.JSONDecodeError as exc:
            errors.append(f"Invalid JSON in baseline {file_path}: {exc}")
        except Exception as exc:  # noqa: BLE001
            errors.append(f"Failed to read baseline {file_path}: {exc}")
    return baselines, errors


def is_authoritative_baseline(baseline: dict[str, Any]) -> tuple[bool, str]:
    """Return (is_authoritative, reason) for a loaded baseline.

    Enforced mode requires the baseline to explicitly declare itself
    authoritative and to contain validation metadata.
    """
    meta = baseline.get("meta")
    if not isinstance(meta, dict) or not meta:
        return False, "missing meta block"

    if not meta.get("authoritative"):
        note = str(meta.get("note", "")).lower()
        if "sample" in note or "non-authoritative" in note:
            return False, "sample / non-authoritative baseline"
        return False, "meta.authoritative is not true"

    for required in ("last_validated_commit", "validated_at"):
        if not meta.get(required):
            return False, f"missing meta.{required}"

    if meta.get("last_validated_commit") == "sample":
        return False, "last_validated_commit is sample"

    return True, ""


def is_sample_named_baseline(path: Path) -> bool:
    """Return True if the baseline filename starts with sample_."""
    return path.name.lower().startswith("sample_")


def validate_enforced_baseline(baseline: dict[str, Any]) -> tuple[bool, str]:
    """Validate a baseline for enforce mode. Returns (valid, reason)."""
    source_path = baseline.get("_source_path")
    if not source_path:
        return False, "missing _source_path"
    path = Path(source_path)
    if is_sample_named_baseline(path):
        return False, f"sample-named source file: {path.name}"
    return is_authoritative_baseline(baseline)


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
        description="Compare ferrum-stress JSON output against baselines.",
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
    parser.add_argument(
        "--enforce",
        action="store_true",
        help=(
            "Fail closed on missing, malformed, or non-authoritative baselines. "
            "Implies non-dry-run."
        ),
    )
    args = parser.parse_args()

    enforce = args.enforce
    dry_run = args.dry_run and not enforce

    # Load stress output
    stress_path = Path(args.stress_json)
    if not stress_path.is_file():
        print(f"[ERROR] Stress JSON file not found: {args.stress_json}")
        return 0 if dry_run else 1

    try:
        with stress_path.open(encoding="utf-8") as fh:
            stress_data = json.load(fh)
    except json.JSONDecodeError as exc:
        print(f"[ERROR] Invalid stress JSON: {exc}")
        return 0 if dry_run else 1

    scenarios = stress_data.get("scenarios", [])
    if not scenarios:
        print("[WARN] No scenarios found in stress output.")
        return 0 if dry_run else 1

    # Load baselines
    baselines, baseline_errors = load_baselines(args.baselines_dir)
    if baseline_errors:
        if enforce:
            for error in baseline_errors:
                print(f"[FAIL] {error}")
            return 1
        for error in baseline_errors:
            print(f"[WARN] {error}")
    if not baselines:
        print("[WARN] No baselines loaded. Comparison skipped.")
        return 0 if dry_run else 1

    # Compare each scenario
    all_passed = True
    print("═══════════════════════════════════════════════════════════════")
    print("  PERFORMANCE REGRESSION GATE")
    if enforce:
        print("  MODE: enforced (authoritative baselines required)")
    print("═══════════════════════════════════════════════════════════════")
    print()

    for scenario in scenarios:
        scenario_name = scenario.get("scenario", "unknown")
        baseline = baselines.get(scenario_name)
        if not baseline:
            msg = f"[SKIP] No baseline for scenario '{scenario_name}'"
            if enforce:
                print(f"[FAIL] {msg}")
                all_passed = False
            else:
                print(msg)
            print()
            continue

        if enforce:
            valid, reason = validate_enforced_baseline(baseline)
            if not valid:
                print(f"[FAIL] Baseline for '{scenario_name}' is not enforceable: {reason}")
                print()
                all_passed = False
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
        if dry_run:
            print("[ADVISORY] Thresholds exceeded, but dry-run mode prevents failure.")
        else:
            print("[FAIL] One or more scenarios exceeded baseline thresholds.")
    print("───────────────────────────────────────────────────────────────")

    if dry_run:
        return 0
    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
