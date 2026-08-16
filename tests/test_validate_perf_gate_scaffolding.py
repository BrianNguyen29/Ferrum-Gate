import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.compare_perf_baselines import (
    compare_scenario,
    is_authoritative_baseline,
    load_baselines,
    validate_enforced_baseline,
)
from scripts.generate_perf_baselines import generate_baselines, load_stress_json


class PerfGateScaffoldingTests(unittest.TestCase):
    """Fixture tests for the perf gate comparator and generator."""

    def _make_stress_data(self, scenario: str, metrics: dict) -> dict:
        return {
            "format_version": "1.0",
            "scenarios": [
                {
                    "scenario": scenario,
                    "concurrency": 50,
                    "duration_secs": 5,
                    **metrics,
                }
            ],
        }

    def _make_baseline(self, scenario: str, metrics: dict, meta: dict) -> dict:
        return {
            "format_version": "1.0",
            "scenario": scenario,
            "concurrency": 50,
            "duration_secs": 5,
            "metrics": metrics,
            "meta": meta,
        }

    def test_authoritative_baseline_passes(self):
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
                "p95_ms": {"baseline": 10.0, "unit": "ms", "max_ratio": 2.00},
                "error_rate": {"baseline": 0.0, "unit": "ratio", "max_absolute": 0.01},
            },
            {
                "authoritative": True,
                "last_validated_commit": "abc1234",
                "validated_at": "2026-06-25T00:00:00Z",
                "note": "Authoritative baseline",
            },
        )
        self.assertTrue(is_authoritative_baseline(baseline)[0])

    def test_sample_baseline_rejected_in_enforce(self):
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
            },
            {
                "authoritative": False,
                "last_validated_commit": "sample",
                "validated_at": "2026-06-25T00:00:00Z",
                "note": "SAMPLE / NON-AUTHORITATIVE",
            },
        )
        authoritative, reason = is_authoritative_baseline(baseline)
        self.assertFalse(authoritative)
        self.assertIn("sample", reason)

    def test_missing_meta_rejected_in_enforce(self):
        baseline = self._make_baseline(
            "health",
            {"req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80}},
            {},
        )
        authoritative, reason = is_authoritative_baseline(baseline)
        self.assertFalse(authoritative)
        self.assertIn("missing meta", reason)

    def test_missing_last_validated_commit_rejected_in_enforce(self):
        baseline = self._make_baseline(
            "health",
            {"req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80}},
            {
                "authoritative": True,
                "validated_at": "2026-06-25T00:00:00Z",
            },
        )
        authoritative, reason = is_authoritative_baseline(baseline)
        self.assertFalse(authoritative)
        self.assertIn("last_validated_commit", reason)

    def test_compare_scenario_passes_when_within_threshold(self):
        stress = self._make_stress_data("health", {"req_per_sec": 900.0, "p95_ms": 15.0, "error_rate": 0.0})
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
                "p95_ms": {"baseline": 10.0, "unit": "ms", "max_ratio": 2.00},
                "error_rate": {"baseline": 0.0, "unit": "ratio", "max_absolute": 0.01},
            },
            {"authoritative": True, "last_validated_commit": "abc", "validated_at": "2026-01-01T00:00:00Z"},
        )
        passed, _ = compare_scenario(stress["scenarios"][0], baseline, 0.20)
        self.assertTrue(passed)

    def test_compare_scenario_fails_when_throughput_drops(self):
        stress = self._make_stress_data("health", {"req_per_sec": 700.0})
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
            },
            {"authoritative": True, "last_validated_commit": "abc", "validated_at": "2026-01-01T00:00:00Z"},
        )
        passed, _ = compare_scenario(stress["scenarios"][0], baseline, 0.20)
        self.assertFalse(passed)

    def test_generated_baseline_is_non_authoritative_by_default(self):
        stress = self._make_stress_data("health", {"req_per_sec": 500.0, "p95_ms": 5.0, "error_rate": 0.0})
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            written = generate_baselines(stress, output_dir, 5, False, None, None)
            self.assertEqual(len(written), 1)
            baseline = json.loads(written[0].read_text(encoding="utf-8"))
            self.assertFalse(baseline["meta"]["authoritative"])
            self.assertIn("SAMPLE", baseline["meta"]["note"])
            self.assertIn("req_per_sec", baseline["metrics"])
            self.assertEqual(baseline["metrics"]["req_per_sec"]["baseline"], 500.0)

    def test_generated_baseline_requires_promotion_env_for_authoritative(self):
        stress = self._make_stress_data("health", {"req_per_sec": 500.0})
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            with self.assertRaises(ValueError):
                generate_baselines(stress, output_dir, 5, True, None, None)

    def test_generated_baseline_authoritative_when_promoted(self):
        stress = self._make_stress_data("health", {"req_per_sec": 500.0})
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            written = generate_baselines(
                stress, output_dir, 5, True, "abc1234", "2026-06-25T00:00:00Z"
            )
            self.assertEqual(len(written), 1)
            baseline = json.loads(written[0].read_text(encoding="utf-8"))
            self.assertTrue(baseline["meta"]["authoritative"])
            self.assertEqual(baseline["meta"]["last_validated_commit"], "abc1234")

    def test_generated_baseline_authoritative_defaults_at_when_commit_only(self):
        stress = self._make_stress_data("health", {"req_per_sec": 500.0})
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            written = generate_baselines(stress, output_dir, 5, True, "abc1234", None)
            self.assertEqual(len(written), 1)
            baseline = json.loads(written[0].read_text(encoding="utf-8"))
            self.assertTrue(baseline["meta"]["authoritative"])
            self.assertEqual(baseline["meta"]["last_validated_commit"], "abc1234")
            self.assertNotEqual(baseline["meta"]["validated_at"], "2026-01-01T00:00:00Z")

    def test_load_baselines_skips_malformed_files_in_advisory_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            good = root / "good.json"
            good.write_text(json.dumps({"scenario": "health", "meta": {}}), encoding="utf-8")
            bad = root / "bad.json"
            bad.write_text("not json", encoding="utf-8")
            missing_scenario = root / "missing_scenario.json"
            missing_scenario.write_text(json.dumps({"meta": {}}), encoding="utf-8")
            baselines, errors = load_baselines(str(root))
            self.assertEqual(set(baselines.keys()), {"health"})
            self.assertTrue(any("Invalid JSON" in e for e in errors))

    def test_load_baselines_returns_errors_for_malformed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            good = root / "good.json"
            good.write_text(json.dumps({"scenario": "health", "meta": {}}), encoding="utf-8")
            bad = root / "bad.json"
            bad.write_text("not json", encoding="utf-8")
            baselines, errors = load_baselines(str(root))
            self.assertEqual(set(baselines.keys()), {"health"})
            self.assertTrue(any("Invalid JSON" in e for e in errors))

    def test_sample_named_authoritative_baseline_rejected_in_enforce(self):
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
            },
            {
                "authoritative": True,
                "last_validated_commit": "abc1234",
                "validated_at": "2026-06-25T00:00:00Z",
                "note": "Authoritative baseline",
            },
        )
        baseline["_source_path"] = "sample_health_5s.json"
        valid, reason = validate_enforced_baseline(baseline)
        self.assertFalse(valid)
        self.assertIn("sample", reason)

    def test_missing_source_path_rejected_in_enforce(self):
        baseline = self._make_baseline(
            "health",
            {
                "req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80},
            },
            {
                "authoritative": True,
                "last_validated_commit": "abc1234",
                "validated_at": "2026-06-25T00:00:00Z",
                "note": "Authoritative baseline",
            },
        )
        valid, reason = validate_enforced_baseline(baseline)
        self.assertFalse(valid)
        self.assertIn("_source_path", reason)

    def test_enforce_mode_rejects_sample_named_authoritative_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline_dir = root / "baselines"
            baseline_dir.mkdir()
            baseline = self._make_baseline(
                "health",
                {"req_per_sec": {"baseline": 1000.0, "unit": "req/s", "min_ratio": 0.80}},
                {
                    "authoritative": True,
                    "last_validated_commit": "abc1234",
                    "validated_at": "2026-06-25T00:00:00Z",
                    "note": "Authoritative baseline",
                },
            )
            sample_path = baseline_dir / "sample_health_5s.json"
            sample_path.write_text(json.dumps(baseline), encoding="utf-8")
            stress = self._make_stress_data("health", {"req_per_sec": 900.0})
            stress_path = root / "stress.json"
            stress_path.write_text(json.dumps(stress), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/compare_perf_baselines.py",
                    "--stress-json",
                    str(stress_path),
                    "--baselines-dir",
                    str(baseline_dir),
                    "--enforce",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("sample", result.stdout.lower())

    def test_enforce_mode_fails_on_malformed_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline_dir = root / "baselines"
            baseline_dir.mkdir()
            good = baseline_dir / "health.json"
            good.write_text(json.dumps({"scenario": "health", "meta": {}}), encoding="utf-8")
            bad = baseline_dir / "bad.json"
            bad.write_text("not json", encoding="utf-8")
            stress = self._make_stress_data("health", {"req_per_sec": 900.0})
            stress_path = root / "stress.json"
            stress_path.write_text(json.dumps(stress), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/compare_perf_baselines.py",
                    "--stress-json",
                    str(stress_path),
                    "--baselines-dir",
                    str(baseline_dir),
                    "--enforce",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("Invalid JSON", result.stdout)

    def test_generated_authoritative_baseline_uses_non_sample_name(self):
        stress = self._make_stress_data("health", {"req_per_sec": 500.0})
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            written = generate_baselines(
                stress, output_dir, 5, True, "abc1234", "2026-06-25T00:00:00Z"
            )
            self.assertEqual(len(written), 1)
            self.assertTrue(written[0].name.startswith("baseline_"))
            self.assertFalse(written[0].name.startswith("sample_"))

    def test_load_stress_json_requires_scenarios_array(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "stress.json"
            path.write_text(json.dumps({"format_version": "1.0"}), encoding="utf-8")
            with self.assertRaises(ValueError) as ctx:
                load_stress_json(path)
            self.assertIn("scenarios", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
