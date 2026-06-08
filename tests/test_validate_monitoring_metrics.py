import json
import tempfile
import unittest
from pathlib import Path

from scripts.validate_monitoring_metrics import (
    dashboard_expressions,
    rule_expressions,
    validate,
)


class MonitoringMetricContractTests(unittest.TestCase):
    def test_rule_expressions_support_inline_and_block_values(self):
        source = """
rules:
  - alert: Inline
    expr: ferrumgate_store_health_up == 0
  - alert: Block
    expr: |
      rate(ferrumgate_http_requests_total[5m]) > 0
    for: 1m
"""
        self.assertEqual(
            rule_expressions(source),
            [
                "ferrumgate_store_health_up == 0",
                "rate(ferrumgate_http_requests_total[5m]) > 0",
            ],
        )

    def test_dashboard_expressions_find_nested_targets(self):
        source = json.dumps(
            {"panels": [{"targets": [{"expr": "ferrumgate_store_health_up"}]}]}
        )
        self.assertEqual(
            dashboard_expressions(source), ["ferrumgate_store_health_up"]
        )

    def test_validate_rejects_unknown_ferrumgate_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "monitoring.rs"
            rules = root / "alerts.yaml"
            dashboard = root / "dashboard.json"

            runtime.write_text(
                '"# TYPE ferrumgate_store_health_up gauge\\n"',
                encoding="utf-8",
            )
            rules.write_text(
                "rules:\n  - alert: Bad\n"
                "    expr: ferrumgate_missing_total > 0\n",
                encoding="utf-8",
            )
            dashboard.write_text('{"panels": []}', encoding="utf-8")

            errors = validate(runtime, rules, dashboard)

        self.assertEqual(len(errors), 1)
        self.assertIn("ferrumgate_missing_total", errors[0])

    def test_validate_does_not_treat_runtime_comments_as_emitted_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "monitoring.rs"
            rules = root / "alerts.yaml"
            dashboard = root / "dashboard.json"

            runtime.write_text(
                '// old name: ferrumgate_comment_only_total\n'
                '"# HELP ferrumgate_store_health_up Store health\\n"',
                encoding="utf-8",
            )
            rules.write_text(
                "rules:\n  - alert: Bad\n"
                "    expr: ferrumgate_comment_only_total > 0\n",
                encoding="utf-8",
            )
            dashboard.write_text('{"panels": []}', encoding="utf-8")

            errors = validate(runtime, rules, dashboard)

        self.assertEqual(len(errors), 1)
        self.assertIn("ferrumgate_comment_only_total", errors[0])


if __name__ == "__main__":
    unittest.main()
