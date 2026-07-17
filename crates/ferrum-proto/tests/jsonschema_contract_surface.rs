//! JSON Schema contract surface gate — Slice 3.
//!
//! This test is a limited, Rust-derived public-contract surface check. It uses
//! `schemars::schema_for!` on the compiled proto types and compares a tiny
//! stable projection of the generated schemas with the curated checked-in JSON
//! Schema files:
//!
//! - root object property names
//! - root object required property sets
//! - explicitly bound string enum value sets
//!
//! It deliberately does NOT compare descriptions, titles, ordering, format,
//! additionalProperties, nested full trees, `$id`, draft version, or the
//! `definitions` vs `$defs` representation. The checked-in schemas remain the
//! curated external contracts; full generated-schema authority is deferred.

use ferrum_proto::{
    ActionProposal, ActionType, ActorType, ApprovalMode, ApprovalRequest, ApprovalState,
    CapabilityLease, CapabilityStatus, CheckType, EffectType, HttpMethod, IntentEnvelope,
    IntentStatus, ProvenanceEdgeType, ProvenanceEvent, ProvenanceEventKind, ResourceMode, RiskTier,
    RollbackClass, RollbackContract, RollbackState, SensitivityLabel, TrustLabel,
};
use schemars::schema_for;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Maps a Rust root type to a curated schema file under `schemas/jsonschema/`.
struct RootBinding {
    type_name: &'static str,
    schema_file: &'static str,
}

/// Maps a Rust enum type to a curated schema file and a JSON pointer where the
/// string enum is defined.
struct EnumBinding {
    type_name: &'static str,
    schema_file: &'static str,
    pointer: &'static str,
}

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("schemas/jsonschema")
}

fn load_curated_schema(file: &str) -> Value {
    let path = schemas_dir().join(file);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read curated schema {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse curated schema {}: {}", path.display(), e))
}

fn generated_schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("generated schema serialization")
}

fn property_names(schema: &Value) -> BTreeSet<String> {
    schema["properties"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn required_set(schema: &Value) -> BTreeSet<String> {
    schema["required"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn enum_set(schema: &Value) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    if let Some(arr) = schema["enum"].as_array() {
        result.extend(arr.iter().filter_map(|v| v.as_str().map(String::from)));
    }
    // Some generated enums are represented as `oneOf` over per-variant enum
    // subschemas (e.g., when doc comments differ). Collect all enum strings
    // across the `oneOf` branches without comparing descriptions.
    if let Some(one_of) = schema["oneOf"].as_array() {
        for sub in one_of {
            result.extend(enum_set(sub));
        }
    }
    result
}

/// Resolve a generated schema node if it is a local `$ref`. Supports both the
/// `definitions` style used by schemars 0.8 and the newer `$defs` style.
fn resolve_generated_local_ref(generated: &Value, node: &Value) -> Value {
    let r#ref = match node.get("$ref").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => return node.clone(),
    };

    if let Some(key) = r#ref.strip_prefix("#/definitions/") {
        let def = generated["definitions"].get(key).unwrap_or_else(|| {
            panic!(
                "dead/unresolvable generated $ref: {} (definitions missing)",
                r#ref
            )
        });
        return resolve_generated_local_ref(generated, def);
    }

    if let Some(key) = r#ref.strip_prefix("#/$defs/") {
        let def = generated["$defs"].get(key).unwrap_or_else(|| {
            panic!(
                "dead/unresolvable generated $ref: {} ($defs missing)",
                r#ref
            )
        });
        return resolve_generated_local_ref(generated, def);
    }

    panic!("unexpected generated $ref (not a local pointer): {}", r#ref);
}

fn check_root_surface<T: schemars::JsonSchema>(binding: &RootBinding) {
    let generated = generated_schema::<T>();
    let curated = load_curated_schema(binding.schema_file);

    let gen_props = property_names(&generated);
    let cur_props = property_names(&curated);
    let gen_required = required_set(&generated);
    let cur_required = required_set(&curated);

    let missing_in_curated: BTreeSet<String> = gen_props.difference(&cur_props).cloned().collect();
    let extra_in_curated: BTreeSet<String> = cur_props.difference(&gen_props).cloned().collect();

    if !missing_in_curated.is_empty() || !extra_in_curated.is_empty() {
        panic!(
            "root property set drift for {} ({}):\n  generated: {:?}\n  curated:   {:?}\n  missing in curated: {:?}\n  extra in curated:   {:?}",
            binding.type_name,
            binding.schema_file,
            gen_props,
            cur_props,
            missing_in_curated,
            extra_in_curated
        );
    }

    let required_missing: BTreeSet<String> =
        gen_required.difference(&cur_required).cloned().collect();
    let required_extra: BTreeSet<String> =
        cur_required.difference(&gen_required).cloned().collect();

    if !required_missing.is_empty() || !required_extra.is_empty() {
        panic!(
            "required set drift for {} ({}):\n  generated: {:?}\n  curated:   {:?}\n  missing in curated: {:?}\n  extra in curated:   {:?}",
            binding.type_name,
            binding.schema_file,
            gen_required,
            cur_required,
            required_missing,
            required_extra
        );
    }
}

fn check_enum_surface<T: schemars::JsonSchema>(binding: &EnumBinding) {
    let generated = generated_schema::<T>();
    let curated = load_curated_schema(binding.schema_file);

    let target = curated.pointer(binding.pointer).unwrap_or_else(|| {
        panic!(
            "dead/unresolvable binding: pointer {} not found in {}",
            binding.pointer, binding.schema_file
        )
    });

    let resolved = resolve_generated_local_ref(&generated, &generated);
    let gen_enum = enum_set(&resolved);
    let cur_enum = enum_set(target);

    let missing: BTreeSet<String> = gen_enum.difference(&cur_enum).cloned().collect();
    let extra: BTreeSet<String> = cur_enum.difference(&gen_enum).cloned().collect();

    if !missing.is_empty() || !extra.is_empty() {
        panic!(
            "enum value drift for {} at {} ({}):\n  generated: {:?}\n  curated:   {:?}\n  missing in curated: {:?}\n  extra in curated:   {:?}",
            binding.type_name,
            binding.pointer,
            binding.schema_file,
            gen_enum,
            cur_enum,
            missing,
            extra
        );
    }
}

#[test]
fn jsonschema_contract_surface_gate() {
    // Root object bindings.
    check_root_surface::<IntentEnvelope>(&RootBinding {
        type_name: "IntentEnvelope",
        schema_file: "intent-envelope.json",
    });
    check_root_surface::<ActionProposal>(&RootBinding {
        type_name: "ActionProposal",
        schema_file: "action-proposal.json",
    });
    check_root_surface::<ApprovalRequest>(&RootBinding {
        type_name: "ApprovalRequest",
        schema_file: "approval-request.json",
    });
    check_root_surface::<CapabilityLease>(&RootBinding {
        type_name: "CapabilityLease",
        schema_file: "capability-lease.json",
    });
    check_root_surface::<ProvenanceEvent>(&RootBinding {
        type_name: "ProvenanceEvent",
        schema_file: "provenance-event.json",
    });
    check_root_surface::<RollbackContract>(&RootBinding {
        type_name: "RollbackContract",
        schema_file: "rollback-contract.json",
    });

    // Explicitly bound string enum bindings. Includes RecoveryRequired-bearing
    // RollbackState and the enums reachable from the root schemas.
    check_enum_surface::<IntentStatus>(&EnumBinding {
        type_name: "IntentStatus",
        schema_file: "intent-envelope.json",
        pointer: "/properties/status",
    });
    check_enum_surface::<EffectType>(&EnumBinding {
        type_name: "EffectType",
        schema_file: "intent-envelope.json",
        pointer: "/properties/allowed_outcomes/items/properties/effect_type",
    });
    check_enum_surface::<RiskTier>(&EnumBinding {
        type_name: "RiskTier",
        schema_file: "common.json",
        pointer: "/definitions/RiskTier",
    });
    check_enum_surface::<ApprovalMode>(&EnumBinding {
        type_name: "ApprovalMode",
        schema_file: "common.json",
        pointer: "/definitions/ApprovalMode",
    });
    check_enum_surface::<RollbackClass>(&EnumBinding {
        type_name: "RollbackClass",
        schema_file: "common.json",
        pointer: "/definitions/RollbackClass",
    });
    check_enum_surface::<ResourceMode>(&EnumBinding {
        type_name: "ResourceMode",
        schema_file: "common.json",
        pointer: "/definitions/ResourceMode",
    });
    check_enum_surface::<HttpMethod>(&EnumBinding {
        type_name: "HttpMethod",
        schema_file: "common.json",
        pointer: "/definitions/HttpMethod",
    });
    check_enum_surface::<TrustLabel>(&EnumBinding {
        type_name: "TrustLabel",
        schema_file: "common.json",
        pointer: "/definitions/TrustLabel",
    });
    check_enum_surface::<SensitivityLabel>(&EnumBinding {
        type_name: "SensitivityLabel",
        schema_file: "common.json",
        pointer: "/definitions/SensitivityLabel",
    });
    check_enum_surface::<ActorType>(&EnumBinding {
        type_name: "ActorType",
        schema_file: "provenance-event.json",
        pointer: "/properties/actor/properties/actor_type",
    });
    check_enum_surface::<ProvenanceEventKind>(&EnumBinding {
        type_name: "ProvenanceEventKind",
        schema_file: "provenance-event.json",
        pointer: "/properties/kind",
    });
    check_enum_surface::<ProvenanceEdgeType>(&EnumBinding {
        type_name: "ProvenanceEdgeType",
        schema_file: "provenance-event.json",
        pointer: "/properties/parent_edges/items/properties/edge_type",
    });
    check_enum_surface::<ApprovalState>(&EnumBinding {
        type_name: "ApprovalState",
        schema_file: "approval-request.json",
        pointer: "/properties/state",
    });
    check_enum_surface::<CapabilityStatus>(&EnumBinding {
        type_name: "CapabilityStatus",
        schema_file: "capability-lease.json",
        pointer: "/properties/status",
    });
    check_enum_surface::<ActionType>(&EnumBinding {
        type_name: "ActionType",
        schema_file: "rollback-contract.json",
        pointer: "/properties/action_type",
    });
    check_enum_surface::<RollbackState>(&EnumBinding {
        type_name: "RollbackState",
        schema_file: "rollback-contract.json",
        pointer: "/properties/state",
    });
    check_enum_surface::<CheckType>(&EnumBinding {
        type_name: "CheckType",
        schema_file: "rollback-contract.json",
        pointer: "/properties/prepare_checks/items/properties/check_type",
    });
}
