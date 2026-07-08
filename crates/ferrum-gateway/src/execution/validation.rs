use ferrum_proto::{ActionProposal, ArgumentConstraint, CapabilityLease};
use regex::Regex;

/// Verify that a minted capability lease matches the action proposal it is being used for.
pub(crate) fn validate_capability_proposal_binding(
    lease: &CapabilityLease,
    proposal: &ActionProposal,
) -> Result<(), String> {
    if lease.proposal_id != proposal.proposal_id {
        return Err("capability proposal_id does not match proposal".to_string());
    }
    if lease.intent_id != proposal.intent_id {
        return Err("capability intent_id does not match proposal intent_id".to_string());
    }
    if lease.tool_binding.server_name != proposal.server_name
        || lease.tool_binding.tool_name != proposal.tool_name
    {
        return Err("capability tool_binding does not match proposal tool".to_string());
    }
    Ok(())
}

/// Look up a value in a JSON payload by either a top-level key or a JSON pointer.
fn argument_value<'a>(payload: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if key.starts_with('/') {
        payload.pointer(key)
    } else {
        payload.as_object().and_then(|object| object.get(key))
    }
}

/// Validate that a JSON payload satisfies every argument constraint on the capability.
pub(crate) fn validate_argument_constraints(
    payload: &serde_json::Value,
    constraints: &[ArgumentConstraint],
) -> Result<(), String> {
    for constraint in constraints {
        match constraint {
            ArgumentConstraint::ExactString { key, value } => {
                let actual = argument_value(payload, key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("argument constraint requires string at '{key}'"))?;
                if actual != value {
                    return Err(format!("argument constraint ExactString failed at '{key}'"));
                }
            }
            ArgumentConstraint::StringOneOf { key, values } => {
                let actual = argument_value(payload, key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("argument constraint requires string at '{key}'"))?;
                if !values.iter().any(|allowed| allowed == actual) {
                    return Err(format!("argument constraint StringOneOf failed at '{key}'"));
                }
            }
            ArgumentConstraint::StringRegex { key, pattern } => {
                let actual = argument_value(payload, key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("argument constraint requires string at '{key}'"))?;
                let regex = Regex::new(pattern)
                    .map_err(|e| format!("invalid StringRegex constraint at '{key}': {e}"))?;
                if !regex.is_match(actual) {
                    return Err(format!("argument constraint StringRegex failed at '{key}'"));
                }
            }
            ArgumentConstraint::IntRange { key, min, max } => {
                if min > max {
                    return Err(format!("invalid IntRange constraint at '{key}': min > max"));
                }
                let actual = argument_value(payload, key)
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| format!("argument constraint requires integer at '{key}'"))?;
                if actual < *min || actual > *max {
                    return Err(format!("argument constraint IntRange failed at '{key}'"));
                }
            }
            ArgumentConstraint::BoolExact { key, value } => {
                let actual = argument_value(payload, key)
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| format!("argument constraint requires boolean at '{key}'"))?;
                if actual != *value {
                    return Err(format!("argument constraint BoolExact failed at '{key}'"));
                }
            }
            ArgumentConstraint::JsonPointerMustExist { pointer } => {
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return Err(format!("invalid JSON pointer constraint '{pointer}'"));
                }
                if payload.pointer(pointer).is_none() {
                    return Err(format!(
                        "argument constraint JsonPointerMustExist failed at '{pointer}'"
                    ));
                }
            }
            ArgumentConstraint::JsonPointerMustNotExist { pointer } => {
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return Err(format!("invalid JSON pointer constraint '{pointer}'"));
                }
                if payload.pointer(pointer).is_some() {
                    return Err(format!(
                        "argument constraint JsonPointerMustNotExist failed at '{pointer}'"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Merge execute-time payload arguments over the proposal's default arguments.
#[cfg(test)]
pub(crate) fn effective_arguments(
    proposal_arguments: &serde_json::Value,
    execute_payload: &serde_json::Value,
) -> serde_json::Value {
    match (proposal_arguments, execute_payload) {
        (serde_json::Value::Object(proposal), serde_json::Value::Object(payload)) => {
            let mut effective = proposal.clone();
            effective.extend(payload.clone());
            serde_json::Value::Object(effective)
        }
        (_, serde_json::Value::Null) => proposal_arguments.clone(),
        _ => execute_payload.clone(),
    }
}
