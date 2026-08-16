import re
import subprocess
import unittest
from pathlib import Path

import tomllib


class InvariantSmokeValidatorTests(unittest.TestCase):
    """Validate scripts/run_invariant_smoke.sh points to real targets and tests."""

    def setUp(self):
        self.repo_root = Path(__file__).resolve().parents[1]
        self.script = self.repo_root / "scripts" / "run_invariant_smoke.sh"
        self.cargo_toml = (
            self.repo_root / "crates" / "ferrum-integration-tests" / "Cargo.toml"
        )
        self.assertTrue(self.script.exists(), f"Script not found: {self.script}")
        self.assertTrue(self.cargo_toml.exists(), f"Cargo.toml not found: {self.cargo_toml}")
        self.script_text = self.script.read_text(encoding="utf-8")

    def _integration_targets(self):
        with self.cargo_toml.open("rb") as fh:
            data = tomllib.load(fh)
        targets = {}
        for entry in data.get("test", []):
            name = entry.get("name")
            path = entry.get("path")
            if name and path:
                targets[name] = self.cargo_toml.parent / path
        return targets

    def _parse_tests(self):
        entries = []
        pattern = re.compile(r'^\s*"([^"]+)"')
        in_array = False
        for line in self.script_text.splitlines():
            if re.match(r"^TESTS=\(", line.strip()):
                in_array = True
                continue
            if in_array and line.strip() == ")":
                break
            if in_array:
                m = pattern.match(line.strip())
                if m:
                    category, package, target_args, test_name = m.group(1).split(":", 3)
                    entries.append(
                        {
                            "category": category,
                            "package": package,
                            "target_args": target_args,
                            "test_name": test_name,
                        }
                    )
        return entries

    def test_zero_match_guard_present(self):
        self.assertIn(
            "0 passed; 0 failed",
            self.script_text,
            "Script must check for zero-test matches",
        )
        self.assertIn(
            "zero tests matched",
            self.script_text,
            "Script must emit a zero-test-match error",
        )
        self.assertIn(
            "exit 1",
            self.script_text,
            "Script must fail on zero-test matches",
        )

    def test_all_entries_point_to_real_targets_and_tests(self):
        integration_targets = self._integration_targets()
        entries = self._parse_tests()
        self.assertTrue(entries, "No smoke entries parsed from script")

        for entry in entries:
            target_args = entry["target_args"]
            test_name = entry["test_name"]
            package = entry["package"]

            if target_args == "--lib":
                # For lib tests, search the package source tree for the test function.
                crate_root = self.repo_root / "crates" / package
                if package == "ferrum-gateway":
                    # The test path is inferred from the test name (capabilities::tests::...).
                    module_part = test_name.split("::tests::")[0]
                    src_file = crate_root / "src" / f"{module_part}.rs"
                    self.assertTrue(
                        src_file.exists(),
                        f"Lib test source not found: {src_file}",
                    )
                    source = src_file.read_text(encoding="utf-8")
                else:
                    # Fallback: search all .rs files in src/.
                    sources = list((crate_root / "src").rglob("*.rs"))
                    source = "\n".join(p.read_text(encoding="utf-8") for p in sources)

                pattern = re.compile(r"\bfn\s+" + re.escape(test_name.split("::")[-1]) + r"\b")
                self.assertTrue(
                    pattern.search(source),
                    f"Lib test {test_name!r} not found in package {package}",
                )
            else:
                m = re.match(r"--test\s+(\S+)", target_args)
                if m is None:
                    self.fail(f"Unexpected target_args format: {target_args!r}")
                target_name = m.group(1)
                self.assertIn(
                    target_name,
                    integration_targets,
                    f"Integration target {target_name!r} not declared in Cargo.toml",
                )
                src_file = integration_targets[target_name]
                self.assertTrue(
                    src_file.exists(),
                    f"Integration test source not found: {src_file}",
                )
                source = src_file.read_text(encoding="utf-8")
                pattern = re.compile(r"\bfn\s+" + re.escape(test_name) + r"\b")
                self.assertTrue(
                    pattern.search(source),
                    f"Test {test_name!r} not found in target {target_name} ({src_file})",
                )

    def test_script_is_executable(self):
        self.assertTrue(
            self.script.stat().st_mode & 0o111,
            "run_invariant_smoke.sh must be executable",
        )

    def test_script_has_bash_shebang(self):
        self.assertTrue(
            self.script_text.startswith("#!/usr/bin/env bash")
            or self.script_text.startswith("#!/bin/bash"),
            "Script must use bash shebang",
        )

    def test_script_no_failure_swallowing(self):
        # The script must fail on errors (set -euo pipefail) and must exit 1
        # when tests fail or zero tests match.
        self.assertIn("set -euo pipefail", self.script_text)
        self.assertIn('exit 1', self.script_text)
        # No top-level command should ignore failure.
        for line in self.script_text.splitlines():
            if line.strip().startswith("cargo test") or line.strip().startswith("make "):
                self.assertNotIn(
                    "|| true",
                    line,
                    f"Critical command line must not swallow failure: {line}",
                )

    def test_make_invariant_smoke_is_blocking_in_ci(self):
        ci_yml = self.repo_root / ".github" / "workflows" / "ci.yml"
        self.assertTrue(ci_yml.exists())
        text = ci_yml.read_text(encoding="utf-8")
        self.assertIn("make invariant-smoke", text)
        for line in text.splitlines():
            if "make invariant-smoke" in line:
                stripped = line.lstrip()
                self.assertFalse(
                    stripped.startswith("-") or "|| true" in stripped,
                    "make invariant-smoke step must not ignore failure",
                )
                return
        self.fail("make invariant-smoke step not found in CI workflow")


if __name__ == "__main__":
    unittest.main()
