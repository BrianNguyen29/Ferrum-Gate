import copy
import unittest
from pathlib import Path

import yaml


class CIWorkflowValidationTests(unittest.TestCase):
    """Validate that the PR CI workflow keeps the DR smoke gate blocking."""

    def setUp(self):
        self.repo_root = Path(__file__).resolve().parents[1]
        self.ci_yml = self.repo_root / ".github" / "workflows" / "ci.yml"
        self.assertTrue(
            self.ci_yml.exists(),
            f"CI workflow file not found: {self.ci_yml}",
        )
        with self.ci_yml.open("r", encoding="utf-8") as fh:
            self.workflow = yaml.safe_load(fh)

    def _get_validate_job(self, workflow):
        jobs = workflow.get("jobs", {})
        self.assertIn("validate", jobs, "CI workflow must contain a validate job")
        return jobs["validate"]

    def _get_dr_smoke_steps(self, workflow):
        validate_job = self._get_validate_job(workflow)
        steps = validate_job.get("steps", [])
        return [
            step
            for step in steps
            if step.get("name", "").lower().startswith("dr smoke")
            and "make dr-smoke" in step.get("run", "")
        ]

    def _assert_continue_on_error_blocking(self, scope, value):
        if value is not None and value is not False:
            self.fail(
                f"{scope} continue-on-error must be absent or the literal boolean "
                f"False, got {value!r}"
            )

    def _normalize_run(self, run):
        executable = []
        for raw in run.splitlines():
            line = raw.strip().split("#", 1)[0].strip()
            if line:
                executable.append(line)
        return executable

    def _assert_dr_smoke_shell_blocking(self, step):
        run = step.get("run", "")
        lines = self._normalize_run(run)

        self.assertIn(
            "set -euo pipefail",
            lines,
            "DR smoke step must contain `set -euo pipefail`",
        )

        make_lines = [ln for ln in lines if ln == "make dr-smoke"]
        self.assertEqual(
            len(make_lines),
            1,
            "DR smoke step must contain exactly one executable `make dr-smoke` command",
        )

        for line in lines:
            if line in ("set -euo pipefail", "make dr-smoke"):
                continue
            self.fail(
                f"DR smoke step contains forbidden shell line {line!r} "
                "(no `|| true`, `; true`, `if`, or other failure-swallowing/control syntax)"
            )

    def _assert_dr_smoke_blocking(self, workflow):
        dr_smoke_steps = self._get_dr_smoke_steps(workflow)
        self.assertEqual(
            len(dr_smoke_steps),
            1,
            "expected exactly one 'DR smoke' step running `make dr-smoke` in validate job",
        )

        validate_job = self._get_validate_job(workflow)
        step = dr_smoke_steps[0]

        self._assert_continue_on_error_blocking(
            "validate job", validate_job.get("continue-on-error")
        )
        self._assert_continue_on_error_blocking(
            "DR smoke step", step.get("continue-on-error")
        )
        self._assert_dr_smoke_shell_blocking(step)

    def test_validate_job_has_dr_smoke_step(self):
        self._assert_dr_smoke_blocking(self.workflow)

    def test_job_continue_on_error_true_rejected(self):
        workflow = copy.deepcopy(self.workflow)
        workflow["jobs"]["validate"]["continue-on-error"] = True
        with self.assertRaises(AssertionError):
            self._assert_dr_smoke_blocking(workflow)

    def test_step_continue_on_error_true_rejected(self):
        workflow = copy.deepcopy(self.workflow)
        steps = workflow["jobs"]["validate"]["steps"]
        dr_steps = [
            step
            for step in steps
            if step.get("name", "").lower().startswith("dr smoke")
            and "make dr-smoke" in step.get("run", "")
        ]
        dr_steps[0]["continue-on-error"] = True
        with self.assertRaises(AssertionError):
            self._assert_dr_smoke_blocking(workflow)

    def test_job_continue_on_error_expression_rejected(self):
        workflow = copy.deepcopy(self.workflow)
        workflow["jobs"]["validate"]["continue-on-error"] = "${{ true }}"
        with self.assertRaises(AssertionError):
            self._assert_dr_smoke_blocking(workflow)

    def test_step_continue_on_error_expression_rejected(self):
        workflow = copy.deepcopy(self.workflow)
        steps = workflow["jobs"]["validate"]["steps"]
        dr_steps = [
            step
            for step in steps
            if step.get("name", "").lower().startswith("dr smoke")
            and "make dr-smoke" in step.get("run", "")
        ]
        dr_steps[0]["continue-on-error"] = "${{ true }}"
        with self.assertRaises(AssertionError):
            self._assert_dr_smoke_blocking(workflow)

    def test_make_dr_smoke_or_true_rejected(self):
        workflow = copy.deepcopy(self.workflow)
        steps = workflow["jobs"]["validate"]["steps"]
        dr_steps = [
            step
            for step in steps
            if step.get("name", "").lower().startswith("dr smoke")
            and "make dr-smoke" in step.get("run", "")
        ]
        dr_steps[0]["run"] = "set -euo pipefail\nmake dr-smoke || true"
        with self.assertRaises(AssertionError):
            self._assert_dr_smoke_blocking(workflow)

    def test_no_direct_wal_only_step_in_validate_job(self):
        jobs = self.workflow.get("jobs", {})
        validate_job = jobs.get("validate", {})
        steps = validate_job.get("steps", [])

        direct_wal_steps = [
            step
            for step in steps
            if "run_wal_crash_recovery_drill.sh" in step.get("run", "")
            or "make wal-drill" in step.get("run", "")
        ]
        self.assertEqual(
            len(direct_wal_steps),
            0,
            "WAL-only step should be replaced by `make dr-smoke`; no direct WAL invocation allowed",
        )


if __name__ == "__main__":
    unittest.main()
