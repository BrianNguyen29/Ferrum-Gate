#!/usr/bin/env python3
"""Tests for validate_toml_configs.py"""

import sys
import tempfile
from pathlib import Path
import unittest

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_toml_configs as vtc


class TestCheckParsable(unittest.TestCase):
    def test_valid_toml_passes(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
            f.write('[server]\nhost = "0.0.0.0"\nport = 8080\n')
            path = Path(f.name)
        try:
            self.assertEqual(vtc.check_parsable(path), [])
        finally:
            path.unlink()

    def test_invalid_toml_fails(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
            f.write("[server\nhost = 8080\n")
            path = Path(f.name)
        try:
            errors = vtc.check_parsable(path)
            self.assertTrue(len(errors) > 0)
            self.assertTrue(any("parse error" in e for e in errors))
        finally:
            path.unlink()


class TestCheckSafety(unittest.TestCase):
    def test_prod_with_disabled_auth_fails(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".prod.toml", delete=False) as f:
            f.write('[server]\nauth_mode = "disabled"\n')
            path = Path(f.name)
        try:
            errors, _warnings = vtc.check_safety(path)
            self.assertTrue(any("auth_mode=disabled" in e for e in errors))
        finally:
            path.unlink()

    def test_prod_with_insecure_bind_fails(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".prod.toml", delete=False) as f:
            f.write("[server]\nallow_insecure_nonlocal_bind = true\n")
            path = Path(f.name)
        try:
            errors, _warnings = vtc.check_safety(path)
            self.assertTrue(any("allow_insecure_nonlocal_bind=true" in e for e in errors))
        finally:
            path.unlink()

    def test_nonprod_disabled_auth_passes(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".dev.toml", delete=False) as f:
            f.write('[server]\nauth_mode = "disabled"\n')
            path = Path(f.name)
        try:
            errors, warnings = vtc.check_safety(path)
            self.assertEqual(errors, [])
            self.assertEqual(warnings, [])
        finally:
            path.unlink()

    def test_prod_bearer_auth_passes(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".prod.toml", delete=False) as f:
            f.write('[server]\nauth_mode = "bearer"\n')
            path = Path(f.name)
        try:
            errors, warnings = vtc.check_safety(path)
            self.assertEqual(errors, [])
            self.assertEqual(warnings, [])
        finally:
            path.unlink()

    def test_prod_required_controls_missing_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate.prod.toml"
            path.write_text('[server]\nauth_mode = "bearer"\n')
            errors, _warnings = vtc.check_safety(path)
            for control in vtc.PROD_REQUIRED_CONTROLS:
                self.assertTrue(
                    any(control in e and "must be explicitly set to true" in e for e in errors),
                    f"expected missing-control error for {control}",
                )

    def test_prod_required_controls_disabled_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate.prod.toml"
            path.write_text(
                '[server]\n'
                'auth_mode = "bearer"\n'
                'lifecycle_reconciliation_enabled = false\n'
                'approval_timeout_enabled = false\n'
                'audit_fail_closed = false\n'
            )
            errors, _warnings = vtc.check_safety(path)
            for control in vtc.PROD_REQUIRED_CONTROLS:
                self.assertTrue(
                    any(control in e and "must be set to true" in e for e in errors),
                    f"expected disabled-control error for {control}",
                )

    def test_prod_required_controls_enabled_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate.prod.toml"
            path.write_text(
                '[server]\n'
                'auth_mode = "bearer"\n'
                'lifecycle_reconciliation_enabled = true\n'
                'approval_timeout_enabled = true\n'
                'audit_fail_closed = true\n'
            )
            errors, warnings = vtc.check_safety(path)
            self.assertEqual(errors, [])
            self.assertEqual(warnings, [])

    def test_dev_enables_required_control_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate.dev.toml"
            path.write_text(
                '[server]\n'
                'auth_mode = "disabled"\n'
                'store_dsn = "sqlite::memory:"\n'
                'lifecycle_reconciliation_enabled = true\n'
            )
            errors, _warnings = vtc.check_safety(path)
            self.assertTrue(
                any(
                    "lifecycle_reconciliation_enabled" in e and "must not enable" in e
                    for e in errors
                )
            )

    def test_dev_retains_disabled_auth_and_memory_sqlite_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "ferrumgate.dev.toml"
            path.write_text(
                '[server]\n'
                'auth_mode = "disabled"\n'
                'store_dsn = "sqlite::memory:"\n'
            )
            errors, warnings = vtc.check_safety(path)
            self.assertEqual(errors, [])
            self.assertEqual(warnings, [])


if __name__ == "__main__":
    unittest.main()
