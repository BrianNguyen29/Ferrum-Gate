import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_adr_catalog as vac


class AdrCatalogValidatorTests(unittest.TestCase):
    # ------------------------------------------------------------------
    # ADR document parsing
    # ------------------------------------------------------------------
    def test_parse_adr_doc_em_dash_and_status_heading(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr = Path(tmp) / "adr"
            adr.mkdir()
            doc = adr / "000-adapter-port.md"
            doc.write_text(
                "# ADR 000 — Adapter Port / Rollback Adapter Seam\n\n"
                "## Status\n\nAccepted\n\n## Context\n\nFoo.\n",
                encoding="utf-8",
            )
            parsed = vac.parse_adr_doc(doc)

        self.assertEqual(parsed.number, "000")
        self.assertEqual(parsed.title, "Adapter Port / Rollback Adapter Seam")
        self.assertEqual(parsed.status, "Accepted")
        self.assertEqual(parsed.status_keyword, "accepted")

    def test_parse_adr_doc_colon_and_inline_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr = Path(tmp) / "adr"
            adr.mkdir()
            doc = adr / "015-shared-nonce-cache.md"
            doc.write_text(
                "> # ADR-015: Shared Nonce Cache for Agent Auth Replay Protection\n"
                ">\n"
                "> **Status:** Accepted\n"
                ">\n"
                "> ## Context\n",
                encoding="utf-8",
            )
            parsed = vac.parse_adr_doc(doc)

        self.assertEqual(parsed.number, "015")
        self.assertEqual(parsed.title, "Shared Nonce Cache for Agent Auth Replay Protection")
        self.assertEqual(parsed.status_keyword, "accepted")

    def test_parse_adr_doc_rejects_h1_number_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "000-adapter-port.md"
            doc.write_text(
                "# ADR 001 — Wrong Title\n\n## Status\n\nAccepted\n",
                encoding="utf-8",
            )
            with self.assertRaises(ValueError):
                vac.parse_adr_doc(doc)

    def test_parse_adr_doc_rejects_missing_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "000-adapter-port.md"
            doc.write_text(
                "# ADR 000 — Title\n\n## Context\n\nFoo.\n",
                encoding="utf-8",
            )
            with self.assertRaises(ValueError):
                vac.parse_adr_doc(doc)

    # ------------------------------------------------------------------
    # Discovery
    # ------------------------------------------------------------------
    def test_discover_adr_docs_skips_noncanonical_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr = Path(tmp) / "adr"
            adr.mkdir()
            (adr / "000-adapter-port.md").write_text(
                "# ADR 000 — Title\n\n## Status\n\nAccepted\n",
                encoding="utf-8",
            )
            (adr / "019-draft.md").write_text(
                "# ADR 019 — Draft\n\n## Status\n\nProposed\n",
                encoding="utf-8",
            )
            (adr / "19-bad.md").write_text("# ADR 19 — Bad\n\n## Status\n\nAccepted\n", encoding="utf-8")
            (adr / "README.md").write_text("# ADR README\n", encoding="utf-8")
            (adr / "notes.txt").write_text("notes", encoding="utf-8")

            docs = vac.discover_adr_docs(adr)

        self.assertEqual(sorted(docs.keys()), ["000", "019"])

    def test_discover_adr_docs_rejects_duplicate_number(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr = Path(tmp) / "adr"
            adr.mkdir()
            (adr / "000-adapter-port.md").write_text(
                "# ADR 000 — Title\n\n## Status\n\nAccepted\n", encoding="utf-8"
            )
            (adr / "000-other.md").write_text(
                "# ADR 000 — Other\n\n## Status\n\nAccepted\n", encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                vac.discover_adr_docs(adr)

    # ------------------------------------------------------------------
    # README index validation
    # ------------------------------------------------------------------
    def _build_catalog(self, tmp: str) -> tuple[Path, Path, dict[str, vac.AdrDoc]]:
        """Create a minimal valid ADR catalog in a temp directory.

        Returns (adr_dir, readme_path, docs).
        """
        tmp_path = Path(tmp)
        adr = tmp_path / "adr"
        adr.mkdir()
        (adr / "000-adapter-port.md").write_text(
            "# ADR 000 — Adapter Port / Rollback Adapter Seam\n\n"
            "## Status\n\nAccepted\n",
            encoding="utf-8",
        )
        (adr / "001-capability-ttl.md").write_text(
            "# ADR 001 — Capability TTL + Single-Use Model\n\n"
            "## Status\n\nAccepted\n",
            encoding="utf-8",
        )
        docs = vac.discover_adr_docs(adr)
        return adr, adr / "README.md", docs

    def test_validate_readme_index_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n"
                "| [001](001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertEqual(errors, [])

    def test_validate_readme_index_fails_zero_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text("# ADRs\n\nNo table here.\n", encoding="utf-8")
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertEqual(errors, ["README index table has no ADR rows"])

    def test_validate_readme_index_fails_duplicate_row(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("duplicate row for ADR 000" in e for e in errors))

    def test_validate_readme_index_fails_missing_doc(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("ADR 001" in e and "missing" in e for e in errors))

    def test_validate_readme_index_fails_extra_row(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n"
                "| [001](001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |\n"
                "| [999](999-missing.md) | Missing | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("unknown ADR 999" in e for e in errors))

    def test_validate_readme_index_fails_title_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Wrong Title | Accepted |\n"
                "| [001](001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("title" in e and "ADR 000" in e for e in errors))

    def test_validate_readme_index_fails_status_keyword_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Proposed |\n"
                "| [001](001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("status" in e and "ADR 000" in e for e in errors))

    def test_validate_readme_index_fails_noncanonical_link_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, readme, docs = self._build_catalog(tmp)
            readme.write_text(
                "# ADRs\n\n| ADR | Title | Status |\n"
                "|-----|-------|--------|\n"
                "| [000](000-adapter-port.md) | Adapter Port / Rollback Adapter Seam | Accepted |\n"
                "| [001](../adr/001-capability-ttl.md) | Capability TTL + Single-Use Model | Accepted |\n",
                encoding="utf-8",
            )
            errors = vac.validate_readme_index(readme, adr, docs)
        self.assertTrue(any("not canonical" in e and "ADR 001" in e for e in errors))

    # ------------------------------------------------------------------
    # ROADMAP validation
    # ------------------------------------------------------------------
    def test_validate_roadmap_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, _readme, docs = self._build_catalog(tmp)
            roadmap = Path(tmp) / "ROADMAP.md"
            roadmap.write_text(
                "# Roadmap\n\n"
                "- See ADR 000 for the adapter port.\n"
                "- See ADR-001 for capability TTL.\n"
                "- [ADR 001](adr/001-capability-ttl.md) link.\n",
                encoding="utf-8",
            )
            errors = vac.validate_roadmap(roadmap, adr, docs)
        self.assertEqual(errors, [])

    def test_validate_roadmap_ignores_noncanonical(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, _readme, docs = self._build_catalog(tmp)
            roadmap = Path(tmp) / "ROADMAP.md"
            roadmap.write_text(
                "# Roadmap\n\n"
                "- This is ADR-like text.\n"
                "- See ADR 19 for old numbering.\n"
                "- ADR 000 is canonical.\n",
                encoding="utf-8",
            )
            errors = vac.validate_roadmap(roadmap, adr, docs)
        # Only the canonical ADR 000 reference should be accepted; ADR-like and ADR 19 ignored.
        self.assertEqual(errors, [])

    def test_validate_roadmap_fails_unknown_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, _readme, docs = self._build_catalog(tmp)
            roadmap = Path(tmp) / "ROADMAP.md"
            roadmap.write_text(
                "# Roadmap\n\n- See ADR 999 for missing.\n",
                encoding="utf-8",
            )
            errors = vac.validate_roadmap(roadmap, adr, docs)
        self.assertTrue(any("unknown ADR 999" in e for e in errors))

    def test_validate_roadmap_fails_bad_link_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            adr, _readme, docs = self._build_catalog(tmp)
            roadmap = Path(tmp) / "ROADMAP.md"
            roadmap.write_text(
                "# Roadmap\n\n- [ADR 000](adr/000-wrong.md)\n",
                encoding="utf-8",
            )
            errors = vac.validate_roadmap(roadmap, adr, docs)
        self.assertTrue(any("does not match canonical" in e for e in errors))

    # ------------------------------------------------------------------
    # Real repository baseline
    # ------------------------------------------------------------------
    def test_main_real_repo_passes(self):
        rc = vac.main()
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
