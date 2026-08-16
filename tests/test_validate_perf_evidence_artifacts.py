#!/usr/bin/env python3
"""Verify that artifact paths referenced by P2 perf evidence metadata exist."""

from collections.abc import Iterator
import json
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_DIR = REPO_ROOT / "baselines" / "evidence"
PERF_EVIDENCE = EVIDENCE_DIR / "perf-evidence-a14150e.json"


def artifact_paths(artifacts: dict) -> Iterator[tuple[str, str]]:
    """Yield (key, relative_path) for every artifact path in the metadata."""
    for key, value in artifacts.items():
        if isinstance(value, str):
            yield key, value
        elif isinstance(value, list):
            for item in value:
                if isinstance(item, str):
                    yield key, item


def validate_artifact_paths(evidence_path: Path) -> None:
    """Raise AssertionError if any referenced artifact path does not exist."""
    with open(evidence_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    artifacts = data.get("artifacts")
    if not artifacts:
        raise AssertionError(f"{evidence_path.name}: missing 'artifacts' section")

    for key, rel_path in artifact_paths(artifacts):
        full_path = REPO_ROOT / rel_path
        if not full_path.exists():
            raise AssertionError(
                f"{evidence_path.name}: artifacts.{key} path missing: {rel_path}"
            )


class TestPerfEvidenceArtifacts(unittest.TestCase):
    """Targeted existence check for P2.1 perf evidence artifact references."""

    def test_perf_evidence_artifacts_exist(self):
        self.assertTrue(
            PERF_EVIDENCE.exists(),
            f"perf evidence file missing: {PERF_EVIDENCE}",
        )
        validate_artifact_paths(PERF_EVIDENCE)

    def test_missing_artifact_path_raises(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory) / "missing-evidence.json"
            evidence.write_text(
                json.dumps(
                    {"artifacts": {"stress_json": "baselines/evidence/does_not_exist.json"}}
                ),
                encoding="utf-8",
            )
            with self.assertRaises(AssertionError) as ctx:
                validate_artifact_paths(evidence)
            self.assertIn("does_not_exist.json", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
