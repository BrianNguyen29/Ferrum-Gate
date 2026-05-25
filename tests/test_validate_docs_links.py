#!/usr/bin/env python3
"""Tests for validate_docs_links.py"""

import sys
import tempfile
from pathlib import Path
import unittest

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_docs_links as vdl


class TestResolveLink(unittest.TestCase):
    def test_resolve_absolute(self):
        root = Path(__file__).resolve().parents[1]
        source = root / "docs" / "guides" / "test.md"
        resolved = vdl.resolve_link(source, "/docs/README.md")
        self.assertEqual(resolved, root / "docs" / "README.md")

    def test_resolve_relative(self):
        root = Path(__file__).resolve().parents[1]
        source = root / "docs" / "guides" / "test.md"
        resolved = vdl.resolve_link(source, "../README.md")
        self.assertEqual(resolved, (source.parent / "../README.md").resolve())


class TestCheckFile(unittest.TestCase):
    def test_valid_link_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir = Path(tmpdir)
            guide = tmpdir / "guide.md"
            target = tmpdir / "target.md"
            target.write_text("hello")
            guide.write_text("[link](target.md)")
            original_guides_dir = vdl.GUIDES_DIR
            original_docs_dir = vdl.DOCS_DIR
            vdl.GUIDES_DIR = tmpdir
            vdl.DOCS_DIR = tmpdir
            try:
                errors = vdl.check_file(guide)
                self.assertEqual(errors, [])
            finally:
                vdl.GUIDES_DIR = original_guides_dir
                vdl.DOCS_DIR = original_docs_dir

    def test_broken_link_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir = Path(tmpdir)
            guide = tmpdir / "guide.md"
            guide.write_text("[link](missing.md)")
            original_guides_dir = vdl.GUIDES_DIR
            original_docs_dir = vdl.DOCS_DIR
            vdl.GUIDES_DIR = tmpdir
            vdl.DOCS_DIR = tmpdir
            try:
                errors = vdl.check_file(guide)
                self.assertTrue(len(errors) > 0)
                self.assertTrue(any("broken link" in e for e in errors))
            finally:
                vdl.GUIDES_DIR = original_guides_dir
                vdl.DOCS_DIR = original_docs_dir

    def test_external_link_skipped(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir = Path(tmpdir)
            guide = tmpdir / "guide.md"
            guide.write_text("[link](https://example.com)")
            original_guides_dir = vdl.GUIDES_DIR
            original_docs_dir = vdl.DOCS_DIR
            vdl.GUIDES_DIR = tmpdir
            vdl.DOCS_DIR = tmpdir
            try:
                errors = vdl.check_file(guide)
                self.assertEqual(errors, [])
            finally:
                vdl.GUIDES_DIR = original_guides_dir
                vdl.DOCS_DIR = original_docs_dir

    def test_anchor_link_skipped(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir = Path(tmpdir)
            guide = tmpdir / "guide.md"
            guide.write_text("[link](#section)")
            original_guides_dir = vdl.GUIDES_DIR
            original_docs_dir = vdl.DOCS_DIR
            vdl.GUIDES_DIR = tmpdir
            vdl.DOCS_DIR = tmpdir
            try:
                errors = vdl.check_file(guide)
                self.assertEqual(errors, [])
            finally:
                vdl.GUIDES_DIR = original_guides_dir
                vdl.DOCS_DIR = original_docs_dir

    def test_link_outside_docs_skipped(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir = Path(tmpdir)
            guide = tmpdir / "guides" / "guide.md"
            guide.parent.mkdir(parents=True)
            outside = tmpdir / "outside" / "file.md"
            outside.parent.mkdir(parents=True)
            outside.write_text("hello")
            guide.write_text("[link](../outside/file.md)")
            original_guides_dir = vdl.GUIDES_DIR
            original_docs_dir = vdl.DOCS_DIR
            vdl.GUIDES_DIR = tmpdir / "guides"
            vdl.DOCS_DIR = tmpdir / "docs"
            try:
                errors = vdl.check_file(guide)
                self.assertEqual(errors, [])
            finally:
                vdl.GUIDES_DIR = original_guides_dir
                vdl.DOCS_DIR = original_docs_dir


if __name__ == "__main__":
    unittest.main()
