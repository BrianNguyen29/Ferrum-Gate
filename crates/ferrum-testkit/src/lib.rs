//! Minimal test fixtures and assertion helpers for FerrumGate workspace tests.
//!
//! ## Fixtures
//!
//! `IntentFixture`, `ProposalFixture`, and `ApprovalFixture` are builder-style
//! fixtures that produce minimal, valid protocol values. They default to a
//! deterministic `test_now()` timestamp and common placeholder values; override
//! individual fields with `with_*` methods.
//!
//! ## Gateway harness
//!
//! Enable the optional `gateway` feature for an opt-in `SqliteGateway` harness
//! that wires an in-memory SQLite store to a `GatewayRuntime`.

use ferrum_proto::{
    CapabilityMintRequest, Decision, EvaluateProposalResponse, IntentCompileRequest, JsonMap,
    RiskTier, ToolBinding,
};

mod approval_fixture;
mod intent_fixture;
mod proposal_fixture;

#[cfg(feature = "gateway")]
pub mod gateway;

pub use approval_fixture::ApprovalFixture;
pub use intent_fixture::IntentFixture;
pub use proposal_fixture::ProposalFixture;

/// Returns a deterministic UTC timestamp for tests.
///
/// Fixtures use this value as their default `now` so records are reproducible
/// across runs. Tests that require wall-clock time (e.g. approvals that must
/// not be expired) can override `with_now(chrono::Utc::now())` explicitly.
pub fn test_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00")
        .expect("fixed test timestamp is valid")
        .with_timezone(&chrono::Utc)
}

/// Builds a minimal `IntentCompileRequest` for testing.
///
/// Uses a placeholder principal. Override fields as needed.
pub fn sample_intent_compile_request() -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: sample_intent_title().to_string(),
        goal: "Draft an invoice email".to_string(),
        agent_plan_summary: None,
        trusted_context: JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Medium),
        approval_mode: None,
        metadata: JsonMap::new(),
    }
}

/// Returns a sample intent title for use in tests.
pub fn sample_intent_title() -> &'static str {
    "Create invoice email draft"
}

/// Builds a minimal `EvaluateProposalResponse` with `Allow` decision for testing.
pub fn sample_proposal_allow_response() -> EvaluateProposalResponse {
    EvaluateProposalResponse {
        decision: Decision::Allow,
        reason: "policy matched test rule".to_string(),
        matched_rule_ids: vec!["test-rule-1".to_string()],
        warnings: vec![],
    }
}

/// Builds a minimal `CapabilityMintRequest` for testing.
///
/// Uses a 60-second TTL. Override fields as needed.
pub fn sample_capability_mint_request(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
) -> CapabilityMintRequest {
    CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: JsonMap::new(),
    }
}

/// Asserts that `haystack` contains all keys/values from `needle` at the top level.
///
/// Panics with a descriptive message if any key is missing or the value does not match.
///
/// # Example
///
/// ```
/// use ferrum_testkit::assert_json_contains;
/// use serde_json::json;
///
/// let response = json!({"status": "ok", "count": 42});
/// assert_json_contains(&response, &json!({"status": "ok"}));
/// ```
pub fn assert_json_contains(haystack: &serde_json::Value, needle: &serde_json::Value) {
    let hay_obj = haystack
        .as_object()
        .expect("haystack must be a JSON object");
    let needle_obj = needle.as_object().expect("needle must be a JSON object");

    for (key, expected) in needle_obj {
        let actual = hay_obj
            .get(key)
            .unwrap_or_else(|| panic!("missing key '{}' in haystack", key));
        assert_eq!(
            actual, expected,
            "value mismatch for key '{}': got {:?}, expected {:?}",
            key, actual, expected
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_intent_title() {
        assert_eq!(sample_intent_title(), "Create invoice email draft");
    }

    #[test]
    fn test_sample_intent_compile_request() {
        let req = sample_intent_compile_request();
        assert_eq!(req.title, "Create invoice email draft");
        assert_eq!(req.requested_risk_tier, Some(RiskTier::Medium));
    }

    #[test]
    fn test_sample_proposal_allow_response() {
        let resp = sample_proposal_allow_response();
        assert!(matches!(resp.decision, Decision::Allow));
    }

    #[test]
    fn test_sample_capability_mint_request() {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let req = sample_capability_mint_request(intent_id, proposal_id);
        assert_eq!(req.requested_ttl_secs, 60);
        assert_eq!(req.tool_binding.tool_name, "test-tool");
    }

    #[test]
    fn test_assert_json_contains_passes() {
        let haystack = serde_json::json!({"status": "ok", "count": 42});
        let needle = serde_json::json!({"status": "ok"});
        assert_json_contains(&haystack, &needle);
    }

    #[test]
    #[should_panic(expected = "missing key 'status' in haystack")]
    fn test_assert_json_contains_missing_key() {
        let haystack = serde_json::json!({"count": 42});
        let needle = serde_json::json!({"status": "ok"});
        assert_json_contains(&haystack, &needle);
    }

    #[test]
    #[should_panic(expected = "value mismatch for key 'status'")]
    fn test_assert_json_contains_value_mismatch() {
        let haystack = serde_json::json!({"status": "error"});
        let needle = serde_json::json!({"status": "ok"});
        assert_json_contains(&haystack, &needle);
    }
}
