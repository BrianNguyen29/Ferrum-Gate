import unittest
from pathlib import Path

import yaml


class WorkflowPermissionsTests(unittest.TestCase):
    """Validate that GitHub Actions workflows declare explicit, least-privilege permissions."""

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
        self.assertTrue(
            self.workflows,
            f"No workflow files found in {self.workflow_dir}",
        )

    def test_all_workflows_declare_explicit_permissions(self):
        for name, workflow in self.workflows.items():
            self.assertIn(
                "permissions",
                workflow,
                f"Workflow {name} must declare explicit permissions",
            )

    def test_non_release_workflows_are_contents_read_only(self):
        for name, workflow in self.workflows.items():
            if name == "release.yml":
                continue
            perms = workflow.get("permissions")
            self.assertIsInstance(
                perms,
                dict,
                f"Workflow {name} permissions must be a mapping",
            )
            self.assertEqual(
                sorted(perms.keys()),
                ["contents"],
                f"Workflow {name} must only declare the contents permission",
            )
            self.assertEqual(
                perms["contents"],
                "read",
                f"Workflow {name} must only grant contents: read",
            )
            for job_name, job in workflow.get("jobs", {}).items():
                self.assertNotIn(
                    "permissions",
                    job,
                    f"Workflow {name} job {job_name} must not override permissions",
                )

    def test_release_workflow_has_only_required_contents_write(self):
        workflow = self.workflows["release.yml"]
        top_perms = workflow.get("permissions")
        self.assertIsInstance(
            top_perms,
            dict,
            "release.yml top-level permissions must be a mapping",
        )
        self.assertEqual(
            top_perms,
            {"contents": "read"},
            "release.yml default permissions must be contents: read",
        )

        jobs = workflow.get("jobs", {})
        self.assertEqual(
            len(jobs),
            1,
            "release.yml must contain exactly one job",
        )
        for job_name, job in jobs.items():
            self.assertIn(
                "permissions",
                job,
                f"release.yml job {job_name} must declare explicit permissions",
            )
            self.assertEqual(
                job["permissions"],
                {"contents": "write"},
                f"release.yml job {job_name} must only have contents: write",
            )

    def test_no_non_release_workflow_has_job_level_permissions(self):
        for name, workflow in self.workflows.items():
            if name == "release.yml":
                continue
            for job_name, job in workflow.get("jobs", {}).items():
                self.assertNotIn(
                    "permissions",
                    job,
                    f"Workflow {name} job {job_name} must not declare job-level permissions",
                )


if __name__ == "__main__":
    unittest.main()
