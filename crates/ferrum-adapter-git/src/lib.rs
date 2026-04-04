// Test-focused local git adapter primitives for rollback evidence.
// This slice captures refs from a local repository and can reset back to a
// previously recorded ref. Full git mutation execution is intentionally out of
// scope here.

use async_trait::async_trait;
use ferrum_proto::{ActionType, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget};
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

        // For GitBranchCreate action, perform additional validation
        if matches!(request.action_type, ActionType::GitBranchCreate) {
            // Extract new branch name from metadata
            let new_branch = request
                .metadata
                .get("new_branch_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "GitBranchCreate requires new_branch_name in metadata".to_string(),
                    )
                })?;

            // Fail-closed if repo is dirty
            if git_is_repo_dirty(&repo_path)? {
                return Err(AdapterError::Validation(
                    "GitBranchCreate failed: repo has uncommitted changes".to_string(),
                ));
            }

            // Fail-closed if branch already exists
            if git_branch_exists(&repo_path, new_branch)? {
                return Err(AdapterError::Validation(format!(
                    "GitBranchCreate failed: branch '{}' already exists",
                    new_branch
                )));
            }

            // Capture original branch name for rollback
            let original_branch = git_current_branch(&repo_path)?;
            metadata.insert(
                "original_branch".to_string(),
                serde_json::json!(original_branch),
            );
            metadata.insert("new_branch_name".to_string(), serde_json::json!(new_branch));
        }

        // For GitPush action, validate remote and capture pre-push state
        if matches!(request.action_type, ActionType::GitPush) {
            let remote = request
                .metadata
                .get("remote")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation("GitPush requires remote in metadata".to_string())
                })?;

            let refspec = request
                .metadata
                .get("refspec")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation("GitPush requires refspec in metadata".to_string())
                })?;

            // Fail-closed if repo is dirty (uncommitted changes would be pushed)
            if git_is_repo_dirty(&repo_path)? {
                return Err(AdapterError::Validation(
                    "GitPush failed: repo has uncommitted changes".to_string(),
                ));
            }

            // Validate remote exists
            if !git_remote_exists(&repo_path, remote)? {
                return Err(AdapterError::Validation(format!(
                    "GitPush failed: remote '{}' does not exist",
                    remote
                )));
            }

            // Capture current branch for rollback
            let current_branch = git_current_branch(&repo_path)?;
            metadata.insert("remote".to_string(), serde_json::json!(remote));
            metadata.insert("refspec".to_string(), serde_json::json!(refspec));
            metadata.insert(
                "current_branch".to_string(),
                serde_json::json!(current_branch),
            );

            // Capture pre-push remote ref if available (for rollback)
            let pre_push_ref = git_remote_ref(&repo_path, remote, refspec).ok();
            if let Some(ref pre_ref) = pre_push_ref {
                metadata.insert("pre_push_ref".to_string(), serde_json::json!(pre_ref));
            }
        }

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

        // Handle GitBranchCreate action type
        if matches!(contract.action_type, ActionType::GitBranchCreate) {
            let new_branch = contract
                .metadata
                .get("new_branch_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "GitBranchCreate execute requires new_branch_name in contract metadata"
                            .to_string(),
                    )
                })?;

            // Create the new branch
            git_create_branch(&repo_path, new_branch)?;

            // Switch to the new branch
            git_checkout(&repo_path, new_branch)?;

            // Verify we're now on the new branch
            let current_branch = git_current_branch(&repo_path)?;
            if current_branch != new_branch {
                return Err(AdapterError::Internal(format!(
                    "After branch creation and checkout, expected branch '{}' but on branch '{}'",
                    new_branch, current_branch
                )));
            }

            let current_head = git_head(&repo_path)?;

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("new_branch_name".to_string(), serde_json::json!(new_branch));
            metadata.insert("after_ref".to_string(), serde_json::json!(current_head));

            return Ok(ExecuteReceipt {
                external_id: Some(current_branch),
                result_digest: Some(format!("git-branch:{}", new_branch)),
                adapter_metadata: metadata,
            });
        }

        // Handle GitPush action type
        if matches!(contract.action_type, ActionType::GitPush) {
            let remote = contract
                .metadata
                .get("remote")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "GitPush execute requires remote in contract metadata".to_string(),
                    )
                })?;

            let refspec = contract
                .metadata
                .get("refspec")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "GitPush execute requires refspec in contract metadata".to_string(),
                    )
                })?;

            // Perform the push
            git_push(&repo_path, remote, refspec)?;

            // Get the post-push remote ref for verification
            let post_push_ref = git_remote_ref(&repo_path, remote, refspec)?;

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("remote".to_string(), serde_json::json!(remote));
            metadata.insert("refspec".to_string(), serde_json::json!(refspec));
            metadata.insert(
                "post_push_ref".to_string(),
                serde_json::json!(post_push_ref.clone()),
            );

            return Ok(ExecuteReceipt {
                external_id: Some(format!("{}:{}", remote, refspec)),
                result_digest: Some(format!("git-push:{}:{}", remote, post_push_ref)),
                adapter_metadata: metadata,
            });
        }

        // Default: GitCommit behavior - validate after_ref matches HEAD
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
        // Fail-closed: extract_repo_path returns error only on malformed input;
        // missing repo path is handled gracefully below as verified=false.
        let repo_path = match extract_repo_path_from_contract(contract) {
            Ok(path) => path,
            Err(e) => {
                let mut metadata = JsonMap::new();
                metadata.insert("error".to_string(), serde_json::json!(e.to_string()));
                metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: metadata,
                });
            }
        };

        // Fail-closed: if git_head fails (repo missing, permission denied, etc.),
        // return verified=false rather than propagating error.
        let current_head = match git_head(&repo_path) {
            Ok(head) => head,
            Err(e) => {
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("error".to_string(), serde_json::json!(e.to_string()));
                metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: metadata,
                });
            }
        };

        // For GitBranchCreate, verify we're on the correct branch and at expected ref
        if matches!(contract.action_type, ActionType::GitBranchCreate) {
            let new_branch = match contract
                .metadata
                .get("new_branch_name")
                .and_then(|v| v.as_str())
            {
                Some(name) => name,
                None => {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert(
                        "error".to_string(),
                        serde_json::json!(
                            "GitBranchCreate verify requires new_branch_name in contract metadata"
                        ),
                    );
                    metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }
            };

            // Fail-closed: if git_current_branch fails, return verified=false
            let current_branch = match git_current_branch(&repo_path) {
                Ok(branch) => branch,
                Err(e) => {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("error".to_string(), serde_json::json!(e.to_string()));
                    metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }
            };

            let expected_ref = expected_verify_ref(contract)?;

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert(
                "current_branch".to_string(),
                serde_json::json!(current_branch.clone()),
            );
            metadata.insert("expected_branch".to_string(), serde_json::json!(new_branch));
            metadata.insert(
                "current_ref".to_string(),
                serde_json::json!(current_head.clone()),
            );
            metadata.insert(
                "expected_ref".to_string(),
                serde_json::json!(expected_ref.clone()),
            );

            // Verified if we're on the correct branch AND at the expected ref
            let branch_verified = current_branch == new_branch;
            let ref_verified = current_head == expected_ref;

            return Ok(VerifyReceipt {
                verified: branch_verified && ref_verified,
                adapter_metadata: metadata,
            });
        }

        // For GitPush, verify the remote ref matches expected
        if matches!(contract.action_type, ActionType::GitPush) {
            let remote = match contract.metadata.get("remote").and_then(|v| v.as_str()) {
                Some(name) => name,
                None => {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert(
                        "error".to_string(),
                        serde_json::json!("GitPush verify requires remote in contract metadata"),
                    );
                    metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }
            };

            let refspec = match contract.metadata.get("refspec").and_then(|v| v.as_str()) {
                Some(rs) => rs,
                None => {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert(
                        "error".to_string(),
                        serde_json::json!("GitPush verify requires refspec in contract metadata"),
                    );
                    metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }
            };

            // Get the current remote ref
            let remote_ref = match git_remote_ref(&repo_path, remote, refspec) {
                Ok(r) => r,
                Err(e) => {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("error".to_string(), serde_json::json!(e.to_string()));
                    metadata.insert("fail_closed".to_string(), serde_json::json!(true));
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: metadata,
                    });
                }
            };

            // Use after_ref from contract target as expected, or fall back to local HEAD
            let expected_ref = match &contract.target {
                RollbackTarget::GitRef {
                    after_ref: Some(after_ref),
                    ..
                } => after_ref.clone(),
                _ => current_head.clone(),
            };

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("remote".to_string(), serde_json::json!(remote));
            metadata.insert("refspec".to_string(), serde_json::json!(refspec));
            metadata.insert(
                "remote_ref".to_string(),
                serde_json::json!(remote_ref.clone()),
            );
            metadata.insert(
                "expected_ref".to_string(),
                serde_json::json!(expected_ref.clone()),
            );

            return Ok(VerifyReceipt {
                verified: remote_ref.as_str() == expected_ref.as_str(),
                adapter_metadata: metadata,
            });
        }

        // Default: verify against expected ref
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
        // For GitBranchCreate, restore original branch and cleanup
        if matches!(contract.action_type, ActionType::GitBranchCreate) {
            return git_cleanup_branch_create(contract);
        }
        // For GitPush, attempt to restore pre-push remote ref via force-push
        if matches!(contract.action_type, ActionType::GitPush) {
            return git_cleanup_push(contract);
        }
        reset_to_before_ref(contract)
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // For GitBranchCreate, restore original branch and cleanup
        if matches!(contract.action_type, ActionType::GitBranchCreate) {
            return git_cleanup_branch_create(contract);
        }
        // For GitPush, attempt to restore pre-push remote ref via force-push
        if matches!(contract.action_type, ActionType::GitPush) {
            return git_cleanup_push(contract);
        }
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

fn git_current_branch(repo_path: &str) -> Result<String, AdapterError> {
    // Use --quiet to avoid error output when HEAD is detached
    run_git(repo_path, &["branch", "--show-current"])
}

fn git_is_repo_dirty(repo_path: &str) -> Result<bool, AdapterError> {
    // Check for uncommitted changes using git status --porcelain
    let output = run_git(repo_path, &["status", "--porcelain"])?;
    // If output is empty, repo is clean; otherwise it's dirty
    Ok(!output.trim().is_empty())
}

fn git_branch_exists(repo_path: &str, branch_name: &str) -> Result<bool, AdapterError> {
    let output = run_git(repo_path, &["branch", "--list", branch_name])?;
    Ok(!output.trim().is_empty())
}

fn git_create_branch(repo_path: &str, branch_name: &str) -> Result<(), AdapterError> {
    run_git(repo_path, &["branch", branch_name]).map(|_| ())
}

fn git_checkout(repo_path: &str, branch_name: &str) -> Result<(), AdapterError> {
    run_git(repo_path, &["checkout", branch_name]).map(|_| ())
}

fn git_delete_branch(repo_path: &str, branch_name: &str) -> Result<(), AdapterError> {
    // Use -d (delete) which fails if branch is not merged; use -D for force delete
    // We use -D since we created this branch and know it's safe to delete
    run_git(repo_path, &["branch", "-D", branch_name]).map(|_| ())
}

fn git_cleanup_branch_create(contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
    let repo_path = extract_repo_path_from_contract(contract)?;
    let original_branch = contract
        .metadata
        .get("original_branch")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AdapterError::Validation(
                "GitBranchCreate rollback requires original_branch in contract metadata"
                    .to_string(),
            )
        })?;
    let new_branch = contract
        .metadata
        .get("new_branch_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AdapterError::Validation(
                "GitBranchCreate rollback requires new_branch_name in contract metadata"
                    .to_string(),
            )
        })?;

    // Switch back to original branch
    git_checkout(&repo_path, original_branch)?;

    // Delete the created branch (force delete since we created it during execute)
    // Ignore error if branch was already deleted or doesn't exist
    let _ = git_delete_branch(&repo_path, new_branch);

    let mut metadata = JsonMap::new();
    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
    metadata.insert(
        "restored_branch".to_string(),
        serde_json::json!(original_branch),
    );
    metadata.insert("deleted_branch".to_string(), serde_json::json!(new_branch));

    Ok(RecoveryReceipt {
        recovered: true,
        adapter_metadata: metadata,
    })
}

fn git_remote_exists(repo_path: &str, remote: &str) -> Result<bool, AdapterError> {
    let output = run_git(repo_path, &["remote", "show", remote])?;
    Ok(!output.trim().is_empty())
}

fn git_remote_ref(repo_path: &str, remote: &str, refspec: &str) -> Result<String, AdapterError> {
    // Use ls-remote to get the ref hash for the given refspec
    // For branches: refs/heads/<branch>
    // For tags: refs/tags/<tag>
    // For refs like master, main: try to resolve to full ref path
    let ref_to_query = if refspec.contains("refs/") {
        refspec.to_string()
    } else if refspec == "HEAD" {
        format!("refs/remotes/{}/HEAD", remote)
    } else {
        // Try to find the ref - check heads first
        let heads_ref = format!("refs/heads/{}", refspec);
        let result = run_git(repo_path, &["ls-remote", remote, &heads_ref]);
        if let Ok(output) = result {
            if !output.trim().is_empty() {
                // Output format: "<hash> <ref>"
                if let Some(hash) = output.split_whitespace().next() {
                    return Ok(hash.to_string());
                }
            }
        }
        // Try tags
        let tags_ref = format!("refs/tags/{}", refspec);
        let result = run_git(repo_path, &["ls-remote", remote, &tags_ref]);
        if let Ok(output) = result {
            if !output.trim().is_empty() {
                if let Some(hash) = output.split_whitespace().next() {
                    return Ok(hash.to_string());
                }
            }
        }
        // Fall back to direct refspec
        refspec.to_string()
    };

    // Query the remote for this ref
    let output = run_git(repo_path, &["ls-remote", remote, &ref_to_query])?;

    // Parse the output: "<hash> <ref>"
    output
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AdapterError::Validation(format!(
                "git remote ref not found for {} on {}",
                refspec, remote
            ))
        })
}

fn git_push(repo_path: &str, remote: &str, refspec: &str) -> Result<(), AdapterError> {
    run_git(repo_path, &["push", remote, refspec]).map(|_| ())
}

fn git_push_force(repo_path: &str, remote: &str, refspec: &str) -> Result<(), AdapterError> {
    run_git(repo_path, &["push", "--force", remote, refspec]).map(|_| ())
}

fn git_cleanup_push(contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
    let repo_path = extract_repo_path_from_contract(contract)?;
    let remote = contract
        .metadata
        .get("remote")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AdapterError::Validation(
                "GitPush rollback requires remote in contract metadata".to_string(),
            )
        })?;
    let refspec = contract
        .metadata
        .get("refspec")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AdapterError::Validation(
                "GitPush rollback requires refspec in contract metadata".to_string(),
            )
        })?;

    // If we captured pre_push_ref, attempt force-push to restore it
    let pre_push_ref = contract
        .metadata
        .get("pre_push_ref")
        .and_then(|v| v.as_str());

    let mut metadata = JsonMap::new();
    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
    metadata.insert("remote".to_string(), serde_json::json!(remote));
    metadata.insert("refspec".to_string(), serde_json::json!(refspec));

    if let Some(pre_ref) = pre_push_ref {
        // Force-push the pre-push ref back to restore remote state
        git_push_force(&repo_path, remote, &format!("{}:{}", pre_ref, refspec))?;
        metadata.insert(
            "compensated_with".to_string(),
            serde_json::json!("force-push pre_push_ref"),
        );
        metadata.insert("pre_push_ref".to_string(), serde_json::json!(pre_ref));
    } else {
        metadata.insert(
            "compensated_with".to_string(),
            serde_json::json!("no-op (no pre_push_ref captured)"),
        );
    }

    Ok(RecoveryReceipt {
        recovered: true,
        adapter_metadata: metadata,
    })
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

    // ============ GitBranchCreate Tests ============

    fn make_branch_create_prepare_request(
        repo_path: &str,
        new_branch_name: &str,
    ) -> RollbackPrepareRequest {
        let mut metadata = JsonMap::new();
        metadata.insert(
            "new_branch_name".to_string(),
            serde_json::json!(new_branch_name),
        );

        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitBranchCreate,
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
            metadata,
        }
    }

    fn make_branch_create_contract(
        repo_path: &str,
        before_ref: &str,
        new_branch_name: &str,
        original_branch: &str,
    ) -> RollbackContract {
        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("before_ref".to_string(), serde_json::json!(before_ref));
        metadata.insert(
            "new_branch_name".to_string(),
            serde_json::json!(new_branch_name),
        );
        metadata.insert(
            "original_branch".to_string(),
            serde_json::json!(original_branch),
        );

        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitBranchCreate,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: ADAPTER_KEY.to_string(),
            target: RollbackTarget::GitRef {
                repo_path: repo_path.to_string(),
                before_ref: Some(before_ref.to_string()),
                after_ref: None,
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
    async fn test_branch_create_prepare_captures_original_branch() {
        let (_temp_dir, repo_path, _head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        let receipt = adapter
            .prepare(&make_branch_create_prepare_request(
                &repo_path,
                "feature/test",
            ))
            .await
            .unwrap();

        assert!(receipt.accepted);
        // Verify original_branch is captured (actual name may be "main" or "master" depending on git version)
        let original_branch = receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(original_branch == "main" || original_branch == "master");
        assert_eq!(
            receipt
                .adapter_metadata
                .get("new_branch_name")
                .unwrap()
                .as_str(),
            Some("feature/test")
        );
    }

    #[tokio::test]
    async fn test_branch_create_prepare_rejects_dirty_repo() {
        let (_temp_dir, repo_path, _head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Make repo dirty by creating a file
        std::fs::write(Path::new(&repo_path).join("uncommitted.txt"), "dirty\n").unwrap();

        let err = adapter
            .prepare(&make_branch_create_prepare_request(
                &repo_path,
                "feature/test",
            ))
            .await
            .unwrap_err();

        assert!(
            matches!(err, AdapterError::Validation(ref msg) if msg.contains("uncommitted changes"))
        );
    }

    #[tokio::test]
    async fn test_branch_create_prepare_rejects_existing_branch() {
        let (_temp_dir, repo_path, _head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Create the branch first
        run_git(&repo_path, &["branch", "feature/existing"]).unwrap();

        let err = adapter
            .prepare(&make_branch_create_prepare_request(
                &repo_path,
                "feature/existing",
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, AdapterError::Validation(ref msg) if msg.contains("already exists")));
    }

    #[tokio::test]
    async fn test_branch_create_execute_creates_and_switches_branch() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let new_branch = "feature/test";

        // First prepare
        let prep_receipt = adapter
            .prepare(&make_branch_create_prepare_request(&repo_path, new_branch))
            .await
            .unwrap();

        // Get original branch from prepare receipt
        let original_branch = prep_receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let contract =
            make_branch_create_contract(&repo_path, &before_ref, new_branch, &original_branch);

        // Execute
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(exec_receipt.external_id.as_deref(), Some(new_branch));
        assert_eq!(
            exec_receipt
                .adapter_metadata
                .get("new_branch_name")
                .unwrap()
                .as_str(),
            Some(new_branch)
        );

        // Verify we're on the new branch
        let current_branch = git_current_branch(&repo_path).unwrap();
        assert_eq!(current_branch, new_branch);

        // Verify HEAD hasn't changed (branch points to same commit)
        assert_eq!(git_head(&repo_path).unwrap(), before_ref);
    }

    #[tokio::test]
    async fn test_branch_create_verify_checks_branch_and_ref() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let new_branch = "feature/test";

        // Prepare and execute
        let prep_receipt = adapter
            .prepare(&make_branch_create_prepare_request(&repo_path, new_branch))
            .await
            .unwrap();

        let original_branch = prep_receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let contract =
            make_branch_create_contract(&repo_path, &before_ref, new_branch, &original_branch);

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify should succeed
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
        assert_eq!(
            verify_receipt
                .adapter_metadata
                .get("current_branch")
                .unwrap()
                .as_str(),
            Some(new_branch)
        );
    }

    #[tokio::test]
    async fn test_branch_create_rollback_restores_original_and_cleans_up() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let new_branch = "feature/rollback-test";

        // Prepare
        let prep_receipt = adapter
            .prepare(&make_branch_create_prepare_request(&repo_path, new_branch))
            .await
            .unwrap();

        let original_branch = prep_receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let contract =
            make_branch_create_contract(&repo_path, &before_ref, new_branch, &original_branch);

        // Execute
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Verify we're on the new branch
        assert_eq!(git_current_branch(&repo_path).unwrap(), new_branch);

        // Rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify we're back on original branch
        let current_branch = git_current_branch(&repo_path).unwrap();
        assert_eq!(current_branch, original_branch);

        // Verify the created branch was deleted
        assert!(!git_branch_exists(&repo_path, new_branch).unwrap());

        // Verify HEAD is restored to before_ref
        assert_eq!(git_head(&repo_path).unwrap(), before_ref);
    }

    #[tokio::test]
    async fn test_branch_create_compensate_same_as_rollback() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let new_branch = "feature/compensate-test";

        // Prepare
        let prep_receipt = adapter
            .prepare(&make_branch_create_prepare_request(&repo_path, new_branch))
            .await
            .unwrap();

        let original_branch = prep_receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let contract =
            make_branch_create_contract(&repo_path, &before_ref, new_branch, &original_branch);

        // Execute
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // Compensate
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);

        // Verify we're back on original branch and branch was cleaned up
        assert_eq!(git_current_branch(&repo_path).unwrap(), original_branch);
        assert!(!git_branch_exists(&repo_path, new_branch).unwrap());
    }

    #[tokio::test]
    async fn test_branch_create_happy_path_full_flow() {
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let new_branch = "feature/full-flow-test";

        // Step 1: Prepare
        let prep_receipt = adapter
            .prepare(&make_branch_create_prepare_request(&repo_path, new_branch))
            .await
            .unwrap();
        assert!(prep_receipt.accepted);

        let original_branch = prep_receipt
            .adapter_metadata
            .get("original_branch")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Step 2: Execute
        let contract =
            make_branch_create_contract(&repo_path, &before_ref, new_branch, &original_branch);

        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(exec_receipt.external_id.as_deref(), Some(new_branch));

        // Step 3: Verify
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);

        // Step 4: Rollback (simulating failure/recovery)
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Final state verification
        assert_eq!(git_current_branch(&repo_path).unwrap(), original_branch);
        assert!(!git_branch_exists(&repo_path, new_branch).unwrap());
        assert_eq!(git_head(&repo_path).unwrap(), before_ref);
    }

    // ============ Fail-Closed Verify + Noop Edge Case Tests ============

    #[tokio::test]
    async fn test_verify_repo_path_missing_is_verified_false_not_error() {
        // Fail-closed: when repo path is missing, verify should return verified=false, not error.
        // This ensures commit is rejected rather than ambiguous when verification fails.
        let (_temp_dir, repo_path, _head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let contract = make_contract(&repo_path, None, Some("abc123"));

        // Drop the temp directory to make repo_path invalid
        drop(_temp_dir);

        // Verify should return verified=false (fail-closed), NOT an error
        let receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            !receipt.verified,
            "verify should return false when repo is inaccessible"
        );
    }

    #[tokio::test]
    async fn test_verify_already_at_expected_ref_is_verified_true() {
        // Noop edge case: verify when already at expected ref should succeed.
        let (_temp_dir, repo_path, head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Contract expects current HEAD as after_ref
        let contract = make_contract(&repo_path, None, Some(&head));

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(
            receipt.verified,
            "verify should succeed when already at expected ref"
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("current_ref")
                .unwrap()
                .as_str(),
            Some(head.as_str())
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_ref")
                .unwrap()
                .as_str(),
            Some(head.as_str())
        );
    }

    #[tokio::test]
    async fn test_verify_ref_mismatch_is_verified_false() {
        // When current ref doesn't match expected, verify should return false.
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "newfile.txt", "content\n");

        // Contract expects before_ref but repo is at after_ref
        let contract = make_contract(&repo_path, None, Some(&before_ref));

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(
            !receipt.verified,
            "verify should return false when ref mismatch"
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("current_ref")
                .unwrap()
                .as_str(),
            Some(after_ref.as_str())
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_ref")
                .unwrap()
                .as_str(),
            Some(before_ref.as_str())
        );
    }

    #[tokio::test]
    async fn test_verify_missing_both_refs_falls_back_to_before_ref() {
        // When after_ref is missing from contract, verify should fall back to before_ref.
        let (_temp_dir, repo_path, head) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Contract with only before_ref (no after_ref)
        let contract = make_contract(&repo_path, Some(&head), None);

        let receipt = adapter.verify(&contract).await.unwrap();

        // HEAD matches before_ref, so verified=true
        assert!(
            receipt.verified,
            "verify should succeed when HEAD matches fallback before_ref"
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_ref")
                .unwrap()
                .as_str(),
            Some(head.as_str())
        );
    }

    #[tokio::test]
    async fn test_verify_missing_both_refs_and_head_changed_is_verified_false() {
        // When after_ref is missing and HEAD has changed from before_ref, verify should fail.
        let (_temp_dir, repo_path, before_ref) = init_temp_repo();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);
        let after_ref = commit_change(&repo_path, "change.txt", "modification\n");

        // Contract with only before_ref (no after_ref)
        let contract = make_contract(&repo_path, Some(&before_ref), None);

        let receipt = adapter.verify(&contract).await.unwrap();

        // HEAD is at after_ref which differs from before_ref, so verified=false
        assert!(
            !receipt.verified,
            "verify should fail when HEAD differs from before_ref fallback"
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("current_ref")
                .unwrap()
                .as_str(),
            Some(after_ref.as_str())
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_ref")
                .unwrap()
                .as_str(),
            Some(before_ref.as_str())
        );
    }

    // ============ GitPush Tests ============

    /// Sets up a local repo with a bare repo as remote
    fn init_repo_with_remote() -> (TempDir, String, String, TempDir, String) {
        // Create the main repo
        let main_temp = TempDir::new().unwrap();
        let main_path = main_temp.path().to_str().unwrap().to_string();
        run_git(&main_path, &["init"]).unwrap();
        run_git(&main_path, &["config", "user.name", "Ferrum Test"]).unwrap();
        run_git(&main_path, &["config", "user.email", "ferrum@example.com"]).unwrap();

        std::fs::write(main_temp.path().join("README.md"), "hello\n").unwrap();
        run_git(&main_path, &["add", "README.md"]).unwrap();
        run_git(&main_path, &["commit", "-m", "initial"]).unwrap();
        let main_head = git_head(&main_path).unwrap();

        // Create a bare repo to act as remote
        let remote_temp = TempDir::new().unwrap();
        let remote_path = remote_temp.path().to_str().unwrap().to_string();
        run_git(&remote_path, &["init", "--bare"]).unwrap();

        // Add the bare repo as remote
        run_git(&main_path, &["remote", "add", "origin", &remote_path]).unwrap();

        (main_temp, main_path, main_head, remote_temp, remote_path)
    }

    fn make_push_prepare_request(
        repo_path: &str,
        remote: &str,
        refspec: &str,
    ) -> RollbackPrepareRequest {
        let mut metadata = JsonMap::new();
        metadata.insert("remote".to_string(), serde_json::json!(remote));
        metadata.insert("refspec".to_string(), serde_json::json!(refspec));

        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitPush,
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
            metadata,
        }
    }

    fn make_push_contract(
        repo_path: &str,
        remote: &str,
        refspec: &str,
        pre_push_ref: Option<&str>,
        before_ref: Option<&str>,
        after_ref: Option<&str>,
    ) -> RollbackContract {
        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("remote".to_string(), serde_json::json!(remote));
        metadata.insert("refspec".to_string(), serde_json::json!(refspec));
        if let Some(pre_ref) = pre_push_ref {
            metadata.insert("pre_push_ref".to_string(), serde_json::json!(pre_ref));
        }
        if let Some(before) = before_ref {
            metadata.insert("before_ref".to_string(), serde_json::json!(before));
        }

        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::GitPush,
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
    async fn test_gitpush_prepare_captures_pre_push_state() {
        let (_main_temp, main_path, _main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        let receipt = adapter
            .prepare(&make_push_prepare_request(&main_path, "origin", "master"))
            .await
            .unwrap();

        assert!(receipt.accepted);
        assert_eq!(
            receipt.adapter_metadata.get("remote").unwrap().as_str(),
            Some("origin")
        );
        assert_eq!(
            receipt.adapter_metadata.get("refspec").unwrap().as_str(),
            Some("master")
        );
        // pre_push_ref should be empty since nothing exists on remote yet
        let pre_push_ref = receipt.adapter_metadata.get("pre_push_ref");
        assert!(
            pre_push_ref.is_none() || pre_push_ref.unwrap().as_str() == Some(""),
            "pre_push_ref should be empty for initial push"
        );
    }

    #[tokio::test]
    async fn test_gitpush_prepare_rejects_dirty_repo() {
        let (_main_temp, main_path, _main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Make repo dirty
        std::fs::write(Path::new(&main_path).join("uncommitted.txt"), "dirty\n").unwrap();

        let err = adapter
            .prepare(&make_push_prepare_request(&main_path, "origin", "master"))
            .await
            .unwrap_err();

        assert!(
            matches!(err, AdapterError::Validation(ref msg) if msg.contains("uncommitted changes")),
            "Expected validation error for dirty repo, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_gitpush_prepare_rejects_missing_remote() {
        let (_main_temp, main_path, _main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        let err = adapter
            .prepare(&make_push_prepare_request(
                &main_path,
                "nonexistent",
                "master",
            ))
            .await
            .unwrap_err();

        assert!(
            matches!(err, AdapterError::Validation(ref msg) if msg.contains("does not exist") || msg.contains("does not appear")),
            "Expected validation error for missing remote, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_gitpush_execute_performs_push() {
        let (_main_temp, main_path, main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // First prepare
        let prep_receipt = adapter
            .prepare(&make_push_prepare_request(&main_path, "origin", "master"))
            .await
            .unwrap();

        let pre_push_ref = prep_receipt
            .adapter_metadata
            .get("pre_push_ref")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let contract = make_push_contract(
            &main_path,
            "origin",
            "master",
            pre_push_ref.as_deref(),
            Some(&main_head),
            None,
        );

        // Execute
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(exec_receipt.external_id.as_deref(), Some("origin:master"));
        assert!(
            exec_receipt
                .result_digest
                .as_ref()
                .unwrap()
                .starts_with("git-push:origin:"),
        );

        // Verify the push actually happened by checking remote has the commit
        let remote_ref = git_remote_ref(&main_path, "origin", "master").unwrap();
        assert_eq!(remote_ref, main_head);
    }

    #[tokio::test]
    async fn test_gitpush_verify_confirms_push() {
        let (_main_temp, main_path, main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // First push to set up remote state
        run_git(&main_path, &["push", "origin", "master"]).unwrap();

        // Prepare contract
        let pre_push_ref = git_remote_ref(&main_path, "origin", "master").ok();
        let contract = make_push_contract(
            &main_path,
            "origin",
            "master",
            pre_push_ref.as_deref(),
            Some(&main_head),
            None,
        );

        // Verify should succeed because remote matches local HEAD
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            verify_receipt.verified,
            "verify should confirm remote ref matches expected: {:?}",
            verify_receipt.adapter_metadata
        );
    }

    #[tokio::test]
    async fn test_gitpush_rollback_force_pushes_pre_ref() {
        let (_main_temp, main_path, main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // First push initial state
        run_git(&main_path, &["push", "origin", "master"]).unwrap();
        let initial_remote_ref = git_remote_ref(&main_path, "origin", "master").unwrap();

        // Make a new commit
        commit_change(&main_path, "newfile.txt", "content\n");
        let _new_head = git_head(&main_path).unwrap();

        // Push the new commit
        run_git(&main_path, &["push", "origin", "master"]).unwrap();

        // Now create contract with pre_push_ref pointing to initial state
        let contract = make_push_contract(
            &main_path,
            "origin",
            "master",
            Some(&initial_remote_ref),
            Some(&main_head),
            None,
        );

        // Rollback should force-push the pre_push_ref
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // Verify remote is back to initial state
        let remote_ref = git_remote_ref(&main_path, "origin", "master").unwrap();
        assert_eq!(
            remote_ref, initial_remote_ref,
            "remote should be force-pushed back to pre_push_ref"
        );
    }

    #[tokio::test]
    async fn test_gitpush_happy_path_full_flow() {
        let (_main_temp, main_path, main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Step 1: Prepare - captures pre-push state
        let prep_receipt = adapter
            .prepare(&make_push_prepare_request(&main_path, "origin", "master"))
            .await
            .unwrap();
        assert!(prep_receipt.accepted);

        let pre_push_ref = prep_receipt
            .adapter_metadata
            .get("pre_push_ref")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Step 2: Execute - push to remote
        let contract = make_push_contract(
            &main_path,
            "origin",
            "master",
            pre_push_ref.as_deref(),
            Some(&main_head),
            None,
        );

        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(exec_receipt.external_id.as_deref(), Some("origin:master"));

        // Step 3: Verify - confirm push succeeded
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            verify_receipt.verified,
            "verify should succeed after push: {:?}",
            verify_receipt.adapter_metadata
        );

        // Step 4: Rollback - attempt compensation
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // For initial push (no pre_push_ref), rollback is a no-op
        let compensated_with = rollback_receipt
            .adapter_metadata
            .get("compensated_with")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(
            compensated_with.contains("no-op"),
            "initial push rollback should be no-op, got: {}",
            compensated_with
        );
    }

    #[tokio::test]
    async fn test_gitpush_verify_fails_when_remote_differs() {
        let (_main_temp, main_path, _main_head, _remote_temp, _remote_path) =
            init_repo_with_remote();
        let adapter = GitRollbackAdapter::new(ADAPTER_KEY);

        // Push initial state
        run_git(&main_path, &["push", "origin", "master"]).unwrap();
        let initial_ref = git_remote_ref(&main_path, "origin", "master").unwrap();

        // Make and push a different commit
        commit_change(&main_path, "change.txt", "different\n");
        run_git(&main_path, &["push", "origin", "master"]).unwrap();

        // Create contract expecting the initial ref (after_ref = initial_ref means we expect remote to be at initial_ref)
        let contract = make_push_contract(
            &main_path,
            "origin",
            "master",
            Some(&initial_ref),
            None,
            Some(&initial_ref),
        );

        // Verify should fail because remote is ahead
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(
            !verify_receipt.verified,
            "verify should fail when remote differs from expected: {:?}",
            verify_receipt.adapter_metadata
        );
    }
}
