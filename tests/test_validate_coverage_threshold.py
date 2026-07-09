import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import check_coverage_threshold as cct


class CoverageThresholdTests(unittest.TestCase):
    def test_parse_coverage_text_extracts_crate_averages_and_total(self):
        source = """
TOTAL                                                                 78.45%

Filename                                                                    Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
crates/ferrum-pdp/src/engine.rs                                                100                10   90.00%          20                 2   90.00%         200                20   90.00%           -                 -       -
crates/ferrum-pdp/src/policy.rs                                                100                50   50.00%          20                10   50.00%         200               100   50.00%           -                 -       -
crates/ferrum-gateway/src/server.rs                                             80                40   50.00%          10                 5   50.00%         160                80   50.00%           -                 -       -
crates/ferrum-store/src/lib.rs                                                  40                20   50.00%           5                 2   60.00%          80                40   50.00%           -                 -       -
"""
        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(source)
            path = fh.name

        try:
            result = cct.parse_coverage_text(path)
        finally:
            Path(path).unlink()

        self.assertAlmostEqual(result["ferrum-pdp"], 70.0)
        self.assertAlmostEqual(result["ferrum-gateway"], 50.0)
        self.assertAlmostEqual(result["ferrum-store"], 50.0)
        self.assertAlmostEqual(result["TOTAL"], 78.45)

    def test_load_config_reads_thresholds_and_monitor_only(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "coverage.toml"
            config.write_text(
                "[thresholds]\n"
                "ferrum-pdp = 55.0\n"
                "ferrum-gateway = 45.0\n"
                "ferrum-proto = 0.0\n",
                encoding="utf-8",
            )
            thresholds = cct.load_config(config)

        self.assertEqual(thresholds["ferrum-pdp"], 55.0)
        self.assertEqual(thresholds["ferrum-gateway"], 45.0)
        self.assertEqual(thresholds["ferrum-proto"], 0.0)

    def test_load_config_falls_back_to_defaults_when_missing(self):
        thresholds = cct.load_config(Path("/nonexistent/coverage-thresholds.toml"))
        self.assertEqual(thresholds, dict(cct.DEFAULT_CRITICAL_CRATES))

    def _run_script(self, args, coverage_text):
        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(coverage_text)
            coverage_path = fh.name

        with tempfile.NamedTemporaryFile("w", delete=False) as fh2:
            fh2.write(
                "[thresholds]\n"
                "ferrum-pdp = 70.0\n"
                "ferrum-gateway = 45.0\n"
                "ferrum-proto = 0.0\n"
            )
            config_path = fh2.name

        try:
            cmd = [
                sys.executable,
                "scripts/check_coverage_threshold.py",
                coverage_path,
                "--config",
                config_path,
            ] + args
            return subprocess.run(cmd, capture_output=True, text=True)
        finally:
            Path(coverage_path).unlink(missing_ok=True)
            Path(config_path).unlink(missing_ok=True)

    def test_script_passes_when_all_nonzero_thresholds_met(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 2   80.00%
crates/ferrum-gateway/src/server.rs                                               10                 5   50.00%
"""
        result = self._run_script([], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertIn("[PASS] ferrum-pdp: 80.00% >= 70.00%", result.stdout)
        self.assertIn("[PASS] ferrum-gateway: 50.00% >= 45.00%", result.stdout)

    def test_script_soft_warns_when_threshold_missed(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 5   50.00%
crates/ferrum-gateway/src/server.rs                                               10                 5   50.00%
"""
        result = self._run_script([], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertIn("[WARN] ferrum-pdp: 50.00% < 70.00% (soft mode)", result.stdout)
        self.assertIn("[PASS] ferrum-gateway: 50.00% >= 45.00%", result.stdout)

    def test_script_hard_fails_when_threshold_missed(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 5   50.00%
crates/ferrum-gateway/src/server.rs                                               10                 5   50.00%
"""
        result = self._run_script(["--hard"], coverage)
        self.assertEqual(result.returncode, 1)
        self.assertIn("[FAIL] ferrum-pdp: 50.00% < 70.00% (hard mode)", result.stdout)

    def test_monitor_only_crate_prints_info_without_warning(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-proto/src/lib.rs                                                   10                 5   50.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 2   80.00%
"""
        result = self._run_script([], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertIn("[INFO] ferrum-proto: 50.00% (monitor-only, no threshold)", result.stdout)
        self.assertNotIn("[WARN] ferrum-proto", result.stdout)

    def test_monitor_only_crate_missing_silently_skipped(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 2   80.00%
"""
        result = self._run_script([], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertNotIn("ferrum-proto", result.stdout)

    def test_script_warns_for_configured_crate_missing_from_report(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-gateway/src/server.rs                                               10                 5   50.00%
"""
        result = self._run_script([], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertIn("[WARN] No coverage data found for crate 'ferrum-pdp'", result.stdout)

    def test_crate_and_threshold_override(self):
        coverage = """
TOTAL                                                                 80.00%
crates/ferrum-pdp/src/engine.rs                                                   10                 5   50.00%
"""
        result = self._run_script(["--crate", "ferrum-pdp", "--threshold", "40.0"], coverage)
        self.assertEqual(result.returncode, 0)
        self.assertIn("[PASS] ferrum-pdp: 50.00% >= 40.00%", result.stdout)


if __name__ == "__main__":
    unittest.main()
