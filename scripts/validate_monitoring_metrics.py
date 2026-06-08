#!/usr/bin/env python3
"""Validate monitoring PromQL against metrics emitted by the runtime."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

METRIC_RE = re.compile(r"\bferrumgate_[A-Za-z_:][A-Za-z0-9_:]*\b")
RUNTIME_DECLARATION_RE = re.compile(
    r"# (?:HELP|TYPE) (ferrumgate_[A-Za-z_:][A-Za-z0-9_:]*)"
)
RUNTIME_SAMPLE_RE = re.compile(
    r'"(ferrumgate_[A-Za-z_:][A-Za-z0-9_:]*)(?:\{\{|\s)'
)
EXPR_RE = re.compile(r"^(?P<indent>\s*)expr:\s*(?P<value>.*)$")


def runtime_metrics(source: str) -> set[str]:
    return set(RUNTIME_DECLARATION_RE.findall(source)) | set(
        RUNTIME_SAMPLE_RE.findall(source)
    )


def rule_expressions(source: str) -> list[str]:
    lines = source.splitlines()
    expressions: list[str] = []
    index = 0

    while index < len(lines):
        match = EXPR_RE.match(lines[index])
        if match is None:
            index += 1
            continue

        value = match.group("value").strip()
        if value not in {"|", ">"}:
            expressions.append(value)
            index += 1
            continue

        base_indent = len(match.group("indent"))
        block: list[str] = []
        index += 1
        while index < len(lines):
            line = lines[index]
            if line.strip() and len(line) - len(line.lstrip()) <= base_indent:
                break
            block.append(line.strip())
            index += 1
        expressions.append("\n".join(block))

    return expressions


def dashboard_expressions(source: str) -> list[str]:
    document = json.loads(source)
    expressions: list[str] = []

    def visit(value: object) -> None:
        if isinstance(value, dict):
            expression = value.get("expr")
            if isinstance(expression, str):
                expressions.append(expression)
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(document)
    return expressions


def referenced_metrics(expressions: list[str]) -> set[str]:
    return {
        metric
        for expression in expressions
        for metric in METRIC_RE.findall(expression)
    }


def validate(runtime_path: Path, rules_path: Path, dashboard_path: Path) -> list[str]:
    emitted = runtime_metrics(runtime_path.read_text(encoding="utf-8"))
    rule_refs = referenced_metrics(
        rule_expressions(rules_path.read_text(encoding="utf-8"))
    )
    dashboard_refs = referenced_metrics(
        dashboard_expressions(dashboard_path.read_text(encoding="utf-8"))
    )

    errors: list[str] = []
    if not emitted:
        errors.append(f"no FerrumGate metrics found in runtime source: {runtime_path}")
    if not rule_refs:
        errors.append(f"no FerrumGate metrics found in alert expressions: {rules_path}")

    for location, references in (
        ("alert rules", rule_refs),
        ("Grafana dashboard", dashboard_refs),
    ):
        unknown = sorted(references - emitted)
        if unknown:
            errors.append(
                f"{location} reference metrics not emitted by runtime: "
                + ", ".join(unknown)
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--runtime",
        type=Path,
        default=Path("crates/ferrum-gateway/src/monitoring.rs"),
    )
    parser.add_argument(
        "--rules",
        type=Path,
        default=Path("configs/monitoring/ferrumgate-alerts.yaml"),
    )
    parser.add_argument(
        "--dashboard",
        type=Path,
        default=Path("configs/monitoring/ferrumgate-grafana-dashboard.json"),
    )
    args = parser.parse_args()

    errors = validate(args.runtime, args.rules, args.dashboard)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("Monitoring metric contract is consistent with runtime metrics")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
