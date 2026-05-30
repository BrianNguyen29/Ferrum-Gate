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
            errors = vtc.check_safety(path)
            self.assertTrue(any("auth_mode=disabled" in e for e in errors))
        finally:
            path.unlink()

    def test_prod_with_insecure_bind_fails(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".prod.toml", delete=False) as f:
            f.write("[server]\nallow_insecure_nonlocal_bind = true\n")
            path = Path(f.name)
        try:
            errors = vtc.check_safety(path)
            self.assertTrue(any("allow_insecure_nonlocal_bind=true" in e for e in errors))
        finally:
            path.unlink()

    def test_nonprod_disabled_auth_passes(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".dev.toml", delete=False) as f:
            f.write('[server]\nauth_mode = "disabled"\n')
            path = Path(f.name)
        try:
            self.assertEqual(vtc.check_safety(path), [])
        finally:
            path.unlink()

    def test_prod_bearer_auth_passes(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".prod.toml", delete=False) as f:
            f.write('[server]\nauth_mode = "bearer"\n')
            path = Path(f.name)
        try:
            self.assertEqual(vtc.check_safety(path), [])
        finally:
            path.unlink()


if __name__ == "__main__":
    unittest.main()
