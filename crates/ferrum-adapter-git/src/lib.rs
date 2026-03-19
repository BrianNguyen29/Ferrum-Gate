// Test-focused local git adapter primitives for rollback evidence.
// This slice captures refs from a local repository and can reset back to a
// previously recorded ref. Full git mutation execution is intentionally out of
// scope here.

use async_trait::async_trait;
use ferrum_proto::{JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use std::path::Path;
use std::process::Command;

pub const ADAPTER_KIND: &str = "ferrum-adapter-git";
pub const ADAPTER_KEY: &str = "git";

pub struct GitRollbackAdapter {
    key: &'static str,
}

impl GitRollbackAdapter {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }
}

#[async_trait]
impl RollbackAdapter for GitRollbackAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        let repo_path = extract_repo_path_from_request(request)?;
        let before_ref = git_head(&repo_path)?;

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("before_ref".to_string(), serde_json::json!(before_ref));

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        let repo_path = extract_repo_path_from_contract(contract)?;
        let after_ref = payload
            .get("after_ref")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                AdapterError::Unsupported(
                    "git execute currently only supports payload.after_ref capture".to_string(),
                )
            })?;

        let current_head = git_head(&repo_path)?;
        if current_head != after_ref {
            return Err(AdapterError::Validation(format!(
                "git repo HEAD {} does not match requested after_ref {}",
                current_head, after_ref
            )));
        }

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("after_ref".to_string(), serde_json::json!(after_ref));

        Ok(ExecuteReceipt {
            external_id: Some(current_head.clone()),
            result_digest: Some(format!("git-ref:{}", current_head)),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let repo_path = extract_repo_path_from_contract(contract)?;
        let current_head = git_head(&repo_path)?;
        let expected_ref = expected_verify_ref(contract)?;

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert(
            "current_ref".to_string(),
            serde_json::json!(current_head.clone()),
        );
        metadata.insert(
            "expected_ref".to_string(),
            serde_json::json!(expected_ref.clone()),
        );

        Ok(VerifyReceipt {
            verified: current_head == expected_ref,
            adapter_metadata: metadata,
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        reset_to_before_ref(contract)
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        reset_to_before_ref(contract)
    }
}

fn reset_to_before_ref(contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
    let repo_path = extract_repo_path_from_contract(contract)?;
    let before_ref = before_ref_from_contract(contract)?;
    git_reset_hard(&repo_path, &before_ref)?;

    let mut metadata = JsonMap::new();
    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
    metadata.insert("restored_ref".to_string(), serde_json::json!(before_ref));

    Ok(RecoveryReceipt {
        recovered: true,
        adapter_metadata: metadata,
    })
}

fn expected_verify_ref(contract: &RollbackContract) -> Result<String, AdapterError> {
    after_ref_from_contract(contract).or_else(|_| before_ref_from_contract(contract))
}

fn before_ref_from_contract(contract: &RollbackContract) -> Result<String, AdapterError> {
    match &contract.target {
        RollbackTarget::GitRef {
            before_ref: Some(before_ref),
            ..
        } => Ok(before_ref.clone()),
        _ => contract
            .metadata
            .get("before_ref")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| {
                AdapterError::Validation(
                    "Git rollback contract requires before_ref metadata".to_string(),
                )
            }),
    }
}

fn after_ref_from_contract(contract: &RollbackContract) -> Result<String, AdapterError> {
    match &contract.target {
        RollbackTarget::GitRef {
            after_ref: Some(after_ref),
            ..
        } => Ok(after_ref.clone()),
        _ => contract
            .metadata
            .get("after_ref")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| {
                AdapterError::Validation(
                    "Git rollback contract requires after_ref metadata".to_string(),
                )
            }),
    }
}

fn extract_repo_path_from_request(
    request: &RollbackPrepareRequest,
) -> Result<String, AdapterError> {
    match &request.target {
        RollbackTarget::GitRef { repo_path, .. } => Ok(repo_path.clone()),
        _ => request
            .metadata
            .get("repo_path")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| AdapterError::Validation("Git target requires repo_path".to_string())),
    }
}

fn extract_repo_path_from_contract(contract: &RollbackContract) -> Result<String, AdapterError> {
    match &contract.target {
        RollbackTarget::GitRef { repo_path, .. } => Ok(repo_path.clone()),
        _ => contract
            .metadata
            .get("repo_path")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| {
                AdapterError::Validation("Git contract requires repo_path metadata".to_string())
            }),
    }
}

fn git_head(repo_path: &str) -> Result<String, AdapterError> {
    run_git(repo_path, &["rev-parse", "HEAD"])
}

fn git_reset_hard(repo_path: &str, target_ref: &str) -> Result<(), AdapterError> {
    run_git(repo_path, &["reset", "--hard", target_ref]).map(|_| ())
}

fn run_git(repo_path: &str, args: &[&str]) -> Result<String, AdapterError> {
    if !Path::new(repo_path).exists() {
        return Err(AdapterError::Validation(format!(
            "git repo path does not exist: {}",
            repo_path
        )));
    }

    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|err| AdapterError::Internal(format!("failed to run git: {}", err)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AdapterError::Validation(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn register_git_adapter(registry: &mut AdapterRegistry) {
    registry.register(std::sync::Arc::new(GitRollbackAdapter::new(ADAPTER_KEY)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferrum_proto::{
        ActionType, ExecutionId, IntentId, ProposalId, RollbackClass, RollbackContractId,
        RollbackState,
    };
    use tempfile::TempDir;

    fn init_temp_repo() -> (TempDir, String, String) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_str().unwrap().to_string();

        run_git(&repo_path, &["init"]).unwrap();
        run_git(&repo_path, &["config", "user.name", "Ferrum Test"]).unwrap();
        run_git(&repo_path, &["config", "user.email", "ferrum@example.com"]).unwrap();

        std::fs::write(temp_dir.path().join("README.md"), "hello\n").unwrap();
        run_git(&repo_path, &["add", "README.md"]).unwrap();
        run_git(&repo_path, &["commit", "-m", "initial"]).unwrap();

        let head = git_head(&repo_path).unwrap();
        (temp_dir, repo_path, head)
    }

    fn commit_change(repo_path: &str, name: &str, content: &str) -> String {
        std::fs::write(Path::new(repo_path).join(name), content).unwrap();
        run_git(repo_path, &["add", name]).unwrap();
        run_git(repo_path, &["commit", "-m", "update"]).unwrap();
        git_head(repo_path).unwrap()
    }

    fn make_prepare_request(repo_path: &str) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitCommit,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::GitRef {
                repo_path: repo_path.to_string(),
                before_ref: None,
                after_ref: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        }
    }

    fn make_contract(
        repo_path: &str,
        before_ref: Option<&str>,
        after_ref: Option<&str>,
    ) -> RollbackContract {
        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        if let Some(value) = before_ref {
            metadata.insert("before_ref".to_string(), serde_json::json!(value));
        }
        if let Some(value) = after_ref {
            metadata.insert("after_ref".to_string(), serde_json::json!(value));
        }

        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitCommit,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::GitRef {
                repo_path: repo_path.to_string(),
                before_ref: before_ref.map(str::to_string),
                after_ref: after_ref.map(str::to_string),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata,
        }
    }

    #[tokio::test]
    async fn test_prepare_captures_before_ref() {
        let (_temp_dir, repo_path, head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        let receipt = adapter
            .prepare(&make_prepare_request(&repo_path))
            .await
            .unwrap();

        assert_eq!(
            receipt.adapter_metadata.get("before_ref").unwrap().as_str(),
            Some(head.as_str())
        );
        assert_eq!(
            receipt.adapter_metadata.get("repo_path").unwrap().as_str(),
            Some(repo_path.as_str())
        );
    }

    #[tokio::test]
    async fn test_rollback_restores_head_after_commit_change() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "notes.txt", "changed\n");
        assert_ne!(before_ref, after_ref);

        let contract = make_contract(&repo_path, Some(&before_ref), Some(&after_ref));
        let receipt = adapter.rollback(&contract).await.unwrap();

        assert!(receipt.recovered);
        assert_eq!(git_head(&repo_path).unwrap(), before_ref);
    }

    #[tokio::test]
    async fn test_verify_matches_expected_after_ref() {
        let (_temp_dir, repo_path, _before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "notes.txt", "changed\n");
        let contract = make_contract(&repo_path, None, Some(&after_ref));

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(receipt.verified);
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_ref")
                .unwrap()
                .as_str(),
            Some(after_ref.as_str())
        );
    }

    #[tokio::test]
    async fn test_prepare_rejects_missing_repo_path() {
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let request = make_prepare_request("/definitely/missing/repo");

        let err = adapter.prepare(&request).await.unwrap_err();

        assert!(matches!(err, AdapterError::Validation(_)));
    }

    #[tokio::test]
    async fn test_execute_captures_after_ref_when_head_matches() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "notes.txt", "changed\n");
        let contract = make_contract(&repo_path, Some(&before_ref), None);

        let receipt = adapter
            .execute(&contract, &serde_json::json!({ "after_ref": after_ref }))
            .await
            .unwrap();

        let current_head = git_head(&repo_path).unwrap();
        assert_eq!(receipt.external_id.as_deref(), Some(current_head.as_str()));
        assert_eq!(
            receipt.adapter_metadata.get("after_ref").unwrap().as_str(),
            Some(current_head.as_str())
        );
    }

    #[tokio::test]
    async fn test_execute_rejects_missing_after_ref_payload() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let contract = make_contract(&repo_path, Some(&before_ref), None);

        let err = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap_err();

        assert!(matches!(err, AdapterError::Unsupported(_)));
    }

    #[tokio::test]
    async fn test_execute_rejects_mismatched_after_ref() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let contract = make_contract(&repo_path, Some(&before_ref), None);

        let err = adapter
            .execute(&contract, &serde_json::json!({ "after_ref": "deadbeef" }))
            .await
            .unwrap_err();

        assert!(matches!(err, AdapterError::Validation(_)));
    }

    #[tokio::test]
    async fn test_compensate_aliases_rollback() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "notes.txt", "changed\n");
        assert_ne!(before_ref, after_ref);

        let contract = make_contract(&repo_path, Some(&before_ref), Some(&after_ref));
        let receipt = adapter.compensate(&contract).await.unwrap();

        assert!(receipt.recovered);
        assert_eq!(git_head(&repo_path).unwrap(), before_ref);
    }
}
