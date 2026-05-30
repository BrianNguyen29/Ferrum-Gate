#!/usr/bin/env python3

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = [
    ROOT / "contracts" / "ferrumgate-agent-contract.v1.yaml",
    ROOT / "contracts" / "ferrumgate-integrator-contract.v1.yaml",
    ROOT / "openapi" / "ferrumgate-control-api.v1.yaml",
    ROOT / "schemas" / "jsonschema" / "intent-envelope.json",
    ROOT / "prompts" / "agent_system.md",
]

REQUIRED_SCHEMA_FILES = [
    ROOT / "schemas" / "jsonschema" / "action-proposal.json",
    ROOT / "schemas" / "jsonschema" / "approval-request.json",
    ROOT / "schemas" / "jsonschema" / "capability-lease.json",
    ROOT / "schemas" / "jsonschema" / "common.json",
    ROOT / "schemas" / "jsonschema" / "intent-envelope.json",
    ROOT / "schemas" / "jsonschema" / "provenance-event.json",
    ROOT / "schemas" / "jsonschema" / "rollback-contract.json",
]

CORE_INTENT_FIELDS = {
    "intent_id",
    "principal_id",
    "title",
    "goal",
    "normalized_goal",
    "allowed_outcomes",
    "forbidden_outcomes",
    "resource_scope",
    "risk_tier",
    "approval_mode",
    "default_rollback_class",
    "time_budget",
    "trust_context",
    "status",
    "created_at",
    "expires_at",
}

PHASE_A_DRIFT_FIELDS = ["derived_from_event_ids", "tags"]


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def check_required_files() -> list[str]:
    return [
        f"missing required artifact: {path.relative_to(ROOT)}"
        for path in REQUIRED_FILES
        if not path.exists()
    ]


def check_schema_inventory() -> list[str]:
    return [
        f"missing schema file: {path.relative_to(ROOT)}"
        for path in REQUIRED_SCHEMA_FILES
        if not path.exists()
    ]


def check_intent_schema() -> list[str]:
    errors: list[str] = []
    schema = load_json(ROOT / "schemas" / "jsonschema" / "intent-envelope.json")

    required = set(schema.get("required", []))
    missing_core = sorted(CORE_INTENT_FIELDS - required)
    if missing_core:
        errors.append(
            "intent-envelope.json missing required fields: " + ", ".join(missing_core)
        )

    for key in ["$schema", "$id", "properties"]:
        if key not in schema:
            errors.append(f"intent-envelope.json missing top-level key: {key}")

    properties = schema.get("properties", {})
    for field in PHASE_A_DRIFT_FIELDS:
        if field not in properties:
            errors.append(f"intent-envelope.json missing property: {field}")

    return errors


def extract_intent_envelope_block(openapi_text: str) -> str:
    match = re.search(
        r"(?ms)^\s{4}IntentEnvelope:\n(?P<body>.*?)(?=^\s{4}[A-Z][A-Za-z0-9_]+:\n|\Z)",
        openapi_text,
    )
    return match.group("body") if match else ""


def check_openapi_drift() -> list[str]:
    openapi_text = read_text(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")
    intent_block = extract_intent_envelope_block(openapi_text)
    if not intent_block:
        return ["openapi missing components.schemas.IntentEnvelope block"]

    errors: list[str] = []
    for field in PHASE_A_DRIFT_FIELDS:
        if f"{field}:" not in intent_block:
            errors.append(f"openapi IntentEnvelope missing field: {field}")

    if "$ref: '#/components/schemas/IntentEnvelope'" not in openapi_text:
        errors.append(
            "openapi does not reference components.schemas.IntentEnvelope from endpoints"
        )

    return errors


def check_proto_alignment() -> list[str]:
    proto_text = read_text(ROOT / "crates" / "ferrum-proto" / "src" / "intent.rs")
    schema_props = set(
        load_json(ROOT / "schemas" / "jsonschema" / "intent-envelope.json")
        .get("properties", {})
        .keys()
    )

    errors: list[str] = []
    for field in PHASE_A_DRIFT_FIELDS:
        if f"pub {field}:" in proto_text and field not in schema_props:
            errors.append(
                f"schema drift: ferrum-proto IntentEnvelope has '{field}' but intent-envelope.json does not"
            )

    return errors


def check_contract_structure() -> list[str]:
    agent_contract = read_text(ROOT / "contracts" / "ferrumgate-agent-contract.v1.yaml")
    integrator_contract = read_text(
        ROOT / "contracts" / "ferrumgate-integrator-contract.v1.yaml"
    )

    errors: list[str] = []
    for token in [
        "core_principles:",
        "policy_decisions:",
        "minimum_lineage_chain:",
        "IntentEnvelope:",
        "CapabilityLease:",
        "RollbackContract:",
    ]:
        if token not in agent_contract:
            errors.append(f"agent contract missing section: {token.rstrip(':')}")

    for token in [
        "integration_rules:",
        "required_bindings:",
        "required_checks:",
    ]:
        if token not in integrator_contract:
            errors.append(f"integrator contract missing section: {token.rstrip(':')}")

    return errors


def main() -> int:
    checks = [
        check_required_files,
        check_schema_inventory,
        check_intent_schema,
        check_openapi_drift,
        check_proto_alignment,
        check_contract_structure,
    ]

    errors: list[str] = []
    for check in checks:
        errors.extend(check())

    if errors:
        print("VALIDATION FAILED")
        for error in errors:
            print(f" - {error}")
        return 1

    print("VALIDATION PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
