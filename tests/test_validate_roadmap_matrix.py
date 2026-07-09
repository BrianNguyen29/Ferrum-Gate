import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import validate_roadmap_matrix as vrm


class RoadmapMatrixValidatorTests(unittest.TestCase):
    def test_parse_roadmap_matrix_extracts_rows(self):
        source = """
| Feature | Level | Status | Notes |
|---|---|---|---|
| Core | P4 | Stable | CI-tested. |
| Anomaly | P2 | Experimental | in-memory advisory opt-in. |
| AI ML | P0 | Not implemented | deferred. |
"""
        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(source)
            path = fh.name

        try:
            rows = vrm.parse_roadmap_matrix(Path(path))
        finally:
            Path(path).unlink()

        self.assertEqual(len(rows), 3)
        self.assertEqual(rows[0].name, "Core")
        self.assertEqual(rows[0].level, 4)
        self.assertEqual(rows[1].name, "Anomaly")
        self.assertEqual(rows[1].level, 2)

    def test_p3_plus_with_advisory_phrase_warns_but_exits_zero(self):
        source = """
| Feature | Level | Status | Notes |
|---|---|---|---|
| Widget | P3 | Beta | advisory opt-in in-memory. |
"""
        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(source)
            path = fh.name

        original = vrm.ROADMAP_PATH
        try:
            vrm.ROADMAP_PATH = Path(path)
            rc = vrm.main()
        finally:
            vrm.ROADMAP_PATH = original
            Path(path).unlink()

        self.assertEqual(rc, 0)

    def test_p2_with_advisory_phrase_does_not_warn(self):
        source = """
| Feature | Level | Status | Notes |
|---|---|---|---|
| Widget | P2 | Experimental | advisory opt-in in-memory. |
"""
        import io
        from unittest.mock import patch

        with tempfile.NamedTemporaryFile("w", delete=False) as fh:
            fh.write(source)
            path = fh.name

        original = vrm.ROADMAP_PATH
        try:
            vrm.ROADMAP_PATH = Path(path)
            with patch("sys.stdout", new=io.StringIO()) as captured:
                rc = vrm.main()
                output = captured.getvalue()
        finally:
            vrm.ROADMAP_PATH = original
            Path(path).unlink()

        self.assertEqual(rc, 0)
        self.assertNotIn("WARN", output)


if __name__ == "__main__":
    unittest.main()
