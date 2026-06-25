use super::*;
use ferrum_proto::{ResourceBinding, ResourceMode};

#[test]
fn cancel_only_allows_pre_side_effect_states() {
    for state in [
        ExecutionState::Proposed,
        ExecutionState::Authorized,
        ExecutionState::Prepared,
        ExecutionState::AwaitingApproval,
    ] {
        assert!(
            execution_is_cancelable_pre_side_effect(&state),
            "{state:?} should be cancelable"
        );
    }

    for state in [
        ExecutionState::Running,
        ExecutionState::AwaitingVerification,
        ExecutionState::Committed,
        ExecutionState::Compensated,
        ExecutionState::RolledBack,
        ExecutionState::Denied,
        ExecutionState::Quarantined,
        ExecutionState::Failed,
        ExecutionState::Canceled,
    ] {
        assert!(
            !execution_is_cancelable_pre_side_effect(&state),
            "{state:?} should require terminal/recovery handling instead of cancel"
        );
    }
}

#[test]
fn commit_rejects_all_terminal_execution_states() {
    for state in [
        ExecutionState::Committed,
        ExecutionState::Compensated,
        ExecutionState::RolledBack,
        ExecutionState::Denied,
        ExecutionState::Quarantined,
        ExecutionState::Failed,
        ExecutionState::Canceled,
    ] {
        assert!(
            execution_is_terminal_for_commit(&state),
            "{state:?} should be terminal for commit"
        );
    }

    for state in [
        ExecutionState::Proposed,
        ExecutionState::Authorized,
        ExecutionState::Prepared,
        ExecutionState::Running,
        ExecutionState::AwaitingApproval,
        ExecutionState::AwaitingVerification,
    ] {
        assert!(
            !execution_is_terminal_for_commit(&state),
            "{state:?} should pass terminal guard and be checked by later commit prerequisites"
        );
    }
}

#[test]
fn argument_constraints_accept_matching_payload() {
    let payload = serde_json::json!({
        "name": "release-42",
        "tier": "prod",
        "count": 3,
        "enabled": true,
        "nested": {"value": "present"}
    });
    let constraints = vec![
        ArgumentConstraint::ExactString {
            key: "tier".to_string(),
            value: "prod".to_string(),
        },
        ArgumentConstraint::StringOneOf {
            key: "tier".to_string(),
            values: vec!["prod".to_string(), "staging".to_string()],
        },
        ArgumentConstraint::StringRegex {
            key: "name".to_string(),
            pattern: r"^release-\d+$".to_string(),
        },
        ArgumentConstraint::IntRange {
            key: "count".to_string(),
            min: 1,
            max: 5,
        },
        ArgumentConstraint::BoolExact {
            key: "enabled".to_string(),
            value: true,
        },
        ArgumentConstraint::JsonPointerMustExist {
            pointer: "/nested/value".to_string(),
        },
        ArgumentConstraint::JsonPointerMustNotExist {
            pointer: "/secret".to_string(),
        },
    ];
    assert!(validate_argument_constraints(&payload, &constraints).is_ok());
}

#[test]
fn argument_constraints_reject_mismatch_and_invalid_regex() {
    let payload = serde_json::json!({"name": "release", "count": 9});
    assert!(
        validate_argument_constraints(
            &payload,
            &[ArgumentConstraint::IntRange {
                key: "count".to_string(),
                min: 1,
                max: 5,
            }],
        )
        .is_err()
    );
    assert!(
        validate_argument_constraints(
            &payload,
            &[ArgumentConstraint::StringRegex {
                key: "name".to_string(),
                pattern: "[".to_string(),
            }],
        )
        .is_err()
    );
}

#[test]
fn execute_payload_overrides_proposal_arguments_before_constraint_check() {
    let effective = effective_arguments(
        &serde_json::json!({"path": "/safe/file", "content": "old"}),
        &serde_json::json!({"content": "new"}),
    );
    let constraints = vec![
        ArgumentConstraint::ExactString {
            key: "path".to_string(),
            value: "/safe/file".to_string(),
        },
        ArgumentConstraint::ExactString {
            key: "content".to_string(),
            value: "new".to_string(),
        },
    ];
    assert!(validate_argument_constraints(&effective, &constraints).is_ok());
}

#[test]
fn unknown_mutating_tool_requires_explicit_binding() {
    let metadata = ferrum_proto::JsonMap::new();
    let err = infer_action_type_and_adapter("custom_mutating_tool", &metadata).unwrap_err();
    assert!(err.contains("explicit action binding"));

    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "action_type".to_string(),
        serde_json::json!("McpToolMutation"),
    );
    metadata.insert("adapter_key".to_string(), serde_json::json!("noop"));
    let (action_type, adapter_key) =
        infer_action_type_and_adapter("custom_mutating_tool", &metadata).unwrap();
    assert!(matches!(
        action_type,
        ferrum_proto::ActionType::McpToolMutation
    ));
    assert_eq!(adapter_key, "noop");
}

#[tokio::test]
async fn capability_binding_rejects_tool_mismatch() {
    let service = ferrum_cap::InMemoryCapabilityService::default();
    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();
    let lease = service
        .mint(ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "fs".to_string(),
                tool_name: "fs.read".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: ferrum_proto::JsonMap::new(),
        })
        .await
        .unwrap()
        .lease;
    let proposal = ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "tool mismatch".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "fs".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "write".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: Utc::now(),
    };
    assert!(validate_capability_proposal_binding(&lease, &proposal).is_err());
}

#[test]
fn scope_validation_rejects_prefix_collision_and_traversal() {
    let scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp/ferrum-scope".to_string(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    for path in [
        "/tmp/ferrum-scope-escape/file.txt",
        "/tmp/ferrum-scope/../escape/file.txt",
    ] {
        let binding = vec![ResourceBinding::File {
            path: path.to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }];
        assert!(
            validate_resource_bindings_subset_of_scope(&binding, &scope).is_err(),
            "{path} must be outside scope"
        );
    }
}

#[test]
fn scope_validation_rejects_http_origin_lookalike() {
    let scope = vec![ResourceSelector::HttpEndpoint {
        method: ferrum_proto::HttpMethod::Post,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1".to_string(),
        mode: ResourceMode::Write,
    }];
    let binding = vec![ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Post,
        base_url: "https://api.example.com.evil.test".to_string(),
        path_prefix: "/v1".to_string(),
        header_allowlist: Vec::new(),
        mode: ResourceMode::Write,
    }];
    assert!(validate_resource_bindings_subset_of_scope(&binding, &scope).is_err());
}

#[cfg(unix)]
#[test]
fn scope_validation_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("ferrum-scope-{}", uuid::Uuid::new_v4()));
    let inside = root.join("inside");
    let outside = root.join("outside");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, inside.join("link")).unwrap();

    let scope = vec![ResourceSelector::FilesystemPath {
        path: inside.to_string_lossy().into_owned(),
        mode: ResourceMode::Write,
        content_hash: None,
    }];
    let binding = vec![ResourceBinding::File {
        path: inside
            .join("link/escaped.txt")
            .to_string_lossy()
            .into_owned(),
        mode: ResourceMode::Write,
        required_hash: None,
    }];
    assert!(validate_resource_bindings_subset_of_scope(&binding, &scope).is_err());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn email_send_rejected_via_explicit_action_binding() {
    // EmailSend via explicit metadata.action_type + metadata.adapter_key is rejected
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert("action_type".to_string(), serde_json::json!("EmailSend"));
    metadata.insert("adapter_key".to_string(), serde_json::json!("email"));
    let err = infer_action_type_and_adapter("any_tool", &metadata).unwrap_err();
    assert!(err.contains("EmailSend is reserved/R3"));
}

#[test]
fn email_send_rejected_via_tool_name_path() {
    // EmailSend via tool name pattern matching is also rejected
    let metadata = ferrum_proto::JsonMap::new();
    let err = infer_action_type_and_adapter("email_send", &metadata).unwrap_err();
    assert!(err.contains("EmailSend is reserved/R3"));
}
