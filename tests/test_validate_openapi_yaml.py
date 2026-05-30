#!/usr/bin/env python3
"""Tests for validate_openapi_yaml.py"""

import sys
from pathlib import Path
import unittest

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_openapi_yaml as voy


class TestValidateOpenapiDict(unittest.TestCase):
    def test_valid_dict_passes(self):
        data = {
            "openapi": "3.1.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {"/test": {}},
        }
        self.assertEqual(voy.validate_openapi_dict(data), [])

    def test_missing_top_keys(self):
        data = {
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {"/test": {}},
        }
        errors = voy.validate_openapi_dict(data)
        self.assertTrue(any("missing top-level keys" in e for e in errors))

    def test_missing_info_keys(self):
        data = {
            "openapi": "3.1.0",
            "info": {},
            "paths": {"/test": {}},
        }
        errors = voy.validate_openapi_dict(data)
        self.assertTrue(any("missing info keys" in e for e in errors))

    def test_empty_paths(self):
        data = {
            "openapi": "3.1.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {},
        }
        errors = voy.validate_openapi_dict(data)
        self.assertTrue(any("'paths' is empty" in e for e in errors))

    def test_non_dict_root(self):
        data = ["not", "a", "dict"]
        errors = voy.validate_openapi_dict(data)
        self.assertTrue(any("root is not a mapping" in e for e in errors))

    def test_paths_not_mapping(self):
        data = {
            "openapi": "3.1.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": [],
        }
        errors = voy.validate_openapi_dict(data)
        self.assertTrue(any("'paths' is not a mapping" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
