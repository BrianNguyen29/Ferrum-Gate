#!/usr/bin/env python3
"""Tests for validate_monitoring_templates.py."""

import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_monitoring_templates as vmt


class TestStripYamlComment(unittest.TestCase):
    def test_removes_trailing_comment(self):
        self.assertEqual(
            vmt.strip_yaml_comment("key: value  # comment"),
            "key: value",
        )

    def test_preserves_hash_in_quoted_string(self):
        self.assertEqual(
            vmt.strip_yaml_comment('key: "value # not a comment"'),
            'key: "value # not a comment"',
        )


class TestCheckMarkers(unittest.TestCase):
    def test_template_marker_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text("# TEMPLATE\nkey: value\n", encoding="utf-8")
            self.assertEqual(vmt.check_markers(path), [])

    def test_non_production_claim_marker_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "alertmanager-config.yaml"
            path.write_text(
                "# NON-PRODUCTION CLAIM\nkey: value\n", encoding="utf-8"
            )
            self.assertEqual(vmt.check_markers(path), [])

    def test_local_only_note_marker_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate-alerts.yaml"
            path.write_text("# LOCAL_ONLY NOTE\nkey: value\n", encoding="utf-8")
            self.assertEqual(vmt.check_markers(path), [])

    def test_missing_marker_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text("key: value\n", encoding="utf-8")
            errors = vmt.check_markers(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("missing approved template marker", errors[0])


class TestCheckProductionDrift(unittest.TestCase):
    def test_commented_unsafe_value_ignored(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "# insecure_skip_verify: true\n"
                "# localhost:9093\n"
                "# nip.io\n"
                "# <your-domain>:443\n"
                "# <monitor-name>\n"
                "# http://localhost:9093/webhook\n"
                "# CHANGE_ME_TO_A_SECURE_TOKEN\n"
                "# REPLACE_WITH_PASSWORD\n"
                "safe: value\n",
                encoding="utf-8",
            )
            self.assertEqual(vmt.check_production_drift(path), [])

    def test_active_insecure_skip_verify_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\ninsecure_skip_verify: true\n", encoding="utf-8"
            )
            errors = vmt.check_production_drift(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("insecure_skip_verify: true", errors[0])

    def test_active_insecure_skip_verify_uppercase_true_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\ninsecure_skip_verify: TRUE\n", encoding="utf-8"
            )
            errors = vmt.check_production_drift(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("insecure_skip_verify: true", errors[0])

    def test_active_insecure_skip_verify_mixed_case_true_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\ninsecure_skip_verify: True\n", encoding="utf-8"
            )
            errors = vmt.check_production_drift(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("insecure_skip_verify: true", errors[0])

    def test_active_insecure_skip_verify_casefolded_key_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\nInsecure_Skip_Verify: TRUE\n", encoding="utf-8"
            )
            errors = vmt.check_production_drift(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("insecure_skip_verify: true", errors[0])

    def test_quoted_insecure_skip_verify_true_not_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\ninsecure_skip_verify: 'true'\n", encoding="utf-8"
            )
            self.assertEqual(vmt.check_production_drift(path), [])

    def test_active_localhost_alertmanager_target_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "alertmanager-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "receivers:\n"
                "  - name: 'alerts'\n"
                "    webhook_configs:\n"
                "      - url: 'http://localhost:9093/webhook'\n",
                encoding="utf-8",
            )
            errors = vmt.check_production_drift(path)
            self.assertTrue(len(errors) >= 1)
            self.assertTrue(
                any("http://localhost:9093/webhook" in e for e in errors)
            )

    def test_safe_fixture_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "alertmanager-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "# NON-PRODUCTION CLAIM\n"
                "receivers:\n"
                "  - name: 'production-alerts'\n"
                "    email_configs:\n"
                "      - to: 'ops@example.com'\n",
                encoding="utf-8",
            )
            self.assertEqual(vmt.check_production_drift(path), [])

    def test_active_localhost_uppercase_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "alertmanager-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "receivers:\n"
                "  - name: 'alerts'\n"
                "    webhook_configs:\n"
                "      - url: 'http://LOCALHOST:9093/webhook'\n",
                encoding="utf-8",
            )
            errors = vmt.check_production_drift(path)
            self.assertTrue(len(errors) >= 1)
            self.assertTrue(
                any("http://localhost:9093/webhook" in e for e in errors)
            )

    def test_active_nip_io_mixed_case_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "prometheus-scrape-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "static_configs:\n"
                "  - targets:\n"
                "      - 'api.Nip.IO:443'\n",
                encoding="utf-8",
            )
            errors = vmt.check_production_drift(path)
            self.assertTrue(len(errors) >= 1)
            self.assertTrue(any("nip.io" in e for e in errors))

    def test_active_webhook_mixed_case_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "alertmanager-config.yaml"
            path.write_text(
                "# TEMPLATE\n"
                "receivers:\n"
                "  - name: 'alerts'\n"
                "    webhook_configs:\n"
                "      - url: 'HTTP://LocalHost:9093/WebHook'\n",
                encoding="utf-8",
            )
            errors = vmt.check_production_drift(path)
            self.assertTrue(len(errors) >= 1)
            self.assertTrue(
                any("http://localhost:9093/webhook" in e for e in errors)
            )


class TestValidateIntegration(unittest.TestCase):
    def test_default_mode_passes_on_existing_templates(self):
        paths = [vmt.MONITORING_DIR / name for name in vmt.MONITORING_FILES]
        errors = vmt.validate(paths, production=False)
        self.assertEqual(errors, [])

    def test_production_mode_fails_on_existing_templates(self):
        paths = [vmt.MONITORING_DIR / name for name in vmt.MONITORING_FILES]
        errors = vmt.validate(paths, production=True)
        self.assertTrue(len(errors) > 0)
        self.assertTrue(any("insecure_skip_verify" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
