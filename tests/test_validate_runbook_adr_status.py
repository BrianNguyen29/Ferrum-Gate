import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_runbook_adr_status as vra


class RunbookAdrStatusValidatorTests(unittest.TestCase):
    def test_parse_runbook_adr_rows_extracts_entries(self):
        source = """
| Control | ADR | Status |
|---|---|---|
| WORM export | [ADR 999](../adr/999-test.md) | Accepted (P2-1) |
| Behavioral anomaly | [ADR 010](../adr/010-behavioral-anomaly-detection.md) | Accepted (Phase 1 V1) |
"""
        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(source)
            path = fh.name

        try:
            rows = vra.parse_runbook_adr_rows(Path(path))
        finally:
            Path(path).unlink()

        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0].number, "999")
        self.assertEqual(rows[0].runbook_status, "Accepted (P2-1)")
        self.assertEqual(rows[1].number, "010")

    def test_matching_status_passes(self):
        # Construct a temporary ADR dir and runbook that agree on status.
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            adr_dir = tmp_path / "adr"
            adr_dir.mkdir()
            (adr_dir / "999-test.md").write_text(
                "# ADR 999\n\n## Status\n\nAccepted (P2-1). Implemented.\n",
                encoding="utf-8",
            )
            runbook = tmp_path / "runbook.md"
            runbook.write_text(
                "| Control | ADR | Status |\n|---|---|---|\n"
                "| WORM | [ADR 999](../adr/999-test.md) | Accepted (P2-1) |\n",
                encoding="utf-8",
            )

            orig_runbook = vra.RUNBOOK_PATH
            orig_dir = vra.ADR_DIR
            try:
                vra.RUNBOOK_PATH = runbook
                vra.ADR_DIR = adr_dir
                rc = vra.main()
            finally:
                vra.RUNBOOK_PATH = orig_runbook
                vra.ADR_DIR = orig_dir

        self.assertEqual(rc, 0)

    def test_mismatch_status_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            adr_dir = tmp_path / "adr"
            adr_dir.mkdir()
            (adr_dir / "999-test.md").write_text(
                "# ADR 999\n\n## Status\n\nProposed\n",
                encoding="utf-8",
            )
            runbook = tmp_path / "runbook.md"
            runbook.write_text(
                "| Control | ADR | Status |\n|---|---|---|\n"
                "| WORM | [ADR 999](../adr/999-test.md) | Accepted (P2-1) |\n",
                encoding="utf-8",
            )

            orig_runbook = vra.RUNBOOK_PATH
            orig_dir = vra.ADR_DIR
            try:
                vra.RUNBOOK_PATH = runbook
                vra.ADR_DIR = adr_dir
                rc = vra.main()
            finally:
                vra.RUNBOOK_PATH = orig_runbook
                vra.ADR_DIR = orig_dir

        self.assertEqual(rc, 1)

    def test_missing_adr_doc_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            adr_dir = tmp_path / "adr"
            adr_dir.mkdir()
            runbook = tmp_path / "runbook.md"
            runbook.write_text(
                "| Control | ADR | Status |\n|---|---|---|\n"
                "| WORM | [ADR 999](../adr/999-test.md) | Accepted |\n",
                encoding="utf-8",
            )

            orig_runbook = vra.RUNBOOK_PATH
            orig_dir = vra.ADR_DIR
            try:
                vra.RUNBOOK_PATH = runbook
                vra.ADR_DIR = adr_dir
                rc = vra.main()
            finally:
                vra.RUNBOOK_PATH = orig_runbook
                vra.ADR_DIR = orig_dir

        self.assertEqual(rc, 1)


if __name__ == "__main__":
    unittest.main()
