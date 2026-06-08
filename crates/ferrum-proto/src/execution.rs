use crate::rollback::ActionType;
use crate::{
    CapabilityId, ExecutionId, JsonMap, ProposalId, RiskTier, RollbackClass, RollbackContractId,
    Timestamp,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Normalizes a JSON value by recursively sorting object keys.
fn normalize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut normalized_map: serde_json::Map<String, serde_json::Value> =
                serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                normalized_map.insert(key.clone(), normalize_json_value(&map[key]));
            }
            serde_json::Value::Object(normalized_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_json_value).collect())
        }
        other => other.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionProposal {
    pub proposal_id: ProposalId,
    pub intent_id: crate::IntentId,
    pub step_index: u32,
    pub title: String,
    pub tool_name: String,
    pub server_name: String,
    pub raw_arguments: serde_json::Value,
    pub expected_effect: String,
    pub estimated_risk: RiskTier,
    pub requested_rollback_class: RollbackClass,
    pub taint_inputs: Vec<String>,
    pub metadata: JsonMap,
    pub created_at: Timestamp,
}

/// Explicit adapter/action binding for proposals whose side effects cannot be
/// safely inferred from a tool name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionBinding {
    pub action_type: ActionType,
    pub adapter_key: String,
}

impl ActionBinding {
    /// Parses the compatibility metadata representation used by existing
    /// `ActionProposal` clients:
    /// `metadata.action_type` + `metadata.adapter_key`.
    pub fn from_metadata(metadata: &JsonMap) -> Result<Option<Self>, String> {
        let Some(action_type_value) = metadata.get("action_type") else {
            return Ok(None);
        };
        let Some(adapter_key) = metadata.get("adapter_key").and_then(|value| value.as_str()) else {
            return Err("explicit action binding requires metadata.adapter_key".to_string());
        };
        if adapter_key.trim().is_empty() {
            return Err("explicit action binding adapter_key must not be empty".to_string());
        }

        let action_type: ActionType =
            serde_json::from_value(action_type_value.clone()).map_err(|e| {
                format!(
                    "metadata.action_type is not a valid ferrum_proto::ActionType: {}",
                    e
                )
            })?;
        if matches!(action_type, ActionType::Unknown) {
            return Err("explicit action binding must not use ActionType::Unknown".to_string());
        }

        Ok(Some(Self {
            action_type,
            adapter_key: adapter_key.to_string(),
        }))
    }
}

impl ActionProposal {
    /// Computes the canonical SHA-256 hex digest for an ActionProposal.
    /// Used for I6 approval binding validation.
    ///
    /// Fields included: intent_id, proposal_id, tool_name, server_name,
    /// raw_arguments (normalized recursively by sorted object keys),
    /// expected_effect, estimated_risk, requested_rollback_class.
    ///
    /// Fields excluded: step_index, taint_inputs, metadata, created_at.
    pub fn canonical_action_digest(&self) -> String {
        let mut digest_input = serde_json::Map::new();

        digest_input.insert(
            "intent_id".to_string(),
            serde_json::json!(self.intent_id.to_string()),
        );
        digest_input.insert(
            "proposal_id".to_string(),
            serde_json::json!(self.proposal_id.to_string()),
        );
        digest_input.insert("tool_name".to_string(), serde_json::json!(self.tool_name));
        digest_input.insert(
            "server_name".to_string(),
            serde_json::json!(self.server_name),
        );

        // Normalize raw_arguments recursively by sorting object keys
        let normalized_args = normalize_json_value(&self.raw_arguments);
        digest_input.insert("raw_arguments".to_string(), normalized_args);

        digest_input.insert(
            "expected_effect".to_string(),
            serde_json::json!(self.expected_effect),
        );
        digest_input.insert(
            "estimated_risk".to_string(),
            serde_json::json!(self.estimated_risk),
        );
        digest_input.insert(
            "requested_rollback_class".to_string(),
            serde_json::json!(self.requested_rollback_class),
        );

        // Normalize the entire digest_input map (sorts keys at all levels)
        let normalized_digest = normalize_json_value(&serde_json::Value::Object(digest_input));
        let json_string = serde_json::to_string(&normalized_digest)
            .expect("ActionProposal digest serialization failed");
        let hash = Sha256::digest(json_string.as_bytes());
        hex::encode(hash)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionRecord {
    pub execution_id: ExecutionId,
    pub proposal_id: ProposalId,
    pub intent_id: crate::IntentId,
    pub capability_id: CapabilityId,
    pub rollback_contract_id: Option<RollbackContractId>,
    pub decision: crate::Decision,
    pub state: ExecutionState,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub result_digest: Option<String>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ExecutionState {
    Proposed,
    Authorized,
    Prepared,
    Running,
    AwaitingApproval,
    AwaitingVerification,
    Committed,
    Compensated,
    RolledBack,
    Denied,
    Quarantined,
    Failed,
    Canceled,
}

/// Response type for cancel execution endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelExecutionResponse {
    pub execution_id: ExecutionId,
    pub previous_state: ExecutionState,
    pub current_state: ExecutionState,
    pub canceled_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_action_proposal() -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id: crate::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.read".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({
                "path": "/tmp/test.txt",
                "mode": "r"
            }),
            expected_effect: "read file content".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: vec!["user-input".to_string()],
            metadata: serde_json::from_value(serde_json::json!({"key": "value"})).unwrap(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_canonical_action_digest_determinism() {
        let proposal = make_action_proposal();
        let digest1 = proposal.canonical_action_digest();
        let digest2 = proposal.canonical_action_digest();
        assert_eq!(digest1, digest2, "Digest should be deterministic");
        assert_eq!(digest1.len(), 64, "SHA-256 hex should be 64 characters");
    }

    #[test]
    fn test_action_binding_from_metadata_valid() {
        let metadata: JsonMap = serde_json::from_value(serde_json::json!({
            "action_type": "FileWrite",
            "adapter_key": "fs"
        }))
        .unwrap();

        let binding = ActionBinding::from_metadata(&metadata).unwrap().unwrap();

        assert_eq!(binding.action_type, ActionType::FileWrite);
        assert_eq!(binding.adapter_key, "fs");
    }

    #[test]
    fn test_action_binding_from_metadata_requires_adapter_key() {
        let metadata: JsonMap = serde_json::from_value(serde_json::json!({
            "action_type": "FileWrite"
        }))
        .unwrap();

        let err = ActionBinding::from_metadata(&metadata).unwrap_err();

        assert!(err.contains("metadata.adapter_key"));
    }

    #[test]
    fn test_action_binding_from_metadata_rejects_unknown() {
        let metadata: JsonMap = serde_json::from_value(serde_json::json!({
            "action_type": "Unknown",
            "adapter_key": "noop"
        }))
        .unwrap();

        let err = ActionBinding::from_metadata(&metadata).unwrap_err();

        assert!(err.contains("ActionType::Unknown"));
    }

    #[test]
    fn test_canonical_action_digest_sorted_raw_arguments() {
        let base = make_action_proposal();
        let mut proposal1 = base.clone();
        proposal1.raw_arguments = serde_json::json!({"a": 1, "b": 2, "c": 3});
        let mut proposal2 = base;
        proposal2.raw_arguments = serde_json::json!({"c": 3, "a": 1, "b": 2});
        assert_eq!(
            proposal1.canonical_action_digest(),
            proposal2.canonical_action_digest(),
            "Out-of-order keys should produce same digest"
        );
    }

    #[test]
    fn test_canonical_action_digest_nested_sorted_raw_arguments() {
        let base = make_action_proposal();
        let mut proposal1 = base.clone();
        proposal1.raw_arguments = serde_json::json!({"outer": {"a": 1, "b": 2}, "z": 3});
        let mut proposal2 = base;
        proposal2.raw_arguments = serde_json::json!({"z": 3, "outer": {"b": 2, "a": 1}});
        assert_eq!(
            proposal1.canonical_action_digest(),
            proposal2.canonical_action_digest(),
            "Nested out-of-order keys should produce same digest"
        );
    }

    #[test]
    fn test_canonical_action_digest_included_fields_change() {
        let base = make_action_proposal();

        // Change tool_name
        let mut proposal1 = base.clone();
        proposal1.tool_name = "filesystem.write".to_string();
        assert_ne!(
            base.canonical_action_digest(),
            proposal1.canonical_action_digest(),
            "Changing tool_name should change digest"
        );

        // Change expected_effect
        let mut proposal2 = base.clone();
        proposal2.expected_effect = "different effect".to_string();
        assert_ne!(
            base.canonical_action_digest(),
            proposal2.canonical_action_digest(),
            "Changing expected_effect should change digest"
        );

        // Change raw_arguments
        let mut proposal3 = base.clone();
        proposal3.raw_arguments = serde_json::json!({"path": "/tmp/other.txt"});
        assert_ne!(
            base.canonical_action_digest(),
            proposal3.canonical_action_digest(),
            "Changing raw_arguments should change digest"
        );

        // Change estimated_risk
        let mut proposal4 = base.clone();
        proposal4.estimated_risk = RiskTier::High;
        assert_ne!(
            base.canonical_action_digest(),
            proposal4.canonical_action_digest(),
            "Changing estimated_risk should change digest"
        );

        // Change requested_rollback_class
        let mut proposal5 = base.clone();
        proposal5.requested_rollback_class = RollbackClass::R2Compensatable;
        assert_ne!(
            base.canonical_action_digest(),
            proposal5.canonical_action_digest(),
            "Changing requested_rollback_class should change digest"
        );
    }

    #[test]
    fn test_canonical_action_digest_excluded_fields_no_change() {
        let base = make_action_proposal();
        let base_digest = base.canonical_action_digest();

        // step_index should not affect digest
        let mut proposal1 = base.clone();
        proposal1.step_index = 99;
        assert_eq!(
            base_digest,
            proposal1.canonical_action_digest(),
            "Changing step_index should NOT change digest"
        );

        // taint_inputs should not affect digest
        let mut proposal2 = base.clone();
        proposal2.taint_inputs = vec!["completely-different".to_string()];
        assert_eq!(
            base_digest,
            proposal2.canonical_action_digest(),
            "Changing taint_inputs should NOT change digest"
        );

        // metadata should not affect digest
        let mut proposal3 = base.clone();
        proposal3.metadata =
            serde_json::from_value(serde_json::json!({"different": "metadata"})).unwrap();
        assert_eq!(
            base_digest,
            proposal3.canonical_action_digest(),
            "Changing metadata should NOT change digest"
        );

        // created_at should not affect digest
        let mut proposal4 = base.clone();
        proposal4.created_at = chrono::Utc::now();
        assert_eq!(
            base_digest,
            proposal4.canonical_action_digest(),
            "Changing created_at should NOT change digest"
        );
    }
}
