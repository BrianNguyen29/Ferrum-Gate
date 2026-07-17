use axum::http::StatusCode;
use ferrum_proto::{
    ActionProposal, ActionType, ApiErrorCode, ExecutionId, ResourceSelector, RollbackClass,
    RollbackTarget,
};
use ferrum_rollback::RollbackService;

use crate::problem::ApiProblem;

/// Infer the rollback class from the requested resource scope.
pub(crate) fn infer_rollback_class(scope: &[ResourceSelector]) -> RollbackClass {
    if scope
        .iter()
        .any(|selector| matches!(selector, ResourceSelector::EmailDraft { .. }))
    {
        RollbackClass::R2Compensatable
    } else {
        RollbackClass::R0NativeReversible
    }
}

fn explicit_action_binding(
    metadata: &ferrum_proto::JsonMap,
) -> Result<Option<(ActionType, String)>, String> {
    ferrum_proto::ActionBinding::from_metadata(metadata)
        .map(|binding| binding.map(|binding| (binding.action_type, binding.adapter_key)))
}

/// Infers the action_type and adapter_key from the tool_name.
/// For FileWrite-related tools (containing "file_write", "write_file", "fs_", etc.),
/// returns ActionType::FileWrite and adapter_key "fs".
/// For sql_mutate, returns ActionType::SqlMutation and adapter_key "sqlite".
/// For maildraft/draft-create/email_draft tools, returns ActionType::MailDraft and adapter_key "maildraft".
/// For git_branch_create, returns ActionType::GitBranchCreate and adapter_key "git".
/// For git_tag_create, returns ActionType::GitTagCreate and adapter_key "git".
/// For git_branch_delete, returns ActionType::GitBranchDelete and adapter_key "git".
/// For git_tag_delete, returns ActionType::GitTagDelete and adapter_key "git".
/// For git_push, returns ActionType::GitPush and adapter_key "git".
/// For git_pull, returns ActionType::GitPull and adapter_key "git".
/// For git_fetch, returns ActionType::GitFetch and adapter_key "git".
/// Unknown mutating tools are rejected unless the proposal carries an explicit
/// binding contract in metadata.action_type + metadata.adapter_key.
pub(crate) fn infer_action_type_and_adapter(
    tool_name: &str,
    metadata: &ferrum_proto::JsonMap,
) -> Result<(ActionType, String), String> {
    if let Some((action_type, adapter_key)) = explicit_action_binding(metadata)? {
        if matches!(action_type, ActionType::EmailSend) {
            return Err(
                "EmailSend is reserved/R3 but not implemented in v1. Use MailDraft for draft operations."
                    .to_string(),
            );
        }
        return Ok((action_type, adapter_key));
    }

    let tool_lower = tool_name.to_lowercase();
    if tool_lower.contains("email_send") {
        return Err(
            "EmailSend is reserved/R3 but not implemented in v1. Use MailDraft for draft operations."
                .to_string(),
        );
    }
    if tool_lower.contains("file_write")
        || tool_lower.contains("write_file")
        || tool_lower.contains("fs_")
        || tool_lower.contains("filesystem.write")
        || tool_lower.contains("filesystem_write")
        || tool_lower.contains("file-mutation")
    {
        Ok((ActionType::FileWrite, "fs".to_string()))
    } else if tool_lower.contains("fs.read")
        || tool_lower.contains("filesystem.read")
        || tool_lower.contains("file_read")
        || tool_lower.contains("read_file")
    {
        Ok((ActionType::McpToolMutation, "noop".to_string()))
    } else if tool_lower.contains("sql_mutate") {
        Ok((ActionType::SqlMutation, "sqlite".to_string()))
    } else if tool_lower.contains("maildraft")
        || tool_lower.contains("draft_create")
        || tool_lower.contains("email_draft")
    {
        Ok((ActionType::MailDraft, "maildraft".to_string()))
    } else if tool_lower.contains("git_branch_create") {
        Ok((ActionType::GitBranchCreate, "git".to_string()))
    } else if tool_lower.contains("git_tag_create") {
        Ok((ActionType::GitTagCreate, "git".to_string()))
    } else if tool_lower.contains("git_branch_delete") {
        Ok((ActionType::GitBranchDelete, "git".to_string()))
    } else if tool_lower.contains("git_tag_delete") {
        Ok((ActionType::GitTagDelete, "git".to_string()))
    } else if tool_lower.contains("git_push") {
        Ok((ActionType::GitPush, "git".to_string()))
    } else if tool_lower.contains("git_pull") {
        Ok((ActionType::GitPull, "git".to_string()))
    } else if tool_lower.contains("git_fetch") {
        Ok((ActionType::GitFetch, "git".to_string()))
    } else if tool_lower.contains("http_post")
        || tool_lower.contains("http_put")
        || tool_lower.contains("http_patch")
        || tool_lower.contains("http_delete")
    {
        Ok((ActionType::HttpMutation, "http".to_string()))
    } else if tool_lower.contains("s3_put")
        || tool_lower.contains("s3putobject")
        || tool_lower.contains("s3.write")
    {
        Ok((ActionType::S3PutObject, "s3".to_string()))
    } else if tool_lower.contains("s3_delete")
        || tool_lower.contains("s3deleteobject")
        || tool_lower.contains("s3.remove")
    {
        Ok((ActionType::S3DeleteObject, "s3".to_string()))
    } else if tool_lower.contains("s3_get")
        || tool_lower.contains("s3getobject")
        || tool_lower.contains("s3.read")
    {
        Ok((ActionType::S3GetObject, "s3".to_string()))
    } else if tool_lower.contains("s3_copy") || tool_lower.contains("s3copyobject") {
        Ok((ActionType::S3CopyObject, "s3".to_string()))
    } else {
        Err(format!(
            "unknown mutating tool '{}' has no explicit action binding",
            tool_name
        ))
    }
}

/// Builds a RollbackPrepareRequest with adapter_key inferred from tool_name.
/// This allows the gateway to select the appropriate adapter based on the proposal's tool.
pub(crate) fn build_prepare_request_for_proposal(
    rollback: &RollbackService,
    intent_id: ferrum_proto::IntentId,
    execution_id: ExecutionId,
    rollback_class: &RollbackClass,
    proposal: &ActionProposal,
    resource_scope: &[ResourceSelector],
) -> Result<ferrum_proto::RollbackPrepareRequest, String> {
    let (action_type, adapter_key) =
        infer_action_type_and_adapter(&proposal.tool_name, &proposal.metadata)?;
    let target = infer_target_from_scope(resource_scope, &action_type);
    let mut request = rollback.build_prepare_request_with_target(
        intent_id,
        proposal.proposal_id,
        execution_id,
        rollback_class.clone(),
        action_type,
        adapter_key,
        target,
    );

    // Merge proposal raw_arguments into metadata for git tools so prepare can
    // validate branch_name/remote_name during prepare (fail-closed).
    if let Some(args) = proposal.raw_arguments.as_object() {
        match request.action_type {
            ActionType::GitBranchCreate => {
                let branch_name = args
                    .get("branch_name")
                    .or_else(|| args.get("branch"))
                    .and_then(|v| v.as_str());
                if let Some(branch) = branch_name {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(branch));
                }
            }
            ActionType::GitPush | ActionType::GitPull | ActionType::GitFetch => {
                if let Some(refspec) = args.get("refspec").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(refspec));
                }
                if let Some(remote) = args.get("remote").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("remote_name".to_string(), serde_json::json!(remote));
                }
            }
            _ => {}
        }
    }

    if matches!(request.action_type, ActionType::SqlMutation) {
        for selector in resource_scope {
            if let ResourceSelector::SqliteDatabase { tables, .. } = selector {
                request
                    .metadata
                    .insert("allowed_tables".to_string(), serde_json::json!(tables));
                break;
            }
        }
    }

    Ok(request)
}

/// If the contract has an HTTP placeholder compensation plan (only url present),
/// enrich it with method, payload, and expected_statuses from contract target
/// and metadata so that http.replay_v1 validation succeeds.
/// Fails closed by leaving the contract unchanged when required data is missing.
pub(crate) fn enrich_http_compensation_if_needed(
    mut contract: ferrum_proto::RollbackContract,
) -> ferrum_proto::RollbackContract {
    if contract.adapter_key != "http" || contract.compensation_plan.len() != 1 {
        return contract;
    }
    let step = &contract.compensation_plan[0];
    if step.operation != "http.replay_v1" || step.args.contains_key("method") {
        return contract;
    }

    let method = match &contract.target {
        ferrum_proto::RollbackTarget::HttpRequest { method, .. } => format!("{:?}", method),
        _ => return contract,
    };

    let payload = contract
        .metadata
        .get("execute_payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let expected_statuses: Vec<u16> = contract
        .metadata
        .get("response_status")
        .and_then(|v| v.as_u64())
        .map(|s| vec![s as u16])
        .unwrap_or_else(|| vec![200]);

    let enriched_step = ferrum_proto::CompensationStep {
        order: step.order,
        adapter_key: step.adapter_key.clone(),
        operation: step.operation.clone(),
        idempotency_key: step.idempotency_key.clone(),
        args: {
            let mut args = step.args.clone();
            args.insert("method".to_string(), serde_json::json!(method));
            args.insert("payload".to_string(), payload);
            args.insert(
                "expected_statuses".to_string(),
                serde_json::json!(expected_statuses),
            );
            args
        },
    };

    contract.compensation_plan = vec![enriched_step];
    contract
}

/// Infers the RollbackTarget from resource_scope.
/// For FilesystemPath selectors, returns RollbackTarget::FilePath with the path.
/// For SqliteDatabase selectors with SqlMutation action, returns RollbackTarget::SqliteTxn.
/// For other selectors, returns Generic fallback.
pub(crate) fn infer_target_from_scope(
    scope: &[ResourceSelector],
    action_type: &ActionType,
) -> RollbackTarget {
    // Only use FilePath target for file-related action types
    let is_file_action = matches!(
        action_type,
        ActionType::FileWrite
            | ActionType::FileDelete
            | ActionType::FileMove
            | ActionType::FileCopy
            | ActionType::FileAppend
            | ActionType::FileChmod
    );

    if is_file_action {
        for selector in scope {
            if let ResourceSelector::FilesystemPath {
                path,
                mode: _,
                content_hash: _,
            } = selector
            {
                return RollbackTarget::FilePath {
                    path: path.clone(),
                    before_hash: None,
                    after_hash: None,
                };
            }
        }
    }

    // SqliteDatabase selector for SqlMutation action type
    if matches!(action_type, ActionType::SqlMutation) {
        for selector in scope {
            if let ResourceSelector::SqliteDatabase {
                db_path,
                tables: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::SqliteTxn {
                    db_path: db_path.clone(),
                    tx_id: format!("tx-{}", uuid::Uuid::new_v4()),
                };
            }
        }
    }

    // EmailDraft selector for MailDraft action type
    if matches!(action_type, ActionType::MailDraft) {
        for selector in scope {
            if let ResourceSelector::EmailDraft {
                recipient_allowlist,
                subject_prefix_allowlist: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::EmailDraft {
                    draft_id: None, // draft_id is set at runtime by execute
                    recipients: recipient_allowlist.clone(),
                };
            }
        }
    }

    // GitRepository selector for git action types (GitBranchCreate, GitTagCreate, etc.)
    let is_git_action = matches!(
        action_type,
        ActionType::GitBranchCreate
            | ActionType::GitTagCreate
            | ActionType::GitBranchDelete
            | ActionType::GitTagDelete
            | ActionType::GitPush
            | ActionType::GitPull
            | ActionType::GitFetch
            | ActionType::GitCommit
    );

    if is_git_action {
        for selector in scope {
            if let ResourceSelector::GitRepository {
                repo_path,
                allowed_refs: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::GitRef {
                    repo_path: repo_path.clone(),
                    before_ref: None,
                    after_ref: None,
                };
            }
        }
    }

    // HttpEndpoint selector for HttpMutation action type
    if matches!(action_type, ActionType::HttpMutation) {
        for selector in scope {
            if let ResourceSelector::HttpEndpoint {
                method,
                base_url,
                path_prefix,
                mode: _,
            } = selector
            {
                let url = if path_prefix.starts_with('/') {
                    format!("{}{}", base_url, path_prefix)
                } else {
                    format!("{}/{}", base_url, path_prefix)
                };
                return RollbackTarget::HttpRequest {
                    method: method.clone(),
                    url,
                    request_digest: String::new(),
                };
            }
        }
    }

    // S3Bucket selector for S3 action types
    let is_s3_action = matches!(
        action_type,
        ActionType::S3PutObject
            | ActionType::S3DeleteObject
            | ActionType::S3GetObject
            | ActionType::S3CopyObject
    );
    if is_s3_action {
        for selector in scope {
            if let ResourceSelector::S3Bucket {
                bucket,
                key_prefix_allowlist: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::S3Object {
                    bucket: bucket.clone(),
                    key: String::new(), // key is provided at runtime via payload
                    version_id: None,
                };
            }
        }
    }

    // Default fallback
    RollbackTarget::Generic {
        namespace: "mcp".to_string(),
        identifier: "tool-call".to_string(),
    }
}

pub(crate) fn parse_execution_id(value: &str) -> Result<ExecutionId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid execution uuid",
        )
    })?;
    Ok(ExecutionId(parsed))
}
