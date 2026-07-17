import re
import unittest
from pathlib import Path

import yaml


class ActionPinningTests(unittest.TestCase):
    """Validate that all GitHub Actions references are SHA-pinned and rust-cache is restore-only."""

    # owner/repo@40-hex-SHA; inline version comments are ignored by the YAML parser.
    ACTION_SHA_RE = re.compile(r"^[A-Za-z0-9_.\-]+/[A-Za-z0-9_.\-]+@([0-9a-fA-F]{40})$")

    def setUp(self):
        self.repo_root = Path(__file__).resolve().parents[1]
        self.workflow_dir = self.repo_root / ".github" / "workflows"
        self.assertTrue(
            self.workflow_dir.exists(),
            f"Workflow directory not found: {self.workflow_dir}",
        )
        self.workflows = {
            path.name: yaml.safe_load(path.read_text(encoding="utf-8"))
            for path in sorted(self.workflow_dir.glob("*.yml"))
        }

    def _iter_steps(self):
        for name, workflow in self.workflows.items():
            for job_name, job in workflow.get("jobs", {}).items():
                for step in job.get("steps", []):
                    yield name, job_name, step

    def test_all_uses_are_pinned_to_full_sha(self):
        for name, job_name, step in self._iter_steps():
            uses = step.get("uses")
            if not uses:
                continue
            if uses.startswith("docker://"):
                continue
            match = self.ACTION_SHA_RE.match(uses)
            if not match:
                self.fail(
                    f"Workflow {name} job {job_name} step {step.get('name', '?')!r} "
                    f"uses an unpinned action: {uses!r}"
                )
            sha = match.group(1)
            self.assertEqual(
                len(sha),
                40,
                f"Workflow {name} job {job_name} step {step.get('name', '?')!r} "
                f"does not use a full 40-character commit SHA: {uses!r}",
            )

    def test_rust_cache_is_restore_only(self):
        for name, job_name, step in self._iter_steps():
            uses = step.get("uses", "")
            if "Swatinem/rust-cache" not in uses:
                continue
            with_block = step.get("with", {})
            self.assertIn(
                "save-if",
                with_block,
                f"Workflow {name} job {job_name} step {step.get('name', '?')!r} "
                f"must set save-if for restore-only rust-cache behavior",
            )
            self.assertEqual(
                str(with_block["save-if"]).lower(),
                "false",
                f"Workflow {name} job {job_name} step {step.get('name', '?')!r} "
                f"must set save-if to 'false'",
            )


if __name__ == "__main__":
    unittest.main()
