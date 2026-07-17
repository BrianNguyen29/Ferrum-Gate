#!/usr/bin/env python3

import json
import re
import sys
from pathlib import Path
import yaml


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

METASCHEMA_URI = "https://json-schema.org/draft/2020-12/schema"


# ---------------------------------------------------------------------------
# Schema structure helpers
# ---------------------------------------------------------------------------


def collect_refs(node: object) -> list[str]:
    """Recursively collect all $ref values in a JSON schema."""
    refs: list[str] = []
    if isinstance(node, dict):
        ref = node.get("$ref")
        if isinstance(ref, str):
            refs.append(ref)
        for value in node.values():
            refs.extend(collect_refs(value))
    elif isinstance(node, list):
        for item in node:
            refs.extend(collect_refs(item))
    return refs


def resolve_json_pointer(doc: object, pointer: str) -> object | None:
    """Resolve a JSON Pointer (RFC 6901) against a document."""
    if pointer == "":
        return doc
    if not pointer.startswith("/"):
        return None
    current: object = doc
    for part in pointer[1:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        if isinstance(current, dict):
            if part not in current:
                return None
            current = current[part]
        elif isinstance(current, list):
            try:
                current = current[int(part)]
            except (ValueError, IndexError):
                return None
        else:
            return None
    return current


def _node_path(parts: tuple) -> str:
    if not parts:
        return "root"
    return "/".join(str(p) for p in parts)


def walk_nodes(node: object, path: tuple = ()):
    """Yield every node and its path in a nested JSON structure."""
    yield node, path
    if isinstance(node, dict):
        for key, value in node.items():
            yield from walk_nodes(value, path + (key,))
    elif isinstance(node, list):
        for idx, value in enumerate(node):
            yield from walk_nodes(value, path + (idx,))


def _load_schemas_for_check(
    schema_files: list[Path], errors: list[str]
) -> dict[Path, dict]:
    """Load a list of schema files and append structural errors."""
    schemas: dict[Path, dict] = {}
    for path in schema_files:
        rel = path.relative_to(ROOT)
        try:
            data = load_json(path)
        except Exception as exc:
            errors.append(f"{rel}: JSON parse error: {exc}")
            continue
        if not isinstance(data, dict):
            errors.append(f"{rel}: root is not a JSON object")
            continue
        schemas[path] = data
    return schemas


def _check_ref(
    ref: str, source_path: Path, schema_dir: Path, schemas: dict[Path, dict]
) -> str | None:
    """Validate a single $ref. Returns an error message or None."""
    if ref.startswith("#"):
        target = schemas.get(source_path)
        if target is None:
            return f"cannot resolve local ref {ref}: source schema not loaded"
        pointer = ref[1:]
        if pointer == "":
            return None
        if resolve_json_pointer(target, pointer) is None:
            return f"unresolved local JSON Pointer {ref}"
        return None

    if "#" in ref:
        uri, fragment = ref.split("#", 1)
    else:
        uri, fragment = ref, ""

    if uri.startswith(("http://", "https://")):
        if uri == METASCHEMA_URI:
            return None
        return f"external ref not allowed: {ref}"

    if uri.startswith("file:") or uri.startswith("/") or ".." in Path(uri).parts:
        return f"ref outside schema dir: {ref}"

    target_path = schema_dir / uri
    if not target_path.exists():
        return f"ref target missing: {ref}"

    target_schema = schemas.get(target_path)
    if target_schema is None:
        try:
            target_schema = load_json(target_path)
        except Exception as exc:
            return f"ref target {ref} cannot be parsed: {exc}"

    if fragment == "":
        return None
    if not fragment.startswith("/"):
        return f"ref fragment not a JSON Pointer: {ref}"
    if resolve_json_pointer(target_schema, fragment) is None:
        return f"unresolved JSON Pointer in ref {ref}"
    return None


def _type_of_json_value(value: object) -> str | None:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    return None


def check_schema_structure(schema_files: list[Path] | None = None) -> list[str]:
    """Validate structural integrity of required JSON schemas."""
    if schema_files is None:
        schema_files = REQUIRED_SCHEMA_FILES
    if not schema_files:
        return []

    schema_dir = schema_files[0].parent
    errors: list[str] = []
    schemas = _load_schemas_for_check(schema_files, errors)

    # Root checks: $schema, $id, type, properties, required subset.
    seen_ids: dict[str, Path] = {}
    for path, schema in schemas.items():
        rel = path.relative_to(ROOT)

        if schema.get("$schema") != METASCHEMA_URI:
            errors.append(
                f"{rel}: $schema is not {METASCHEMA_URI}"
            )

        sid = schema.get("$id")
        if not sid:
            errors.append(f"{rel}: missing $id")
        elif not isinstance(sid, str) or not sid.startswith("https://"):
            errors.append(f"{rel}: $id is not a stable absolute URI: {sid}")
        else:
            if sid in seen_ids:
                errors.append(
                    f"{rel}: duplicate $id {sid} (also in {seen_ids[sid].relative_to(ROOT)})"
                )
            else:
                seen_ids[sid] = path

        if schema.get("type") != "object":
            errors.append(f"{rel}: root type is not 'object'")

        properties = schema.get("properties")
        if properties is not None and not isinstance(properties, dict):
            errors.append(f"{rel}: properties is not an object")

        required = schema.get("required")
        if required is not None:
            if not isinstance(required, list):
                errors.append(f"{rel}: required is not an array")
            else:
                if properties is None:
                    errors.append(f"{rel}: required present but properties missing")
                else:
                    for field in required:
                        if field not in properties:
                            errors.append(
                                f"{rel}: required field {field} not in properties"
                            )

    # Reference checks: local pointers and sibling refs must resolve; no external refs.
    for path, schema in schemas.items():
        rel = path.relative_to(ROOT)
        for ref in collect_refs(schema):
            err = _check_ref(ref, path, schema_dir, schemas)
            if err:
                errors.append(f"{rel}: {err}")

    # Enum checks: nonempty, unique, scalar, type-consistent.
    for path, schema in schemas.items():
        rel = path.relative_to(ROOT)
        for node, node_path in walk_nodes(schema):
            if not isinstance(node, dict):
                continue
            enum = node.get("enum")
            if enum is None:
                continue
            path_str = _node_path(node_path)
            if not isinstance(enum, list):
                errors.append(f"{rel}: enum at {path_str} is not an array")
                continue
            if not enum:
                errors.append(f"{rel}: enum at {path_str} is empty")
                continue

            seen_values: set[object] = set()
            duplicates: list[object] = []
            for value in enum:
                key = json.dumps(value, sort_keys=True) if isinstance(value, (dict, list)) else value
                if key in seen_values:
                    duplicates.append(value)
                seen_values.add(key)
            if duplicates:
                errors.append(
                    f"{rel}: enum at {path_str} has duplicate values: {duplicates}"
                )

            non_scalar = [v for v in enum if isinstance(v, (dict, list))]
            if non_scalar:
                errors.append(
                    f"{rel}: enum at {path_str} contains non-scalar values"
                )

            declared_type = node.get("type")
            if declared_type is not None:
                allowed = {declared_type} if isinstance(declared_type, str) else set(declared_type)
                for value in enum:
                    value_type = _type_of_json_value(value)
                    if value_type is None:
                        continue
                    if value_type not in allowed:
                        errors.append(
                            f"{rel}: enum at {path_str} value {value!r} contradicts type {declared_type}"
                        )
                        break

    return errors


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


def load_yaml(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return yaml.safe_load(handle)


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


def check_monitoring_auth() -> list[str]:
    """Verify OpenAPI declares /v1/readyz/deep and /v1/metrics as auth-protected."""
    openapi_text = read_text(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")
    openapi = load_yaml(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")
    errors: list[str] = []

    info = openapi.get("info", {}).get("description", "")
    if "readyz/deep" in info and "metrics" in info:
        if "except `/v1/healthz` and `/v1/readyz`" not in info:
            errors.append(
                "openapi info description does not correctly limit unauthenticated endpoints to healthz/readyz"
            )
    if "single-principal" not in info.lower() and "authactor" not in info.lower():
        errors.append("openapi info description missing single-principal/bearer trust-domain note")

    paths = openapi.get("paths", {})
    for route in ["/v1/readyz/deep", "/v1/metrics"]:
        path_item = paths.get(route, {})
        get_op = path_item.get("get", {})
        security = get_op.get("security")
        if security is None or {"BearerAuth": []} not in [dict(s) for s in security]:
            errors.append(f"openapi {route} is not declared as BearerAuth-protected")
        summary = get_op.get("summary", "")
        if "requires auth" not in summary.lower() and "authenticated" not in summary.lower():
            errors.append(f"openapi {route} summary does not signal auth requirement")

    # healthz and shallow readyz must remain unauthenticated
    for route in ["/v1/healthz", "/v1/readyz"]:
        path_item = paths.get(route, {})
        get_op = path_item.get("get", {})
        security = get_op.get("security")
        if security != []:
            errors.append(f"openapi {route} must remain security: [] (unauthenticated)")

    return errors


def check_recovery_terms() -> list[str]:
    agent_contract = read_text(ROOT / "contracts" / "ferrumgate-agent-contract.v1.yaml")
    integrator_contract = read_text(
        ROOT / "contracts" / "ferrumgate-integrator-contract.v1.yaml"
    )
    openapi_text = read_text(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")
    rollback_schema = load_json(ROOT / "schemas" / "jsonschema" / "rollback-contract.json")

    errors: list[str] = []
    required_agent_terms = [
        "RecoveryRequired",
        "R2Compensatable",
        "R3IrreversibleHighConsequence",
        "HttpMutation",
        "SqlMutation",
        "ErrorRaised",
    ]
    for term in required_agent_terms:
        if term not in agent_contract:
            errors.append(f"agent contract missing recovery term: {term}")

    required_integrator_terms = [
        "RecoveryRequired",
        "R2",
        "R3",
        "owner_only",
        "HttpMutation",
        "SqlMutation",
    ]
    for term in required_integrator_terms:
        if term not in integrator_contract:
            errors.append(f"integrator contract missing recovery term: {term}")

    if "RecoveryRequired" not in openapi_text:
        errors.append("openapi missing RecoveryRequired term")
    if "recovery_required" not in openapi_text:
        errors.append("openapi missing recovery_required response field")
    if "HTTP" not in openapi_text or "SQLite" not in openapi_text:
        errors.append("openapi missing adapter-specific recovery warning context")

    schema_states = rollback_schema.get("properties", {}).get("state", {})
    if "RecoveryRequired" not in schema_states.get("enum", []):
        errors.append("rollback-contract.json missing RecoveryRequired state")
    if "RecoveryRequired" not in schema_states.get("description", ""):
        errors.append("rollback-contract.json state missing RecoveryRequired description")

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


# ---------------------------------------------------------------------------
# Enum drift checks
# ---------------------------------------------------------------------------


def extract_rust_enum_variants(path: Path, enum_name: str) -> set[str]:
    text = read_text(path)
    match = re.search(rf'pub enum {enum_name} \{{(.*?)\}}', text, re.DOTALL)
    if not match:
        return set()
    body = match.group(1)
    variants = set()
    for line in body.splitlines():
        line = line.strip().rstrip(',')
        if not line or line.startswith('//') or line.startswith('#'):
            continue
        if '(' in line:
            line = line.split('(')[0].strip()
        if line:
            variants.add(line)
    return variants


def get_openapi_enum(openapi: dict, schema_name: str, field_name: str) -> set[str] | None:
    schema = openapi.get("components", {}).get("schemas", {}).get(schema_name, {})
    props = schema.get("properties", {})
    field = props.get(field_name, {})
    enum = field.get("enum")
    if enum is None:
        return None
    return set(enum)


def _diff(a: set[str], b: set[str], label: str) -> list[str]:
    errors: list[str] = []
    missing = sorted(a - b)
    extra = sorted(b - a)
    if missing:
        errors.append(f"{label} missing: {', '.join(missing)}")
    if extra:
        errors.append(f"{label} extra: {', '.join(extra)}")
    return errors


def check_enum_drift() -> list[str]:
    errors: list[str] = []
    openapi = load_yaml(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")

    # ExecutionState
    rust = extract_rust_enum_variants(ROOT / "crates" / "ferrum-proto" / "src" / "execution.rs", "ExecutionState")
    openapi_enum = get_openapi_enum(openapi, "ExecutionRecord", "state")
    if openapi_enum is None:
        errors.append("openapi missing enum for ExecutionRecord.state")
    else:
        errors.extend(_diff(rust, openapi_enum, "ExecutionRecord.state"))
    openapi_exec = get_openapi_enum(openapi, "IntentListItem", "exec_state")
    if openapi_exec is None:
        errors.append("openapi missing enum for IntentListItem.exec_state")
    else:
        errors.extend(_diff(rust, openapi_exec, "IntentListItem.exec_state"))

    # CapabilityStatus
    rust = extract_rust_enum_variants(ROOT / "crates" / "ferrum-proto" / "src" / "capability.rs", "CapabilityStatus")
    openapi_enum = get_openapi_enum(openapi, "CapabilityLease", "status")
    if openapi_enum is None:
        errors.append("openapi missing enum for CapabilityLease.status")
    else:
        errors.extend(_diff(rust, openapi_enum, "CapabilityLease.status"))

    # ProvenanceEventKind
    rust = extract_rust_enum_variants(ROOT / "crates" / "ferrum-proto" / "src" / "provenance.rs", "ProvenanceEventKind")
    openapi_enum = get_openapi_enum(openapi, "ProvenanceEvent", "kind")
    if openapi_enum is None:
        errors.append("openapi missing enum for ProvenanceEvent.kind")
    else:
        errors.extend(_diff(rust, openapi_enum, "ProvenanceEvent.kind"))
    openapi_ingest = get_openapi_enum(openapi, "ProvenanceIngestRequest", "kind")
    if openapi_ingest is None:
        errors.append("openapi missing enum for ProvenanceIngestRequest.kind")
    else:
        errors.extend(_diff(rust, openapi_ingest, "ProvenanceIngestRequest.kind"))

    # ActionType
    rust = extract_rust_enum_variants(ROOT / "crates" / "ferrum-proto" / "src" / "rollback.rs", "ActionType")
    openapi_enum = get_openapi_enum(openapi, "RollbackContract", "action_type")
    if openapi_enum is None:
        errors.append("openapi missing enum for RollbackContract.action_type")
    else:
        errors.extend(_diff(rust, openapi_enum, "RollbackContract.action_type"))
    # JSON schema rollback-contract.json
    schema = load_json(ROOT / "schemas" / "jsonschema" / "rollback-contract.json")
    schema_enum = schema.get("properties", {}).get("action_type", {}).get("enum")
    if schema_enum is None:
        errors.append("jsonschema rollback-contract.json missing enum for action_type")
    else:
        errors.extend(_diff(rust, set(schema_enum), "rollback-contract.json action_type"))

    return errors


# ---------------------------------------------------------------------------
# Route coverage check
# ---------------------------------------------------------------------------


def check_route_coverage() -> list[str]:
    errors: list[str] = []
    openapi = load_yaml(ROOT / "openapi" / "ferrumgate-control-api.v1.yaml")
    paths = set(openapi.get("paths", {}).keys())

    expected = {
        "/v1/policy-bundles/simulate",
        "/v1/policy-bundles/{bundle_id}/versions",
        "/v1/policy-bundles/{bundle_id}/diff",
        "/v1/policy-bundles/{bundle_id}/rollback",
        "/v1/admin/tokens",
        "/v1/admin/tokens/{token_id}",
        "/v1/admin/tokens/{token_id}/rotate",
        "/v1/admin/agents",
        "/v1/admin/agents/{agent_id}",
        "/v1/admin/agents/{agent_id}/mfa/enroll",
        "/v1/admin/agents/{agent_id}/mfa/verify",
        "/v1/admin/agents/{agent_id}/mfa/disable",
        "/v1/admin/agents/{agent_id}/mfa/rotate",
        "/v1/admin/agents/{agent_id}/mfa",
        "/v1/admin/agents/{agent_id}/mfa/{mfa_factor_id}",
        "/v1/admin/lifecycle-outbox",
        "/v1/admin/lifecycle-outbox/{outbox_id}",
        "/v1/admin/lifecycle-outbox/{outbox_id}/retry",
        "/v1/admin/lifecycle-outbox/{outbox_id}/resolve",
        "/v1/admin/audit-logs",
        "/v1/admin/audit-logs/export",
        "/v1/admin/audit/verify",
        "/v1/admin/audit/merkle-verify",
        "/v1/admin/audit/merkle-roots",
        "/v1/admin/audit/checkpoints",
        "/v1/admin/audit/checkpoints/{window_start}/verify",
    }

    for route in expected:
        if route not in paths:
            errors.append(f"openapi missing expected route: {route}")

    return errors


def main() -> int:
    checks = [
        check_required_files,
        check_schema_inventory,
        check_schema_structure,
        check_intent_schema,
        check_openapi_drift,
        check_proto_alignment,
        check_contract_structure,
        check_recovery_terms,
        check_monitoring_auth,
        check_enum_drift,
        check_route_coverage,
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
