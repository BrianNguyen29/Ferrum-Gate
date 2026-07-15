import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

from check_advisory_ratchet import (  # noqa: E402
    APPROVED_STATUSES,
    build_manifest,
    determine_status,
    evaluate_coverage,
    evaluate_perf,
)


class AdvisoryRatchetSchemaTests(unittest.TestCase):
    def _fixed_commit(self) -> str:
        return "abc123def456abc123def456abc123def456abc1"

    def _git_commit(self) -> str:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True, cwd=Path(__file__).resolve().parents[1]
        ).strip()

    def _valid_runner(self) -> dict:
        return {
            "provider": "github-actions-self-hosted",
            "runner_class": "ferrumgate-benchmark-linux-x86_64",
            "run_id": "run-123",
        }

    def _valid_coverage(self, commit: str) -> dict:
        return {
            "kind": "ferrumgate.coverage-evidence",
            "format_version": 1,
            "scope": "workspace",
            "authoritative": True,
            "synthetic": False,
            "commit": commit,
            "captured_at": "2026-07-15T00:00:00Z",
            "capture_command": "cargo llvm-cov test --workspace",
            "runner": self._valid_runner(),
            "artifacts": {
                "lcov": "baselines/coverage/coverage.lcov",
                "summary": "baselines/coverage/summary.json",
            },
        }

    def _valid_raw_run(self, commit: str, scenario: str, run_id: str) -> dict:
        return {
            "run_id": run_id,
            "commit": commit,
            "scenario": scenario,
            "captured_at": "2026-07-15T00:00:00Z",
            "artifact_path": f"baselines/evidence/{scenario}/{run_id}.json",
            "sample": False,
            "metrics": {
                "throughput_rps": 100.0,
                "p50_latency_ms": 10.0,
                "p95_latency_ms": 20.0,
                "p99_latency_ms": 30.0,
                "error_rate": 0.0,
            },
        }

    def _valid_perf(self, commit: str) -> dict:
        raw_runs = []
        for scenario in ("health", "intent-compile", "sqlite-contention"):
            for i in range(5):
                raw_runs.append(self._valid_raw_run(commit, scenario, f"{scenario}-{i}"))
        return {
            "kind": "ferrumgate.perf-baseline-evidence",
            "format_version": 1,
            "authoritative": True,
            "commit": commit,
            "last_validated_commit": commit,
            "validated_at": "2026-07-15T00:00:00Z",
            "runner": self._valid_runner(),
            "stability": {
                "method": "median-relative-variance",
                "runs_per_scenario": 5,
                "max_variance_percent": 0.0,
            },
            "raw_runs": raw_runs,
            "artifacts": {
                "baseline": "baselines/evidence/perf-baseline.json",
            },
        }

    def test_status_vocabulary_exactly_four(self):
        self.assertEqual(
            APPROVED_STATUSES,
            {
                "advisory-pass",
                "advisory-regression",
                "blocked-no-comparable-coverage",
                "blocked-no-authoritative-performance-baseline",
            },
        )

    def test_determine_status_uses_only_approved_values(self):
        self.assertEqual(determine_status("comparable", "authoritative"), "advisory-pass")
        self.assertEqual(
            determine_status("blocked", "authoritative"),
            "blocked-no-comparable-coverage",
        )
        self.assertEqual(
            determine_status("comparable", "blocked"),
            "blocked-no-authoritative-performance-baseline",
        )
        self.assertIn(determine_status("blocked", "blocked"), APPROVED_STATUSES)

    def test_full_valid_coverage_is_comparable(self):
        commit = self._fixed_commit()
        v, r = evaluate_coverage(self._valid_coverage(commit), "", commit)
        self.assertEqual(v, "comparable")
        self.assertIn("full-workspace", r)

    def test_full_valid_perf_is_authoritative(self):
        commit = self._fixed_commit()
        v, r = evaluate_perf(self._valid_perf(commit), "", commit)
        self.assertEqual(v, "authoritative")
        self.assertIn("authoritative", r)

    def test_no_enforcement_claims_in_output(self):
        m = build_manifest(
            Path("cov.json"), Path("perf.json"),
            self._valid_coverage("abc"), "comparable", "ok",
            self._valid_perf("abc"), "authoritative", "ok",
            "advisory-pass",
        )
        text = json.dumps({k: v for k, v in m.items() if k != "non_claims"}).lower()
        for phrase in ("release approval", "production ready", "target readiness", "g2/pilot", "slo closure"):
            self.assertNotIn(phrase, text, f"forbidden claim: {phrase!r}")
        self.assertTrue(any("hard" in c.lower() for c in m["non_claims"]), m["non_claims"])

    def _run_script(self, coverage_path: Path, perf_path: Path) -> tuple[int, dict]:
        r = subprocess.run(
            [sys.executable, "scripts/check_advisory_ratchet.py",
             "--coverage-evidence", str(coverage_path),
             "--perf-evidence", str(perf_path)],
            capture_output=True, text=True, check=False,
        )
        return r.returncode, json.loads(r.stdout)

    def _write(self, root: Path, name: str, data: dict) -> Path:
        p = root / name
        p.write_text(json.dumps(data), encoding="utf-8")
        return p

    def test_full_canonical_fixture_advisory_pass(self):
        commit = self._git_commit()
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            cov = self._write(root, "coverage.json", self._valid_coverage(commit))
            perf = self._write(root, "perf.json", self._valid_perf(commit))
            code, m = self._run_script(cov, perf)
        self.assertEqual(code, 0)
        self.assertEqual(m["status"], "advisory-pass")
        self.assertEqual(m["provenance"]["runner"], "unspecified")

    def test_current_evidence_is_blocked(self):
        repo = Path(__file__).resolve().parents[1]
        cov = repo / "baselines" / "coverage" / "coverage-evidence-a14150e.json"
        perf = repo / "baselines" / "evidence" / "perf-evidence-a14150e.json"
        self.assertTrue(cov.exists() and perf.exists())
        code, m = self._run_script(cov, perf)
        self.assertEqual(code, 0)
        self.assertEqual(m["status"], "blocked-no-comparable-coverage")

    def test_non_utf8_evidence_blocked_exit_zero(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            cov = root / "coverage.json"
            cov.write_bytes(b"\xff\xfe")
            perf = self._write(root, "perf.json", self._valid_perf(self._fixed_commit()))
            code, m = self._run_script(cov, perf)
        self.assertEqual(code, 0)
        self.assertIn(m["status"], APPROVED_STATUSES)
        self.assertEqual(m["status"], "blocked-no-comparable-coverage")
        self.assertIn("non-UTF8", m["coverage"]["reason"])

    def test_rfc3339_space_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_coverage(commit)
        e["captured_at"] = "2026-07-15 00:00:00Z"
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")

    def test_rfc3339_no_z_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_coverage(commit)
        e["captured_at"] = "2026-07-15T00:00:00+00:00"
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")

    def test_rfc3339_invalid_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_coverage(commit)
        e["captured_at"] = "2026-02-30T00:00:00Z"
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")

    def test_synthetic_missing_null_string_rejected(self):
        commit = self._fixed_commit()
        for synthetic in (None, "false", True):
            e = self._valid_coverage(commit)
            e["synthetic"] = synthetic
            v, r = evaluate_coverage(e, "", commit)
            self.assertEqual(v, "blocked", f"synthetic={synthetic!r} reason: {r}")
        # missing synthetic
        e = self._valid_coverage(commit)
        del e["synthetic"]
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")

    def test_nonnumeric_string_null_metrics_rejected(self):
        commit = self._fixed_commit()
        for bad_value in ("100", None):
            e = self._valid_perf(commit)
            e["raw_runs"][0]["metrics"]["throughput_rps"] = bad_value
            self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked", f"bad_value={bad_value!r}")

    def test_bool_nan_infinity_negative_variance_rejected(self):
        commit = self._fixed_commit()
        for bad_variance in (True, float("nan"), float("inf"), float("-inf"), -1, 11):
            e = self._valid_perf(commit)
            e["stability"]["max_variance_percent"] = bad_variance
            v, r = evaluate_perf(e, "", commit)
            self.assertEqual(v, "blocked", f"bad_variance={bad_variance!r} reason: {r}")

    def test_invalid_metric_ordering_error_rate_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        e["raw_runs"][0]["metrics"]["p95_latency_ms"] = 5.0
        e["raw_runs"][0]["metrics"]["p50_latency_ms"] = 50.0
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")
        e = self._valid_perf(commit)
        e["raw_runs"][0]["metrics"]["error_rate"] = 1.5
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_sample_paths_in_nested_components_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_coverage(commit)
        e["artifacts"]["lcov"] = "baselines/sample/coverage.lcov"
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")
        e = self._valid_perf(commit)
        e["raw_runs"][0]["artifact_path"] = "baselines/evidence/sample_run.json"
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_sample_true_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        e["raw_runs"][0]["sample"] = True
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_arbitrary_runner_provider_class_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_coverage(commit)
        e["runner"]["provider"] = "github-actions"
        self.assertEqual(evaluate_coverage(e, "", commit)[0], "blocked")
        e = self._valid_perf(commit)
        e["runner"]["runner_class"] = "custom-runner"
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_fake_commit_only_raw_records_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        e["raw_runs"][0]["commit"] = "deadbeef" * 5
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_fewer_than_five_per_scenario_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        # keep only 4 health runs
        e["raw_runs"] = [r for r in e["raw_runs"] if r["scenario"] != "health" or int(r["run_id"].split("-")[-1]) < 4]
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_duplicate_run_ids_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        e["raw_runs"][1]["run_id"] = e["raw_runs"][0]["run_id"]
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_run_count_one_per_scenario_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        # Keep exactly one run per scenario
        e["raw_runs"] = [
            self._valid_raw_run(commit, "health", "health-0"),
            self._valid_raw_run(commit, "intent-compile", "intent-compile-0"),
            self._valid_raw_run(commit, "sqlite-contention", "sqlite-contention-0"),
        ]
        self.assertEqual(evaluate_perf(e, "", commit)[0], "blocked")

    def test_perf_variance_exact_ten_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        # Make one health throughput value deviate exactly 10% from median 100
        for i, run in enumerate(e["raw_runs"]):
            if run["scenario"] == "health":
                run["metrics"]["throughput_rps"] = 110.0 if i == 0 else 100.0
        e["stability"]["max_variance_percent"] = 10.0
        v, r = evaluate_perf(e, "", commit)
        self.assertEqual(v, "blocked", f"reason: {r}")
        self.assertIn("10", r)

    def test_perf_forged_low_variance_rejected(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        # Wildly varying throughput in health: median 300, max deviation 66.7%
        for i, run in enumerate(e["raw_runs"]):
            if run["scenario"] == "health":
                run["metrics"]["throughput_rps"] = 100.0 + i * 100.0
        e["stability"]["max_variance_percent"] = 2.5
        v, r = evaluate_perf(e, "", commit)
        self.assertEqual(v, "blocked", f"reason: {r}")
        self.assertIn("variance", r.lower())

    def test_perf_zero_latency_all_zero_stable(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        for run in e["raw_runs"]:
            run["metrics"]["p50_latency_ms"] = 0.0
            run["metrics"]["p95_latency_ms"] = 0.0
            run["metrics"]["p99_latency_ms"] = 0.0
        e["stability"]["max_variance_percent"] = 0.0
        v, r = evaluate_perf(e, "", commit)
        self.assertEqual(v, "authoritative", f"reason: {r}")

    def test_perf_zero_latency_mixed_zero_blocked(self):
        commit = self._fixed_commit()
        e = self._valid_perf(commit)
        # Set p50 and p99 to fixed values; one p95 latency is zero while others are non-zero
        for i, run in enumerate(e["raw_runs"]):
            if run["scenario"] == "health":
                run["metrics"]["p50_latency_ms"] = 0.0
                run["metrics"]["p95_latency_ms"] = 0.0 if i == 0 else 20.0
                run["metrics"]["p99_latency_ms"] = 20.0
        e["stability"]["max_variance_percent"] = 0.0
        v, r = evaluate_perf(e, "", commit)
        self.assertEqual(v, "blocked", f"reason: {r}")
        self.assertIn("mixed zero", r.lower())

    def test_deeply_nested_json_blocked_exit_zero(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            depth = 10000
            nested = "1"
            for _ in range(depth):
                nested = f'{{"a":{nested}}}'
            cov = root / "coverage.json"
            cov.write_text(nested, encoding="utf-8")
            perf = self._write(root, "perf.json", self._valid_perf(self._fixed_commit()))
            code, m = self._run_script(cov, perf)
        self.assertEqual(code, 0)
        self.assertEqual(m["status"], "blocked-no-comparable-coverage")
        self.assertIn("could not read evidence", m["coverage"]["reason"])

    def test_malformed_evidence_blocked_exit_zero(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            cov = root / "coverage.json"
            cov.write_text("{not valid json", encoding="utf-8")
            perf = root / "perf.json"
            perf.write_text("{not valid json", encoding="utf-8")
            code, m = self._run_script(cov, perf)
        self.assertEqual(code, 0)
        self.assertIn(m["status"], APPROVED_STATUSES)
        self.assertEqual(m["status"], "blocked-no-comparable-coverage")

    def test_detect_runner_is_unspecified(self):
        from check_advisory_ratchet import _detect_runner
        self.assertEqual(_detect_runner(), "unspecified")

    def test_oversized_int_metric_blocked_exit_zero(self):
        """A 309+ digit integer metric must be blocked and exit 0."""
        commit = self._git_commit()
        big_int = int("9" * 309)
        perf = self._valid_perf(commit)
        perf["raw_runs"][0]["metrics"]["throughput_rps"] = big_int
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            cov = self._write(root, "coverage.json", self._valid_coverage(commit))
            perf_path = self._write(root, "perf.json", perf)
            code, m = self._run_script(cov, perf_path)
        self.assertEqual(code, 0)
        self.assertEqual(m["status"], "blocked-no-authoritative-performance-baseline")
        self.assertIn("invalid", m["perf"]["reason"].lower())

    def test_json_integer_over_digit_limit_blocked_exit_zero(self):
        """Portable test: when decoder/interpreter has a digit limit, boundary catches it.

        When no limit exists, a huge integer metric is still rejected by numeric validation.
        """
        commit = self._git_commit()
        huge_literal = "9" * 10000
        raw_json = "{" + json.dumps("huge_int") + ": " + huge_literal + "}"
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            p = root / "huge.json"
            p.write_text(raw_json, encoding="utf-8")
            try:
                parsed = json.loads(raw_json)
            except (ValueError, OverflowError):
                # Decoder has a digit limit; the CLI boundary should catch the decode failure.
                code, m = self._run_script(p, self._write(root, "perf.json", self._valid_perf(commit)))
                self.assertEqual(code, 0)
                self.assertEqual(m["status"], "blocked-no-comparable-coverage")
                self.assertIn("invalid", m["coverage"]["reason"].lower())
                return
            # No digit limit; use the parsed huge int as a metric value.
            perf = self._valid_perf(commit)
            perf["raw_runs"][0]["metrics"]["throughput_rps"] = parsed["huge_int"]
            perf_path = self._write(root, "perf.json", perf)
            cov = self._write(root, "coverage.json", self._valid_coverage(commit))
            code, m = self._run_script(cov, perf_path)
        self.assertEqual(code, 0)
        self.assertEqual(m["status"], "blocked-no-authoritative-performance-baseline")
        self.assertIn("invalid", m["perf"]["reason"].lower())


if __name__ == "__main__":
    unittest.main()
