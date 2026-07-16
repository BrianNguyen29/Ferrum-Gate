#!/usr/bin/env python3
"""check_advisory_ratchet.py — Canonical advisory coverage/perf ratchet status manifest.

Reads existing evidence metadata and emits a non-blocking advisory status. The
default read-only mode does NOT run coverage or performance tests, so it is safe
for ordinary CI.

Approved statuses: advisory-pass, advisory-regression,
blocked-no-comparable-coverage, blocked-no-authoritative-performance-baseline.
"""

from __future__ import annotations

import argparse
import datetime
import json
import math
import re
import statistics
import subprocess
import sys
from pathlib import Path
from typing import Any

APPROVED_STATUSES = {
    "advisory-pass",
    "advisory-regression",
    "blocked-no-comparable-coverage",
    "blocked-no-authoritative-performance-baseline",
}

DEFAULT_COVERAGE_EVIDENCE = Path("baselines/coverage/coverage-evidence-a14150e.json")
DEFAULT_PERF_EVIDENCE = Path("baselines/evidence/perf-evidence-a14150e.json")

NON_CLAIMS = [
    "This output does not constitute release approval.",
    "This output does not constitute target readiness or G2/pilot authorization.",
    "This output does not constitute production readiness or SLO closure.",
    "This output does not constitute a hard CI gate, threshold enforcement, or promotion decision.",
]

CANONICAL_WORKSPACE_SCOPE = "workspace"
CANONICAL_COVERAGE_KIND = "ferrumgate.coverage-evidence"
CANONICAL_PERF_KIND = "ferrumgate.perf-baseline-evidence"
CANONICAL_FORMAT_VERSION = 1

LEGACY_COVERAGE_KIND = "coverage-baseline"
LEGACY_PERF_KIND = "perf-baseline"

ALLOWED_RUNNER_PROVIDER = "github-actions-self-hosted"
ALLOWED_RUNNER_CLASS = "ferrumgate-benchmark-linux-x86_64"

REQUIRED_SCENARIOS = {"health", "intent-compile", "sqlite-contention"}
REQUIRED_RUNS_PER_SCENARIO = 5
MAX_VARIANCE_PERCENT = 10.0
STABILITY_TOLERANCE = 1e-9
CRITICAL_METRICS = ["throughput_rps", "p95_latency_ms", "p99_latency_ms"]
SAMPLE_KEYWORDS = {"sample", "synthetic", "example", "fixture"}


def load_json(path: Path | None) -> tuple[dict[str, Any] | None, str]:
    """Load a JSON evidence file, returning (data, error_reason)."""
    if path is None:
        return None, "evidence path is None"
    if not path.is_file():
        return None, f"evidence file not found: {path}"
    try:
        with path.open(encoding="utf-8") as fh:
            data = json.load(fh)
    except UnicodeDecodeError as exc:
        return None, f"non-UTF8 evidence in {path}: {exc}"
    except json.JSONDecodeError as exc:
        return None, f"invalid JSON in {path}: {exc}"
    except OSError as exc:
        return None, f"could not read {path}: {exc}"
    except (RecursionError, OverflowError) as exc:
        return None, f"could not read evidence {path}: {exc}"
    except ValueError as exc:
        return None, f"invalid evidence value in {path}: {exc}"
    if not isinstance(data, dict):
        return None, f"evidence root is not a JSON object: {path}"
    return data, ""


def _is_nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and value.strip() != ""


def _safe_float(value: Any) -> tuple[float | None, str]:
    """Safely convert a value to a finite float; reject bools, strings, and overflows."""
    if isinstance(value, bool):
        return None, "boolean values are not allowed"
    if isinstance(value, str):
        return None, "string values are not allowed"
    try:
        f = float(value)
    except (TypeError, ValueError, OverflowError) as exc:
        return None, f"cannot convert to float: {exc}"
    if not math.isfinite(f):
        return None, "value is not finite"
    return f, ""


_STRICT_RFC3339_UTC_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$"
)


def _is_strict_rfc3339_utc(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    if not _STRICT_RFC3339_UTC_RE.fullmatch(value):
        return False
    try:
        datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return False
    return True


def _is_valid_runner(runner: Any) -> bool:
    if not isinstance(runner, dict):
        return False
    if runner.get("provider") != ALLOWED_RUNNER_PROVIDER:
        return False
    if runner.get("runner_class") != ALLOWED_RUNNER_CLASS:
        return False
    if not _is_nonempty_string(runner.get("run_id")):
        return False
    return True


def _path_has_sample_component(path: Any) -> bool:
    if not isinstance(path, str):
        return False
    parts = Path(path).parts
    return any(
        keyword in part.lower()
        for part in parts
        for keyword in SAMPLE_KEYWORDS
    )


def evaluate_coverage(
    evidence: dict[str, Any] | None, error: str, current_commit: str | None = None
) -> tuple[str, str]:
    """Return (verdict, reason) for coverage evidence."""
    if evidence is None:
        return "blocked", error or "coverage evidence is missing"
    if current_commit is None:
        current_commit = _git_commit() or ""
    if evidence.get("kind") == LEGACY_COVERAGE_KIND:
        return (
            "blocked",
            "legacy/sample coverage-baseline artifact is non-comparable (not canonical ferrumgate.coverage-evidence)",
        )
    if evidence.get("kind") != CANONICAL_COVERAGE_KIND:
        return (
            "blocked",
            f"unrecognized coverage evidence kind: {evidence.get('kind')!r}",
        )
    if evidence.get("format_version") != CANONICAL_FORMAT_VERSION:
        return "blocked", "coverage format_version must be 1"
    if evidence.get("scope") != CANONICAL_WORKSPACE_SCOPE:
        return (
            "blocked",
            f"coverage scope is {evidence.get('scope')!r}; must be {CANONICAL_WORKSPACE_SCOPE!r}",
        )
    if evidence.get("authoritative") is not True:
        return "blocked", "coverage evidence is not authoritative"
    if evidence.get("synthetic") is not False:
        return "blocked", "coverage synthetic must be False"
    if evidence.get("commit") != current_commit:
        return (
            "blocked",
            "coverage commit does not match inspected commit",
        )
    if not _is_strict_rfc3339_utc(evidence.get("captured_at")):
        return "blocked", "coverage captured_at is not strict RFC3339 UTC"
    if not _is_nonempty_string(evidence.get("capture_command")):
        return "blocked", "coverage capture_command is missing"
    if not _is_valid_runner(evidence.get("runner")):
        return "blocked", "coverage runner is not a canonical controlled runner"
    artifacts = evidence.get("artifacts")
    if not isinstance(artifacts, dict):
        return "blocked", "coverage artifacts is not an object"
    lcov = artifacts.get("lcov")
    summary = artifacts.get("summary")
    if not _is_nonempty_string(lcov):
        return "blocked", "coverage artifacts.lcov is missing"
    if not _is_nonempty_string(summary):
        return "blocked", "coverage artifacts.summary is missing"
    if _path_has_sample_component(lcov) or _path_has_sample_component(summary):
        return (
            "blocked",
            "coverage artifacts contain sample/synthetic/example/fixture component",
        )
    return "comparable", "full-workspace coverage evidence present"


REQUIRED_METRIC_KEYS = {
    "throughput_rps",
    "p50_latency_ms",
    "p95_latency_ms",
    "p99_latency_ms",
    "error_rate",
}


def _is_valid_metrics(metrics: Any) -> tuple[bool, str]:
    if not isinstance(metrics, dict):
        return False, "metrics is not an object"
    if set(metrics.keys()) != REQUIRED_METRIC_KEYS:
        return False, "metrics has invalid keys"
    throughput, err = _safe_float(metrics.get("throughput_rps"))
    if err:
        return False, f"throughput_rps invalid: {err}"
    assert throughput is not None
    if throughput <= 0:
        return False, "throughput_rps must be a finite positive number"
    p50, err = _safe_float(metrics.get("p50_latency_ms"))
    if err:
        return False, f"p50_latency_ms invalid: {err}"
    assert p50 is not None
    p95, err = _safe_float(metrics.get("p95_latency_ms"))
    if err:
        return False, f"p95_latency_ms invalid: {err}"
    assert p95 is not None
    p99, err = _safe_float(metrics.get("p99_latency_ms"))
    if err:
        return False, f"p99_latency_ms invalid: {err}"
    assert p99 is not None
    if p50 < 0 or p95 < 0 or p99 < 0:
        return False, "latency values must be non-negative"
    if not (p50 <= p95 <= p99):
        return False, "latency percentiles must satisfy p50 <= p95 <= p99"
    error_rate, err = _safe_float(metrics.get("error_rate"))
    if err:
        return False, f"error_rate invalid: {err}"
    assert error_rate is not None
    if error_rate < 0 or error_rate > 1:
        return False, "error_rate must be finite between 0 and 1"
    return True, ""


def _is_valid_raw_run(run: Any, current_commit: str) -> tuple[bool, str]:
    if not isinstance(run, dict):
        return False, "raw run is not an object"
    run_id = run.get("run_id")
    if not _is_nonempty_string(run_id):
        return False, "raw run missing run_id"
    if run.get("commit") != current_commit:
        return False, "raw run commit does not match inspected commit"
    scenario = run.get("scenario")
    if scenario not in REQUIRED_SCENARIOS:
        return False, f"raw run scenario {scenario!r} is not required"
    if not _is_strict_rfc3339_utc(run.get("captured_at")):
        return False, "raw run captured_at is not strict RFC3339 UTC"
    artifact_path = run.get("artifact_path")
    if not _is_nonempty_string(artifact_path):
        return False, "raw run missing artifact_path"
    if _path_has_sample_component(artifact_path):
        return False, "raw run artifact_path contains sample/synthetic/example/fixture component"
    if run.get("sample") is not False:
        return False, "raw run sample must be False"
    ok, reason = _is_valid_metrics(run.get("metrics"))
    if not ok:
        return False, f"raw run metrics invalid: {reason}"
    return True, ""


def _is_valid_stability(stability: Any, runs_per_scenario: int) -> tuple[bool, str]:
    if not isinstance(stability, dict):
        return False, "stability is not an object"
    if stability.get("method") != "median-relative-variance":
        return False, "stability method must be median-relative-variance"
    if stability.get("runs_per_scenario") != runs_per_scenario:
        return False, f"stability runs_per_scenario must be {runs_per_scenario}"
    variance, err = _safe_float(stability.get("max_variance_percent"))
    if err:
        return False, f"max_variance_percent invalid: {err}"
    assert variance is not None
    if variance < 0 or variance >= MAX_VARIANCE_PERCENT:
        return False, f"max_variance_percent must be strictly below {MAX_VARIANCE_PERCENT}"
    return True, ""


def _median_relative_variance(values: list[float], metric: str) -> tuple[float, str]:
    """Compute max relative deviation from median for a metric's five values.

    Latency metrics allow all-zero values (variance 0); any mixed zero/nonzero
    values are invalid. Throughput requires a positive median.
    """
    if not values:
        return 0.0, f"no values for metric {metric}"
    median = statistics.median(values)
    zero_count = sum(1 for v in values if v == 0)
    all_zero = zero_count == len(values)

    if metric == "throughput_rps":
        if median <= 0:
            return 0.0, "throughput median must be positive"
        if any(v <= 0 for v in values):
            return 0.0, "throughput values must be positive"
    else:
        if not all_zero and zero_count > 0:
            return 0.0, f"latency metric {metric} has mixed zero/nonzero values"
        if median == 0 and not all_zero:
            return 0.0, f"latency metric {metric} median zero but not all values zero"

    if median == 0:
        return 0.0, ""

    max_dev = 0.0
    for v in values:
        dev = abs(v - median) / median * 100
        max_dev = max(max_dev, dev)
    return max_dev, ""


def _compute_global_stability(raw_runs: list[dict[str, Any]]) -> tuple[float, str]:
    """Recompute the global median-relative variance across required scenarios."""
    runs_by_scenario: dict[str, list[dict[str, Any]]] = {s: [] for s in REQUIRED_SCENARIOS}
    for run in raw_runs:
        scenario = run.get("scenario")
        if scenario in runs_by_scenario:
            runs_by_scenario[scenario].append(run)

    global_max = 0.0
    for scenario in sorted(REQUIRED_SCENARIOS):
        runs = sorted(runs_by_scenario[scenario], key=lambda r: r.get("run_id", ""))
        if len(runs) != REQUIRED_RUNS_PER_SCENARIO:
            return (
                0.0,
                f"scenario {scenario!r} has {len(runs)} runs; required {REQUIRED_RUNS_PER_SCENARIO}",
            )

        values_by_metric: dict[str, list[float]] = {metric: [] for metric in CRITICAL_METRICS}
        for run in runs:
            metrics = run.get("metrics", {})
            for metric in CRITICAL_METRICS:
                values_by_metric[metric].append(float(metrics[metric]))

        scenario_max = 0.0
        for metric in CRITICAL_METRICS:
            max_dev, err = _median_relative_variance(values_by_metric[metric], metric)
            if err:
                return 0.0, f"scenario {scenario!r}: {err}"
            scenario_max = max(scenario_max, max_dev)
        global_max = max(global_max, scenario_max)

    return global_max, ""


def evaluate_perf(
    evidence: dict[str, Any] | None, error: str, current_commit: str | None = None
) -> tuple[str, str]:
    """Return (verdict, reason) for performance evidence."""
    if evidence is None:
        return "blocked", error or "perf evidence is missing"
    if current_commit is None:
        current_commit = _git_commit() or ""
    if evidence.get("kind") == LEGACY_PERF_KIND:
        return (
            "blocked",
            "legacy/sample perf-baseline artifact is non-authoritative (not canonical ferrumgate.perf-baseline-evidence)",
        )
    if evidence.get("kind") != CANONICAL_PERF_KIND:
        return (
            "blocked",
            f"unrecognized perf evidence kind: {evidence.get('kind')!r}",
        )
    if evidence.get("format_version") != CANONICAL_FORMAT_VERSION:
        return "blocked", "perf format_version must be 1"
    if evidence.get("authoritative") is not True:
        return "blocked", "perf evidence is not authoritative"
    if evidence.get("commit") != current_commit:
        return "blocked", "perf commit does not match inspected commit"
    if evidence.get("last_validated_commit") != current_commit:
        return (
            "blocked",
            "perf last_validated_commit does not match inspected commit",
        )
    if not _is_strict_rfc3339_utc(evidence.get("validated_at")):
        return "blocked", "perf validated_at is not strict RFC3339 UTC"
    if not _is_valid_runner(evidence.get("runner")):
        return "blocked", "perf runner is not a canonical controlled runner"
    artifacts = evidence.get("artifacts")
    if not isinstance(artifacts, dict):
        return "blocked", "perf artifacts is not an object"
    baseline = artifacts.get("baseline")
    if not _is_nonempty_string(baseline):
        return "blocked", "perf artifacts.baseline is missing"
    if _path_has_sample_component(baseline):
        return (
            "blocked",
            "perf artifacts.baseline contains sample/synthetic/example/fixture component",
        )
    stability = evidence.get("stability")
    if not isinstance(stability, dict):
        return "blocked", "perf stability is not an object"
    runs_per_scenario = stability.get("runs_per_scenario")
    if runs_per_scenario != REQUIRED_RUNS_PER_SCENARIO:
        return (
            "blocked",
            f"perf stability runs_per_scenario must be {REQUIRED_RUNS_PER_SCENARIO}",
        )
    ok, reason = _is_valid_stability(stability, REQUIRED_RUNS_PER_SCENARIO)
    if not ok:
        return "blocked", f"perf stability invalid: {reason}"
    raw_runs = evidence.get("raw_runs")
    if not isinstance(raw_runs, list):
        return "blocked", "perf raw_runs is not a list"
    required_total = REQUIRED_RUNS_PER_SCENARIO * len(REQUIRED_SCENARIOS)
    if len(raw_runs) < required_total:
        return "blocked", "perf raw_runs count is insufficient"
    seen_run_ids: set[str] = set()
    scenario_counts = {s: 0 for s in REQUIRED_SCENARIOS}
    for run in raw_runs:
        ok, reason = _is_valid_raw_run(run, current_commit)
        if not ok:
            return "blocked", f"perf raw run invalid: {reason}"
        run_id = run["run_id"]
        if run_id in seen_run_ids:
            return "blocked", "duplicate raw run run_id"
        seen_run_ids.add(run_id)
        scenario_counts[run["scenario"]] += 1
    for scenario, count in scenario_counts.items():
        if count != REQUIRED_RUNS_PER_SCENARIO:
            return (
                "blocked",
                f"perf scenario {scenario!r} has {count} runs; required {REQUIRED_RUNS_PER_SCENARIO}",
            )
    try:
        computed_max, err = _compute_global_stability(raw_runs)
    except (ValueError, OverflowError, RecursionError) as exc:
        return "blocked", f"perf stability computation error: {exc}"
    if err:
        return "blocked", f"perf stability computation failed: {err}"
    if computed_max >= MAX_VARIANCE_PERCENT:
        return (
            "blocked",
            f"perf computed variance {computed_max:.6f}% is not strictly below {MAX_VARIANCE_PERCENT}%",
        )
    declared_max = stability["max_variance_percent"]
    if abs(declared_max - computed_max) > STABILITY_TOLERANCE:
        return (
            "blocked",
            f"perf declared max_variance_percent {declared_max} does not match computed {computed_max}",
        )
    return "authoritative", "authoritative perf baseline evidence present"


def determine_status(coverage_verdict: str, perf_verdict: str) -> str:
    if coverage_verdict == "blocked":
        return "blocked-no-comparable-coverage"
    if perf_verdict == "blocked":
        return "blocked-no-authoritative-performance-baseline"
    return "advisory-pass"


def _git_commit() -> str | None:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
            timeout=5,
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except Exception:  # noqa: BLE001
        return None


def _detect_runner() -> str:
    """Return runner provenance. We never infer controlled authority from environment alone."""
    return "unspecified"


def _safe_evaluate_evidence(
    evaluate_fn: Any,
    evidence: dict[str, Any] | None,
    error: str,
    current_commit: str,
    block_reason: str,
) -> tuple[str, str]:
    """Evaluate evidence, catching evidence-driven arithmetic/decode failures."""
    try:
        return evaluate_fn(evidence, error, current_commit)
    except (ValueError, OverflowError, RecursionError) as exc:
        return "blocked", f"{block_reason}: {exc}"


def _utc_now() -> str:
    return datetime.datetime.now(tz=datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def build_manifest(
    coverage_path: Path,
    perf_path: Path,
    coverage_evidence: dict[str, Any] | None,
    coverage_verdict: str,
    coverage_reason: str,
    perf_evidence: dict[str, Any] | None,
    perf_verdict: str,
    perf_reason: str,
    status: str,
    current_commit: str | None = None,
) -> dict[str, Any]:
    if current_commit is None:
        current_commit = _git_commit() or ""
    return {
        "format_version": "1.0",
        "kind": "advisory-ratchet-status",
        "status": status,
        "generated_at": _utc_now(),
        "commit": current_commit,
        "coverage": {
            "path": str(coverage_path),
            "scope": coverage_evidence.get("scope") if coverage_evidence else None,
            "authoritative": coverage_evidence.get("authoritative") if coverage_evidence else None,
            "verdict": coverage_verdict,
            "reason": coverage_reason,
        },
        "perf": {
            "path": str(perf_path),
            "authoritative": perf_evidence.get("authoritative") if perf_evidence else None,
            "verdict": perf_verdict,
            "reason": perf_reason,
        },
        "provenance": {
            "command": "python3 scripts/check_advisory_ratchet.py",
            "runner": _detect_runner(),
            "note": "Advisory read-only ratchet; does not run coverage or perf tests.",
        },
        "non_claims": NON_CLAIMS,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Emit an advisory coverage/perf ratchet status manifest from existing evidence.",
    )
    parser.add_argument(
        "--coverage-evidence",
        type=Path,
        default=DEFAULT_COVERAGE_EVIDENCE,
        help="Path to coverage evidence JSON.",
    )
    parser.add_argument(
        "--perf-evidence",
        type=Path,
        default=DEFAULT_PERF_EVIDENCE,
        help="Path to performance evidence JSON.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Write the manifest JSON to this path.",
    )
    args = parser.parse_args()

    current_commit = _git_commit() or ""
    coverage_evidence, coverage_error = load_json(args.coverage_evidence)
    perf_evidence, perf_error = load_json(args.perf_evidence)

    coverage_verdict, coverage_reason = _safe_evaluate_evidence(
        evaluate_coverage,
        coverage_evidence,
        coverage_error,
        current_commit,
        "coverage evidence processing failed",
    )
    perf_verdict, perf_reason = _safe_evaluate_evidence(
        evaluate_perf,
        perf_evidence,
        perf_error,
        current_commit,
        "perf evidence processing failed",
    )
    status = determine_status(coverage_verdict, perf_verdict)

    if status not in APPROVED_STATUSES:
        print(f"[ERROR] Unapproved status emitted: {status}", file=sys.stderr)
        return 1

    manifest = build_manifest(
        coverage_path=args.coverage_evidence,
        perf_path=args.perf_evidence,
        coverage_evidence=coverage_evidence,
        coverage_verdict=coverage_verdict,
        coverage_reason=coverage_reason,
        perf_evidence=perf_evidence,
        perf_verdict=perf_verdict,
        perf_reason=perf_reason,
        status=status,
        current_commit=current_commit,
    )

    manifest_text = json.dumps(manifest, indent=2) + "\n"
    print(manifest_text)

    if args.output:
        with args.output.open("w", encoding="utf-8") as fh:
            fh.write(manifest_text)

    return 0


if __name__ == "__main__":
    sys.exit(main())
