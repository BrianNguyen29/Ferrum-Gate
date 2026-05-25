#!/usr/bin/env python3
"""Tests for validate_evidence_templates.py"""

import sys
import tempfile
from pathlib import Path
import unittest

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_evidence_templates as vet


class TestValidateTemplate(unittest.TestCase):
    def _write_template(self, content: str) -> Path:
        fd, path = tempfile.mkstemp(suffix=".md")
        with open(fd, "w", encoding="utf-8") as f:
            f.write(content)
        return Path(path)

    def test_valid_template_passes(self):
        content = (
            "# TEMPLATE — Test\n"
            "> THIS IS A TEMPLATE\n"
            "## Metadata\n"
            "## Signoff\n"
            "## Non-Claims\n"
            "## Related Docs\n"
        )
        path = self._write_template(content)
        try:
            errors = vet.validate_template(str(path))
            self.assertEqual(errors, [])
        finally:
            path.unlink()

    def test_missing_title_fails(self):
        content = (
            "Not a title\n"
            "> THIS IS A TEMPLATE\n"
            "## Metadata\n"
            "## Signoff\n"
            "## Non-Claims\n"
            "## Related Docs\n"
        )
        path = self._write_template(content)
        try:
            errors = vet.validate_template(str(path))
            self.assertTrue(any("first line does not start with '# TEMPLATE'" in e for e in errors))
        finally:
            path.unlink()

    def test_missing_phrase_fails(self):
        content = (
            "# TEMPLATE — Test\n"
            "## Metadata\n"
            "## Signoff\n"
            "## Non-Claims\n"
            "## Related Docs\n"
        )
        path = self._write_template(content)
        try:
            errors = vet.validate_template(str(path))
            self.assertTrue(any("missing required phrase" in e for e in errors))
        finally:
            path.unlink()

    def test_missing_section_fails(self):
        content = (
            "# TEMPLATE — Test\n"
            "> THIS IS A TEMPLATE\n"
            "## Metadata\n"
            "## Signoff\n"
            "## Non-Claims\n"
        )
        path = self._write_template(content)
        try:
            errors = vet.validate_template(str(path))
            self.assertTrue(any("missing required section" in e for e in errors))
        finally:
            path.unlink()


if __name__ == "__main__":
    unittest.main()
