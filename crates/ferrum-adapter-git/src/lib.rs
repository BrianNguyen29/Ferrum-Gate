use async_trait::async_trait;
use ferrum_proto::{ActionType, JsonMap, RollbackContract, RollbackPrepareRequest, RollbackTarget};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

pub const ADAPTER_KEY: &str = "git";

// Plannable adapter for Git operations
pub mod planner;
pub use planner::PlannableGitAdapter;

#[derive(Debug, Error)]
pub enum GitAdapterError {
    #[error("unsupported action: {0}")]
    Unsupported(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("git operation failed: {0}")]
    GitError(String),
}

impl From<GitAdapterError> for AdapterError {
    fn from(e: GitAdapterError) -> Self {
        match e {
            GitAdapterError::Unsupported(s) => AdapterError::Unsupported(s),
            GitAdapterError::Validation(s) => AdapterError::Validation(s),
            GitAdapterError::Internal(s) => AdapterError::Internal(s),
            GitAdapterError::GitError(s) => AdapterError::Internal(s),
        }
    }
}

/// GitRollbackAdapter provides local git repository ref capture and reset primitives.
///
/// Supported operations:
/// - `prepare`: captures current HEAD SHA as `before_ref` in adapter metadata
/// - `rollback`: resets repository hard to `before_ref`
/// - `verify`: checks whether current HEAD matches expected ref
/// - `compensate`: alias for rollback in this slice
/// - `execute`: captures `after_ref` when repo state has changed, returns error for
///   unsupported payloads
pub struct GitRollbackAdapter {
    allowed_repo_roots: Vec<PathBuf>,
}

impl GitRollbackAdapter {
    pub fn new(allowed_repo_roots: Vec<PathBuf>) -> Result<Self, AdapterError> {
        let allowed_repo_roots = Self::canonicalize_repo_roots(allowed_repo_roots)?;
        Ok(Self { allowed_repo_roots })
    }

    #[cfg(any(test, feature = "unsafe-unbounded-adapters"))]
    pub fn new_unbounded() -> Self {
        Self {
            allowed_repo_roots: Vec::new(),
        }
    }

    fn canonicalize_repo_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>, AdapterError> {
        if roots.is_empty() {
            return Err(AdapterError::Validation(
                "git adapter requires at least one allowed repository root".to_string(),
            ));
        }
        let mut canonical = Vec::with_capacity(roots.len());
        for root in roots {
            if !root.is_absolute() {
                return Err(AdapterError::Validation(format!(
                    "git repository root must be absolute: {}",
                    root.display()
                )));
            }
            let resolved = std::fs::canonicalize(&root).map_err(|e| {
                AdapterError::Validation(format!(
                    "failed to canonicalize git repository root {}: {}",
                    root.display(),
                    e
                ))
            })?;
            if !resolved.is_dir() {
                return Err(AdapterError::Validation(format!(
                    "git repository root is not a directory: {}",
                    resolved.display()
                )));
            }
            canonical.push(resolved);
        }
        Ok(canonical)
    }

    fn canonical_repo_path_allowed(&self, repo_path: &str) -> Result<PathBuf, AdapterError> {
        let resolved = std::fs::canonicalize(repo_path).map_err(|e| {
            AdapterError::Validation(format!(
                "failed to canonicalize git repository path {}: {}",
                repo_path, e
            ))
        })?;
        if self.allowed_repo_roots.is_empty()
            || self
                .allowed_repo_roots
                .iter()
                .any(|root| resolved.starts_with(root))
        {
            return Ok(resolved);
        }
        Err(AdapterError::Validation(format!(
            "git repository path {} is outside configured repository roots",
            resolved.display()
        )))
    }

    fn canonical_repo_path_string(&self, repo_path: &str) -> Result<String, AdapterError> {
        let resolved = self.canonical_repo_path_allowed(repo_path)?;
        resolved
            .to_str()
            .map(|path| path.to_string())
            .ok_or_else(|| {
                AdapterError::Validation("git repository path is not valid UTF-8".into())
            })
    }

    /// Run a git command and return stdout on success.
    fn git_command(repo_path: &str, args: &[&str]) -> Result<String, GitAdapterError> {
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(args)
            .output()
            .map_err(|e| GitAdapterError::GitError(format!("failed to spawn git: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitAdapterError::GitError(format!(
                "git {} failed: {stderr}",
                args.join(" ")
            )))
        }
    }

    /// Run a git command with environment passthrough for authenticated operations.
    ///
    /// This is the git-native credential delegation approach: instead of storing secrets,
    /// we delegate to the system's existing credential helpers, SSH agent, or git config.
    ///
    /// Environment variables set:
    /// - `GIT_TERMINAL_PROMPT=0`: Disables interactive terminal prompt (fail-fast).
    ///   If git needs credentials and none are available via helper/agent, it fails
    ///   immediately rather than hanging on a prompt.
    /// - `SSH_AUTH_SOCK` passthrough: Forward SSH agent socket so SSH key-based
    ///   authentication works with agents like ssh-agent, gpg-agent, or keychain.
    ///
    /// The credential-helper name may be stored in per-remote config (not secrets);
    /// git resolves this to actual credentials at runtime via the helper.
    ///
    /// Returns stdout on success.
    fn git_command_with_env(
        repo_path: &str,
        args: &[&str],
        credential_helper: Option<&str>,
    ) -> Result<String, GitAdapterError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(repo_path).args(args);

        // GIT_TERMINAL_PROMPT=0: fail-fast instead of interactive prompt
        // This ensures authenticated git operations fail immediately if credentials
        // are missing, rather than hanging waiting for user input.
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        // Pass through SSH_AUTH_SOCK for SSH agent-based authentication.
        // This allows SSH keys managed by ssh-agent, gpg-agent, keychain, etc.
        // to be used for git operations over SSH without storing private keys.
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", sock);
        }

        // Set per-remote credential helper if provided (name only, not secrets).
        // Git uses this helper from ~/.gitconfig or repo .git/config to obtain
        // credentials at runtime. The helper itself (e.g., "osxkeychain", "store")
        // is looked up by git; we only store the helper name reference.
        if let Some(helper) = credential_helper {
            cmd.env("GIT_CREDENTIAL_HELPER", helper);
        }

        let output = cmd
            .output()
            .map_err(|e| GitAdapterError::GitError(format!("failed to spawn git: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitAdapterError::GitError(format!(
                "git {} failed: {stderr}",
                args.join(" ")
            )))
        }
    }

    /// Get current HEAD SHA for a repository.
    fn get_head_sha(repo_path: &str) -> Result<String, GitAdapterError> {
        Self::git_command(repo_path, &["rev-parse", "HEAD"])
    }

    /// Get current branch name for a repository (returns None if detached HEAD).
    fn get_current_branch(repo_path: &str) -> Result<Option<String>, GitAdapterError> {
        let output = Self::git_command(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        if output == "HEAD" {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }

    /// Check if the repository is in detached HEAD state.
    /// Returns true if HEAD is detached (not pointing to a branch), false otherwise.
    fn is_detached_head(repo_path: &str) -> Result<bool, GitAdapterError> {
        Ok(Self::get_current_branch(repo_path)?.is_none())
    }

    /// Check if a branch exists in the local repository.
    fn branch_exists(repo_path: &str, branch_name: &str) -> Result<bool, GitAdapterError> {
        let output = Self::git_command(repo_path, &["branch", "--list", branch_name])?;
        Ok(!output.trim().is_empty())
    }

    /// Check if a tag exists in the local repository.
    fn tag_exists(repo_path: &str, tag_name: &str) -> Result<bool, GitAdapterError> {
        let output = Self::git_command(repo_path, &["tag", "--list", tag_name])?;
        Ok(!output.trim().is_empty())
    }

    /// Validate a tag name using git-native rules via `git check-ref-format`.
    ///
    /// This enforces git's tag naming restrictions during prepare (fail-closed)
    /// so that execute never reaches git CLI failure due to invalid tag names.
    ///
    /// Returns `Ok(())` for valid tag names, `Err` with descriptive message for invalid.
    fn validate_tag_name(tag_name: &str) -> Result<(), GitAdapterError> {
        // Use check-ref-format to validate tag name format
        // Tags are refs under refs/tags/, so we validate "refs/tags/<tag_name>"
        let output = Command::new("git")
            .args([
                "check-ref-format",
                "--normalize",
                &format!("refs/tags/{}", tag_name),
            ])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                // Also ensure the normalized tag matches what we passed (no weird escapes)
                let normalized = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if normalized == format!("refs/tags/{}", tag_name)
                    || normalized
                        == format!("refs/tags/{}", tag_name).trim_start_matches("refs/tags/")
                {
                    Ok(())
                } else {
                    Err(GitAdapterError::Validation(format!(
                        "invalid tag name '{}': git normalized to '{}' which doesn't match",
                        tag_name, normalized
                    )))
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(GitAdapterError::Validation(format!(
                    "invalid tag name '{}': {}",
                    tag_name,
                    if stderr.is_empty() {
                        "git rejected tag name format"
                    } else {
                        &stderr
                    }
                )))
            }
            Err(e) => Err(GitAdapterError::GitError(format!(
                "failed to validate tag name '{}': {}",
                tag_name, e
            ))),
        }
    }

    /// Resolve a git ref to a commit SHA.
    /// Uses `^{commit}` to strip annotated tag objects and ensure the result
    /// is always a commit SHA, which is required for branch creation and verification.
    fn resolve_ref_to_commit_sha(
        repo_path: &str,
        ref_name: &str,
    ) -> Result<String, GitAdapterError> {
        Self::git_command(
            repo_path,
            &["rev-parse", &format!("{}^{{commit}}", ref_name)],
        )
    }

    /// Validate a branch name using git-native rules via `git check-ref-format --branch`.
    fn validate_branch_name(branch_name: &str) -> Result<(), GitAdapterError> {
        let output = Command::new("git")
            .args(["check-ref-format", "--branch", branch_name])
            .output();

        match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(GitAdapterError::Validation(format!(
                    "invalid branch name '{}': {}",
                    branch_name,
                    if stderr.is_empty() {
                        "git rejected branch name format"
                    } else {
                        &stderr
                    }
                )))
            }
            Err(e) => Err(GitAdapterError::GitError(format!(
                "failed to validate branch name '{}': {}",
                branch_name, e
            ))),
        }
    }

    /// Check if the worktree has uncommitted changes.
    /// Returns true if there are uncommitted changes (dirty), false if clean.
    fn is_worktree_dirty(repo_path: &str) -> Result<bool, GitAdapterError> {
        // --porcelain gives a clean, machine-readable output
        // Empty output means clean worktree; non-empty means dirty
        let output = Self::git_command(repo_path, &["status", "--porcelain"])?;
        Ok(!output.trim().is_empty())
    }

    /// Check if a branch has diverged from HEAD (contains commits not in HEAD).
    /// Returns true if the branch tip is not a descendant of HEAD, meaning it has
    /// commits that would be lost by a simple merge and would require force-delete.
    /// This is used to enforce the safe-deletion policy: fail-closed instead of
    /// falling back to `git branch -D` when safe delete would reject deletion.
    #[allow(dead_code)]
    fn is_branch_diverged_from_head(
        repo_path: &str,
        branch_name: &str,
    ) -> Result<bool, GitAdapterError> {
        // Get HEAD commit SHA
        let head_sha = Self::get_head_sha(repo_path)?;

        // Get the merge-base between HEAD and the branch
        // If the branch is just a copy of HEAD or behind HEAD, merge-base == HEAD
        // If the branch has diverged, merge-base will be an ancestor of HEAD
        let merge_base_output = Self::git_command(repo_path, &["merge-base", "HEAD", branch_name])?;

        // If merge-base != HEAD, the branch has diverged (contains commits not in HEAD)
        Ok(merge_base_output.trim() != head_sha)
    }

    /// Check if a branch can be safely deleted with `git branch -d`.
    ///
    /// Safe deletion means the branch is either:
    /// - Fully merged to HEAD (or another specified ref), or
    /// - Already fully merged to its upstream branch
    ///
    /// Returns `Ok(true)` if safe to delete with -d, `Ok(false)` if -d would reject.
    /// Returns `Err` on git errors (treats errors as unsafe).
    fn can_branch_be_safe_deleted(
        repo_path: &str,
        branch_name: &str,
    ) -> Result<bool, GitAdapterError> {
        // Check if branch_sha is ahead of head_sha (diverged or ahead)
        // git rev-list --count HEAD..branch_name returns number of commits from HEAD to branch
        // If ahead == 0, the branch is at or behind HEAD (merged, no divergent commits)
        // If ahead > 0, the branch has commits not in HEAD (diverged), safe delete would reject
        let ahead_count = Self::git_command(
            repo_path,
            &["rev-list", "--count", &format!("HEAD..{}", branch_name)],
        )?;
        let ahead: usize = ahead_count.trim().parse().unwrap_or(1);

        // If branch is ahead (or diverged), safe delete would reject
        Ok(ahead == 0)
    }

    /// Get the URL for a remote.
    #[allow(dead_code)]
    fn get_remote_url(repo_path: &str, remote_name: &str) -> Result<String, GitAdapterError> {
        Self::git_command(repo_path, &["remote", "get-url", remote_name])
    }

    /// Get the configured credential helper name for a remote (helper name only, not secrets).
    ///
    /// Returns `Ok(None)` if no helper is configured for the given remote.
    /// The helper name (e.g., "osxkeychain", "store", "wincred") is looked up by git
    /// at runtime from ~/.gitconfig or repo .git/config. We store only the name,
    /// never the actual credentials.
    fn get_remote_credential_helper(
        repo_path: &str,
        remote_name: &str,
    ) -> Result<Option<String>, GitAdapterError> {
        // Try to get credential.helper config for the specific remote.
        // Note: git config returns error exit code when the key is not set,
        // so we handle that gracefully by treating errors as "not configured".
        let remote_helper = Self::git_command(
            repo_path,
            &["config", &format!("credential.{}.helper", remote_name)],
        );

        match remote_helper {
            Ok(helper) if !helper.trim().is_empty() => Ok(Some(helper.trim().to_string())),
            _ => {
                // Fall back to checking for a default/global credential helper.
                // Try the unprefixed credential.helper first (covers remote-specific URLs
                // and general defaults), then the explicit --global one.
                // git returns error when config is not set, so we handle that as "not configured".
                let default_helper = Self::git_command(repo_path, &["config", "credential.helper"]);
                if let Ok(helper) = default_helper {
                    if !helper.trim().is_empty() {
                        return Ok(Some(helper.trim().to_string()));
                    }
                }

                let global_output =
                    Self::git_command(repo_path, &["config", "--global", "credential.helper"]);
                match global_output {
                    Ok(helper) if !helper.trim().is_empty() => Ok(Some(helper.trim().to_string())),
                    _ => Ok(None),
                }
            }
        }
    }

    /// Get the SHA of a ref on a remote (refs/heads/<branch>).
    #[allow(dead_code)]
    fn get_remote_ref_sha(
        repo_path: &str,
        remote_name: &str,
        branch_name: &str,
    ) -> Result<String, GitAdapterError> {
        Self::git_command(
            repo_path,
            &[
                "rev-parse",
                &format!("refs/remotes/{}/{}", remote_name, branch_name),
            ],
        )
    }

    /// Check if a remote exists.
    fn remote_exists(repo_path: &str, remote_name: &str) -> Result<bool, GitAdapterError> {
        // Use git remote -v to list remotes (works on older git versions)
        let output = Self::git_command(repo_path, &["remote", "-v"])?;
        Ok(output
            .lines()
            .any(|line| line.starts_with(&format!("{}\t", remote_name))))
    }

    /// Get the current tracking branch for a local branch.
    #[allow(dead_code)]
    fn get_tracking_branch(
        repo_path: &str,
        _branch_name: &str,
    ) -> Result<Option<String>, GitAdapterError> {
        let output = Self::git_command(repo_path, &["rev-parse", "{upstream}"])?;
        if output.trim().is_empty() || output.contains("fatal: no upstream") {
            Ok(None)
        } else {
            Ok(Some(output.trim().to_string()))
        }
    }

    /// Extract repo_path from a RollbackTarget::GitRef.
    fn extract_git_target(
        target: &RollbackTarget,
    ) -> Result<(String, Option<String>, Option<String>), GitAdapterError> {
        match target {
            RollbackTarget::GitRef {
                repo_path,
                before_ref,
                after_ref,
            } => Ok((repo_path.clone(), before_ref.clone(), after_ref.clone())),
            _ => Err(GitAdapterError::Validation(format!(
                "expected GitRef target, got {:?}",
                target
            ))),
        }
    }

    /// Validate that repo_path points to a valid git repository.
    fn validate_repo(repo_path: &str) -> Result<(), GitAdapterError> {
        let output = Self::git_command(repo_path, &["rev-parse", "--is-inside-work-tree"])?;
        if output == "true" {
            Ok(())
        } else {
            Err(GitAdapterError::Validation(format!(
                "path '{}' is not a git work tree",
                repo_path
            )))
        }
    }
}

#[cfg(any(test, feature = "unsafe-unbounded-adapters"))]
impl Default for GitRollbackAdapter {
    fn default() -> Self {
        Self::new_unbounded()
    }
}

#[async_trait]
impl RollbackAdapter for GitRollbackAdapter {
    fn key(&self) -> &'static str {
        ADAPTER_KEY
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        let (repo_path, before_ref, _after_ref) =
            Self::extract_git_target(&request.target).map_err(AdapterError::from)?;
        let repo_path = self.canonical_repo_path_string(&repo_path)?;

        // Validate the repo exists and is a valid git work tree
        Self::validate_repo(&repo_path).map_err(AdapterError::from)?;

        // Capture current HEAD SHA as before_ref if not already set in target
        let captured_before_ref = if let Some(ref_str) = before_ref {
            ref_str.clone()
        } else {
            Self::get_head_sha(&repo_path).map_err(AdapterError::from)?
        };

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert(
            "before_ref".to_string(),
            serde_json::json!(captured_before_ref),
        );

        // For GitBranchCreate: branch_name may come from either request.metadata (direct adapter
        // calls) or execute payload (gateway flow). Support both paths.
        // During prepare, validate branch_name if provided (non-empty), otherwise defer to execute.
        if matches!(request.action_type, ActionType::GitBranchCreate) {
            let has_explicit_base_ref = request
                .metadata
                .get("base_ref")
                .and_then(|v| v.as_str())
                .is_some();

            // Fail-closed: require branch_name in metadata (direct adapter call or gateway flow).
            let branch_name = match request.metadata.get("branch_name").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s,
                _ => {
                    return Err(AdapterError::Validation(
                        "branch_name is required in request.metadata for GitBranchCreate"
                            .to_string(),
                    ));
                }
            };

            // Fail-closed: validate branch name using git-native rules during prepare.
            Self::validate_branch_name(branch_name).map_err(AdapterError::from)?;

            // Fail-closed: check if branch already exists locally during prepare.
            if Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(format!(
                    "branch '{}' already exists locally",
                    branch_name
                )));
            }

            // Store in adapter_metadata so it gets copied to contract.metadata
            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));

            // Resolve base_ref to SHA and persist for verification.
            // If base_ref is provided in metadata, use it; otherwise default to HEAD.
            let base_ref = request
                .metadata
                .get("base_ref")
                .and_then(|v| v.as_str())
                .unwrap_or("HEAD");

            match Self::resolve_ref_to_commit_sha(&repo_path, base_ref) {
                Ok(resolved_sha) => {
                    metadata.insert("base_ref".to_string(), serde_json::json!(base_ref));
                    metadata.insert("base_ref_sha".to_string(), serde_json::json!(resolved_sha));
                    if !has_explicit_base_ref {
                        metadata.insert("implicit_base_ref".to_string(), serde_json::json!(true));
                    }
                }
                Err(e) => {
                    return Err(AdapterError::Validation(format!(
                        "invalid or unresolvable base_ref '{}': {}",
                        base_ref, e
                    )));
                }
            }

            // Bounded repo-state guard: fail-closed if in detached HEAD state with
            // implicit HEAD base (no explicit base_ref provided).
            if !has_explicit_base_ref
                && Self::is_detached_head(&repo_path).map_err(AdapterError::from)?
            {
                return Err(AdapterError::Validation(
                    "cannot create branch in detached HEAD state without explicit base_ref; \
                     provide explicit base_ref to specify the commit for branch creation"
                        .to_string(),
                ));
            }
        }

        // For GitTagCreate: require tag_name in request.metadata (contract metadata)
        if matches!(request.action_type, ActionType::GitTagCreate) {
            match request.metadata.get("tag_name").and_then(|v| v.as_str()) {
                Some(tag_name) if !tag_name.is_empty() => {
                    // Fail-closed: validate tag name using git-native rules during prepare
                    Self::validate_tag_name(tag_name).map_err(AdapterError::from)?;

                    // Fail-closed: reject if tag already exists locally
                    if Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)? {
                        return Err(AdapterError::Validation(format!(
                            "tag '{}' already exists locally",
                            tag_name
                        )));
                    }

                    // For tag creation, get HEAD SHA to capture what the tag will point to
                    let head_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                    metadata.insert("tag_target_sha".to_string(), serde_json::json!(head_sha));

                    // Store tag_name in metadata so execute/verify/rollback can use it
                    metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                }
                _ => {
                    return Err(AdapterError::Validation(
                        "tag_name is required in request.metadata for GitTagCreate".to_string(),
                    ));
                }
            }
        }

        // For GitTagDelete: require tag_name in request.metadata and ensure it exists
        if matches!(request.action_type, ActionType::GitTagDelete) {
            match request.metadata.get("tag_name").and_then(|v| v.as_str()) {
                Some(tag_name) if !tag_name.is_empty() => {
                    // Fail-closed: validate tag name format
                    Self::validate_tag_name(tag_name).map_err(AdapterError::from)?;

                    // Fail-closed: tag MUST exist for deletion
                    if !Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)? {
                        return Err(AdapterError::Validation(format!(
                            "tag '{}' does not exist locally",
                            tag_name
                        )));
                    }

                    // Capture the tag's SHA so we can recreate it on rollback
                    let tag_sha = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("refs/tags/{}", tag_name)],
                    )
                    .map_err(AdapterError::from)?;
                    metadata.insert("tag_sha".to_string(), serde_json::json!(tag_sha));
                    metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                }
                _ => {
                    return Err(AdapterError::Validation(
                        "tag_name is required in request.metadata for GitTagDelete".to_string(),
                    ));
                }
            }
        }

        // For GitBranchDelete: require branch_name in request.metadata and ensure it exists
        if matches!(request.action_type, ActionType::GitBranchDelete) {
            match request.metadata.get("branch_name").and_then(|v| v.as_str()) {
                Some(branch_name) if !branch_name.is_empty() => {
                    // Fail-closed: validate branch name format
                    Self::validate_branch_name(branch_name).map_err(AdapterError::from)?;

                    // Fail-closed: branch MUST exist for deletion
                    if !Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                        return Err(AdapterError::Validation(format!(
                            "branch '{}' does not exist locally",
                            branch_name
                        )));
                    }

                    // Fail-closed: cannot delete the currently checked-out branch
                    let current_branch =
                        Self::get_current_branch(&repo_path).map_err(AdapterError::from)?;
                    if current_branch.as_deref() == Some(branch_name) {
                        return Err(AdapterError::Validation(format!(
                            "cannot delete branch '{}': it is currently checked out",
                            branch_name
                        )));
                    }

                    // Fail-closed: cannot delete in detached HEAD state
                    if Self::is_detached_head(&repo_path).map_err(AdapterError::from)? {
                        return Err(AdapterError::Validation(
                            "cannot delete branch in detached HEAD state".to_string(),
                        ));
                    }

                    // Capture the branch tip SHA so we can recreate it on rollback
                    let branch_tip_sha = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    )
                    .map_err(AdapterError::from)?;
                    metadata.insert(
                        "branch_tip_sha".to_string(),
                        serde_json::json!(branch_tip_sha),
                    );
                    // Use delete_branch_name key to avoid collision with GitBranchCreate's
                    // execute code which reads branch_name from metadata
                    metadata.insert(
                        "delete_branch_name".to_string(),
                        serde_json::json!(branch_name),
                    );
                }
                _ => {
                    return Err(AdapterError::Validation(
                        "branch_name is required in request.metadata for GitBranchDelete"
                            .to_string(),
                    ));
                }
            }
        }

        // For GitPush: require branch_name and remote_name in metadata
        if matches!(request.action_type, ActionType::GitPush) {
            let branch_name = request.metadata.get("branch_name").and_then(|v| v.as_str());
            let remote_name = request
                .metadata
                .get("remote_name")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");

            if branch_name.is_none() || branch_name.map(|s| s.is_empty()).unwrap_or(false) {
                return Err(AdapterError::Validation(
                    "branch_name is required in request.metadata for GitPush".to_string(),
                ));
            }

            let branch_name = branch_name.unwrap();
            let remote_name = remote_name.to_string();

            // Fail-closed: cannot push in detached HEAD state
            if Self::is_detached_head(&repo_path).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(
                    "cannot push in detached HEAD state".to_string(),
                ));
            }

            // Fail-closed: check remote exists
            if !Self::remote_exists(&repo_path, &remote_name).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(format!(
                    "remote '{}' does not exist",
                    remote_name
                )));
            }

            // Capture local branch tip SHA
            let local_sha = Self::git_command(
                &repo_path,
                &["rev-parse", &format!("{}^{{commit}}", branch_name)],
            )
            .map_err(AdapterError::from)?;

            // Capture remote branch SHA (if exists)
            let remote_sha = Self::git_command(
                &repo_path,
                &[
                    "rev-parse",
                    &format!("refs/remotes/{}/{}", remote_name, branch_name),
                ],
            )
            .ok();

            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
            metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
            metadata.insert("local_sha".to_string(), serde_json::json!(local_sha));
            if let Some(sha) = remote_sha {
                metadata.insert("remote_sha".to_string(), serde_json::json!(sha));
            }
        }

        // For GitPull: require branch_name and remote_name in metadata
        if matches!(request.action_type, ActionType::GitPull) {
            let branch_name = request.metadata.get("branch_name").and_then(|v| v.as_str());
            let remote_name = request
                .metadata
                .get("remote_name")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");

            if branch_name.is_none() || branch_name.map(|s| s.is_empty()).unwrap_or(false) {
                return Err(AdapterError::Validation(
                    "branch_name is required in request.metadata for GitPull".to_string(),
                ));
            }

            let branch_name = branch_name.unwrap();
            let remote_name = remote_name.to_string();

            // Fail-closed: cannot pull with dirty worktree
            if Self::is_worktree_dirty(&repo_path).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(
                    "cannot pull with dirty worktree; commit or stash changes first".to_string(),
                ));
            }

            // Fail-closed: check remote exists
            if !Self::remote_exists(&repo_path, &remote_name).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(format!(
                    "remote '{}' does not exist",
                    remote_name
                )));
            }

            // Capture current HEAD SHA
            let head_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;

            // Capture remote branch SHA (if exists)
            let remote_sha = Self::git_command(
                &repo_path,
                &[
                    "rev-parse",
                    &format!("refs/remotes/{}/{}", remote_name, branch_name),
                ],
            )
            .ok();

            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
            metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
            metadata.insert("before_head_sha".to_string(), serde_json::json!(head_sha));
            if let Some(sha) = remote_sha {
                metadata.insert("remote_sha".to_string(), serde_json::json!(sha));
            }
        }

        // For GitFetch: require branch_name and remote_name in metadata
        if matches!(request.action_type, ActionType::GitFetch) {
            let branch_name = request.metadata.get("branch_name").and_then(|v| v.as_str());
            let remote_name = request
                .metadata
                .get("remote_name")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");

            if branch_name.is_none() || branch_name.map(|s| s.is_empty()).unwrap_or(false) {
                return Err(AdapterError::Validation(
                    "branch_name is required in request.metadata for GitFetch".to_string(),
                ));
            }

            let branch_name = branch_name.unwrap();
            let remote_name = remote_name.to_string();

            // Fail-closed: check remote exists
            if !Self::remote_exists(&repo_path, &remote_name).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(format!(
                    "remote '{}' does not exist",
                    remote_name
                )));
            }

            // Check if local ref exists (for rollback decision)
            let local_ref_exists = Self::git_command(
                &repo_path,
                &["rev-parse", &format!("{}^{{commit}}", branch_name)],
            )
            .is_ok();

            // Capture pre-fetch local ref if it exists (for restore on rollback)
            let pre_fetch_ref = if local_ref_exists {
                Some(
                    Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    )
                    .map_err(AdapterError::from)?,
                )
            } else {
                None
            };

            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
            metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
            metadata.insert(
                "local_ref_existed".to_string(),
                serde_json::json!(local_ref_exists),
            );
            if let Some(pre_ref) = pre_fetch_ref {
                metadata.insert("pre_fetch_ref".to_string(), serde_json::json!(pre_ref));
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
        let (repo_path, _, _) =
            Self::extract_git_target(&contract.target).map_err(AdapterError::from)?;
        let repo_path = self.canonical_repo_path_string(&repo_path)?;

        // Validate repo exists
        Self::validate_repo(&repo_path).map_err(AdapterError::from)?;

        // Check if payload is a GitBranchCreate operation
        if let Some(obj) = payload.as_object() {
            // GitBranchCreate: branch_name should come from contract.metadata (set in prepare),
            // but we fall back to payload for backward compatibility.
            let branch_name = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("branch_name").and_then(|v| v.as_str()));

            // GitBranchCreate: handle when action_type matches (GitBranchCreate or GitCommit for backward compat)
            if matches!(
                contract.action_type,
                ActionType::GitBranchCreate | ActionType::GitCommit
            ) {
                if let Some(branch_name) = branch_name {
                    // Use base_ref_sha from contract metadata if available (set during prepare),
                    // otherwise fall back to base_ref string and resolve it now.
                    // If neither is available, default to HEAD and resolve.
                    let base_ref_sha = if let Some(sha) = contract
                        .metadata
                        .get("base_ref_sha")
                        .and_then(|v| v.as_str())
                    {
                        sha.to_string()
                    } else {
                        let base_ref = contract
                            .metadata
                            .get("base_ref")
                            .and_then(|v| v.as_str())
                            .or_else(|| obj.get("base_ref").and_then(|v| v.as_str()))
                            .unwrap_or("HEAD");
                        Self::resolve_ref_to_commit_sha(&repo_path, base_ref)?
                    };

                    let base_ref_for_display = contract
                        .metadata
                        .get("base_ref")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.get("base_ref").and_then(|v| v.as_str()))
                        .unwrap_or("HEAD");

                    // Check if branch already exists
                    if Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                        return Err(AdapterError::Validation(format!(
                            "branch '{}' already exists",
                            branch_name
                        )));
                    }

                    // Create the branch at the resolved SHA
                    Self::git_command(&repo_path, &["branch", branch_name, &base_ref_sha])
                        .map_err(AdapterError::from)?;

                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                    metadata.insert(
                        "base_ref".to_string(),
                        serde_json::json!(base_ref_for_display),
                    );
                    metadata.insert("base_ref_sha".to_string(), serde_json::json!(base_ref_sha));
                    let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                    metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));

                    return Ok(ExecuteReceipt {
                        external_id: Some(branch_name.to_string()),
                        result_digest: Some(current_sha),
                        adapter_metadata: metadata,
                    });
                }
            }

            // GitTagCreate: create a lightweight tag at HEAD
            if matches!(contract.action_type, ActionType::GitTagCreate) {
                if let Some(tag_name) = contract
                    .metadata
                    .get("tag_name")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("tag_name").and_then(|v| v.as_str()))
                {
                    // Create lightweight tag at current HEAD
                    Self::git_command(&repo_path, &["tag", tag_name])
                        .map_err(AdapterError::from)?;

                    // Capture the tag SHA
                    let tag_sha = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("refs/tags/{}", tag_name)],
                    )
                    .map_err(AdapterError::from)?;

                    let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                    metadata.insert("tag_sha".to_string(), serde_json::json!(tag_sha));
                    metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
                    return Ok(ExecuteReceipt {
                        external_id: Some(tag_name.to_string()),
                        result_digest: Some(tag_sha),
                        adapter_metadata: metadata,
                    });
                }
            }

            // GitTagDelete: delete the tag
            if matches!(contract.action_type, ActionType::GitTagDelete) {
                if let Some(tag_name) = contract
                    .metadata
                    .get("tag_name")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("tag_name").and_then(|v| v.as_str()))
                {
                    if !Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)? {
                        // Idempotent: tag already deleted
                        let mut metadata = JsonMap::new();
                        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                        metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                        metadata.insert("idempotent".to_string(), serde_json::json!(true));
                        return Ok(ExecuteReceipt {
                            external_id: None,
                            result_digest: None,
                            adapter_metadata: metadata,
                        });
                    }

                    Self::git_command(&repo_path, &["tag", "-d", tag_name])
                        .map_err(AdapterError::from)?;

                    let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                    metadata.insert("deleted".to_string(), serde_json::json!(true));
                    return Ok(ExecuteReceipt {
                        external_id: None,
                        result_digest: Some(current_sha),
                        adapter_metadata: metadata,
                    });
                }
            }

            // GitBranchDelete: delete the branch
            if matches!(contract.action_type, ActionType::GitBranchDelete) {
                if let Some(branch_name) = contract
                    .metadata
                    .get("delete_branch_name")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("delete_branch_name").and_then(|v| v.as_str()))
                {
                    if !Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                        // Idempotent: branch already deleted
                        let mut metadata = JsonMap::new();
                        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                        metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                        metadata.insert("idempotent".to_string(), serde_json::json!(true));
                        return Ok(ExecuteReceipt {
                            external_id: None,
                            result_digest: None,
                            adapter_metadata: metadata,
                        });
                    }

                    Self::git_command(&repo_path, &["branch", "-D", branch_name])
                        .map_err(AdapterError::from)?;

                    let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                    metadata.insert("deleted".to_string(), serde_json::json!(true));
                    return Ok(ExecuteReceipt {
                        external_id: None,
                        result_digest: Some(current_sha),
                        adapter_metadata: metadata,
                    });
                }
            }

            // GitPush: push branch to remote
            if matches!(contract.action_type, ActionType::GitPush) {
                let branch_name = contract
                    .metadata
                    .get("branch_name")
                    .and_then(|v| v.as_str())
                    .unwrap();
                let remote_name = contract
                    .metadata
                    .get("remote_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin");

                // Get credential helper for this remote (name only, not secrets).
                // This enables git-native credential delegation for authenticated pushes.
                let cred_helper = Self::get_remote_credential_helper(&repo_path, remote_name)
                    .ok()
                    .flatten();

                // Push the branch to remote using env passthrough for auth delegation.
                // GIT_TERMINAL_PROMPT=0 ensures fail-fast if credentials are missing.
                // SSH_AUTH_SOCK forwards SSH agent for key-based auth.
                Self::git_command_with_env(
                    &repo_path,
                    &["push", remote_name, &format!("refs/heads/{}", branch_name)],
                    cred_helper.as_deref(),
                )
                .map_err(AdapterError::from)?;

                // Get the new remote SHA
                let new_remote_sha = Self::git_command(
                    &repo_path,
                    &[
                        "rev-parse",
                        &format!("refs/remotes/{}/{}", remote_name, branch_name),
                    ],
                )
                .map_err(AdapterError::from)?;

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
                metadata.insert("pushed_sha".to_string(), serde_json::json!(new_remote_sha));

                return Ok(ExecuteReceipt {
                    external_id: Some(format!("{}/{}", remote_name, branch_name)),
                    result_digest: Some(new_remote_sha),
                    adapter_metadata: metadata,
                });
            }

            // GitPull: pull from remote
            if matches!(contract.action_type, ActionType::GitPull) {
                let branch_name = contract
                    .metadata
                    .get("branch_name")
                    .and_then(|v| v.as_str())
                    .unwrap();
                let remote_name = contract
                    .metadata
                    .get("remote_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin");

                // Get credential helper for this remote (name only, not secrets).
                let cred_helper = Self::get_remote_credential_helper(&repo_path, remote_name)
                    .ok()
                    .flatten();

                // Pull the branch from remote using env passthrough for auth delegation.
                Self::git_command_with_env(
                    &repo_path,
                    &["pull", remote_name, &format!("refs/heads/{}", branch_name)],
                    cred_helper.as_deref(),
                )
                .map_err(AdapterError::from)?;

                // Get the new local HEAD SHA
                let new_head_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
                metadata.insert("pulled_sha".to_string(), serde_json::json!(new_head_sha));

                return Ok(ExecuteReceipt {
                    external_id: Some(format!("{}/{}", remote_name, branch_name)),
                    result_digest: Some(new_head_sha),
                    adapter_metadata: metadata,
                });
            }

            // GitFetch: fetch from remote
            if matches!(contract.action_type, ActionType::GitFetch) {
                let branch_name = contract
                    .metadata
                    .get("branch_name")
                    .and_then(|v| v.as_str())
                    .unwrap();
                let remote_name = contract
                    .metadata
                    .get("remote_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin");

                // Get credential helper for this remote (name only, not secrets).
                let cred_helper = Self::get_remote_credential_helper(&repo_path, remote_name)
                    .ok()
                    .flatten();

                // Fetch the branch from remote into FETCH_HEAD
                // Note: git fetch does not update local branches directly - it updates
                // remote tracking branches. For this slice, we fetch to update the
                // remote tracking ref and the local branch stays unchanged unless we
                // explicitly reset it.
                Self::git_command_with_env(
                    &repo_path,
                    &["fetch", remote_name, &format!("refs/heads/{}", branch_name)],
                    cred_helper.as_deref(),
                )
                .map_err(AdapterError::from)?;

                // Get the remote tracking ref SHA after fetch
                let remote_tracking_sha = Self::git_command(
                    &repo_path,
                    &[
                        "rev-parse",
                        &format!("refs/remotes/{}/{}", remote_name, branch_name),
                    ],
                )
                .ok();

                // Also check if local branch tip differs from remote tracking ref after fetch
                let local_sha = Self::git_command(
                    &repo_path,
                    &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                )
                .ok();

                let fetched_sha = remote_tracking_sha
                    .clone()
                    .or(local_sha.clone())
                    .unwrap_or_default();

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
                metadata.insert("fetched_sha".to_string(), serde_json::json!(fetched_sha));
                if let Some(local) = local_sha {
                    metadata.insert("local_sha".to_string(), serde_json::json!(local));
                }
                if let Some(remote) = remote_tracking_sha {
                    metadata.insert("remote_tracking_sha".to_string(), serde_json::json!(remote));
                }

                return Ok(ExecuteReceipt {
                    external_id: Some(format!("{}/{}", remote_name, branch_name)),
                    result_digest: Some(fetched_sha),
                    adapter_metadata: metadata,
                });
            }

            // Legacy: capture after_ref from payload
            if let Some(after_ref) = obj.get("after_ref").and_then(|v| v.as_str()) {
                let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("after_ref".to_string(), serde_json::json!(after_ref));
                metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
                return Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: Some(current_sha),
                    adapter_metadata: metadata,
                });
            }
        }

        // For unsupported payloads, return an error rather than silently succeeding
        Err(AdapterError::Unsupported(format!(
            "execute payload type '{}' not supported by git adapter in this slice; \
             provide {{ \"branch_name\": \"<name>\", \"base_ref\": \"<ref>\" }} for branch creation \
             or {{ \"after_ref\": \"<sha>\" }} to capture current HEAD",
            payload
        )))
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let (repo_path, before_ref, after_ref) =
            Self::extract_git_target(&contract.target).map_err(AdapterError::from)?;
        let repo_path = self.canonical_repo_path_string(&repo_path)?;

        Self::validate_repo(&repo_path).map_err(AdapterError::from)?;

        // For GitTagCreate: verify tag exists after creation
        if matches!(contract.action_type, ActionType::GitTagCreate) {
            if let Some(tag_name) = contract.metadata.get("tag_name").and_then(|v| v.as_str()) {
                let exists = Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)?;
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                metadata.insert("tag_exists".to_string(), serde_json::json!(exists));
                metadata.insert("verified".to_string(), serde_json::json!(exists));
                if !exists {
                    metadata.insert(
                        "reason".to_string(),
                        serde_json::json!("tag does not exist after create"),
                    );
                }
                return Ok(VerifyReceipt {
                    verified: exists,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitTagDelete: verify tag is gone after deletion
        if matches!(contract.action_type, ActionType::GitTagDelete) {
            if let Some(tag_name) = contract.metadata.get("tag_name").and_then(|v| v.as_str()) {
                let exists = Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)?;
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                metadata.insert("tag_exists".to_string(), serde_json::json!(exists));
                metadata.insert("verified".to_string(), serde_json::json!(!exists));
                if exists {
                    metadata.insert(
                        "reason".to_string(),
                        serde_json::json!("tag still exists after delete"),
                    );
                }
                return Ok(VerifyReceipt {
                    verified: !exists,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitBranchDelete: verify branch is gone after deletion
        if matches!(contract.action_type, ActionType::GitBranchDelete) {
            if let Some(branch_name) = contract
                .metadata
                .get("delete_branch_name")
                .and_then(|v| v.as_str())
            {
                let exists =
                    Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)?;
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("branch_exists".to_string(), serde_json::json!(exists));
                metadata.insert("verified".to_string(), serde_json::json!(!exists));
                if exists {
                    metadata.insert(
                        "reason".to_string(),
                        serde_json::json!("branch still exists after delete"),
                    );
                }
                return Ok(VerifyReceipt {
                    verified: !exists,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitPush: verify remote ref matches pushed SHA
        if matches!(contract.action_type, ActionType::GitPush) {
            let branch_name = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str());
            let remote_name = contract
                .metadata
                .get("remote_name")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");

            if let Some(branch_name) = branch_name {
                let pushed_sha = contract
                    .metadata
                    .get("pushed_sha")
                    .and_then(|v| v.as_str())
                    .or_else(|| contract.metadata.get("local_sha").and_then(|v| v.as_str()));

                let remote_sha = Self::git_command(
                    &repo_path,
                    &[
                        "rev-parse",
                        &format!("refs/remotes/{}/{}", remote_name, branch_name),
                    ],
                )
                .ok();

                let verified = if let Some(expected) = pushed_sha {
                    remote_sha.as_deref() == Some(expected)
                } else {
                    false
                };

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));
                if let Some(sha) = remote_sha {
                    metadata.insert("remote_sha".to_string(), serde_json::json!(sha));
                }
                metadata.insert("verified".to_string(), serde_json::json!(verified));

                return Ok(VerifyReceipt {
                    verified,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitPull: verify local HEAD was updated after pull
        if matches!(contract.action_type, ActionType::GitPull) {
            let head_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
            let before_head_sha = contract
                .metadata
                .get("before_head_sha")
                .and_then(|v| v.as_str());

            // Verify: HEAD should have changed from the before_head_sha captured during prepare
            let verified = if let Some(before) = before_head_sha {
                head_sha != *before
            } else {
                false
            };

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("head_sha".to_string(), serde_json::json!(head_sha));
            metadata.insert("verified".to_string(), serde_json::json!(verified));

            return Ok(VerifyReceipt {
                verified,
                adapter_metadata: metadata,
            });
        }

        // GitFetch: verify local ref was updated after fetch
        if matches!(contract.action_type, ActionType::GitFetch) {
            let branch_name = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str());
            let pre_fetch_ref = contract
                .metadata
                .get("pre_fetch_ref")
                .and_then(|v| v.as_str());

            let verified = if let Some(before) = pre_fetch_ref {
                // Get current branch tip and compare with pre_fetch_ref
                if let Some(branch_name) = branch_name {
                    let current_tip = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    )
                    .ok();
                    current_tip.as_deref() != Some(before)
                } else {
                    false
                }
            } else {
                false
            };

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            if let Some(bn) = branch_name {
                metadata.insert("branch_name".to_string(), serde_json::json!(bn));
            }
            if let Some(before) = pre_fetch_ref {
                metadata.insert("pre_fetch_ref".to_string(), serde_json::json!(before));
            }
            metadata.insert("verified".to_string(), serde_json::json!(verified));

            return Ok(VerifyReceipt {
                verified,
                adapter_metadata: metadata,
            });
        }

        let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;

        // For GitBranchCreate: verify branch exists and optionally matches expected ref
        if let Some(branch_name) = contract
            .metadata
            .get("branch_name")
            .and_then(|v| v.as_str())
        {
            let branch_exists =
                Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)?;

            // Safety check: verify fail-closes when:
            // 1. On the created branch - rollback would be blocked (cannot delete checked-out branch)
            // 2. In detached HEAD state - rollback would be blocked due to ambiguous HEAD reference
            // Use actual repo state via get_current_branch, not receipt metadata.
            let current_branch =
                Self::get_current_branch(&repo_path).map_err(AdapterError::from)?;
            let on_created_branch = current_branch.as_deref() == Some(branch_name);

            // Also check with explicit helper for clarity
            let in_detached_head_state =
                Self::is_detached_head(&repo_path).map_err(AdapterError::from)?;

            // Determine verification mode and perform verification with rich audit metadata
            #[derive(Debug)]
            enum VerificationMode {
                OnCreatedBranch,
                DetachedHead,
                BaseRefShaMatch,
                BaseRefShaMismatch,
                BaseRefResolveFailed,
                BranchTipResolveFailed,
                NoBaseRefBranchExistsOnly,
                BranchMissing,
            }

            let (verified, mode, expected_sha, actual_branch_tip) = if on_created_branch {
                // Fail closed: if we're on this branch, rollback would be blocked,
                // so verification must fail to prevent an inconsistent state.
                (false, VerificationMode::OnCreatedBranch, None, None)
            } else if in_detached_head_state {
                // Fail closed: in detached HEAD state, rollback would be blocked because
                // branch deletion is unsafe without a stable branch reference to reset to.
                // The created branch may still exist but we're not on a stable branch.
                (false, VerificationMode::DetachedHead, None, None)
            } else if branch_exists {
                // If we have base_ref_sha (from prepare), verify branch tip matches it.
                // This is the fail-closed path: explicit base_ref_sha means we must verify.
                if let Some(expected) = contract
                    .metadata
                    .get("base_ref_sha")
                    .and_then(|v| v.as_str())
                {
                    // Get the SHA that the branch points to
                    let branch_tip = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    );
                    match branch_tip {
                        Ok(tip) => {
                            let matched = tip == expected;
                            (
                                matched,
                                if matched {
                                    VerificationMode::BaseRefShaMatch
                                } else {
                                    VerificationMode::BaseRefShaMismatch
                                },
                                Some(expected.to_string()),
                                Some(tip),
                            )
                        }
                        Err(_) => (
                            false,
                            VerificationMode::BranchTipResolveFailed,
                            Some(expected.to_string()),
                            None,
                        ), // Could not get branch tip - fail closed
                    }
                } else if contract.metadata.get("base_ref").is_some() {
                    // We have base_ref but not base_ref_sha - try to resolve and compare.
                    // This handles backward compatibility for cases where base_ref was set
                    // but not resolved during prepare.
                    let base_ref = contract
                        .metadata
                        .get("base_ref")
                        .and_then(|v| v.as_str())
                        .unwrap();
                    match Self::resolve_ref_to_commit_sha(&repo_path, base_ref) {
                        Ok(expected) => {
                            let branch_tip = Self::git_command(
                                &repo_path,
                                &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                            );
                            match branch_tip {
                                Ok(tip) => {
                                    let matched = tip == expected;
                                    (
                                        matched,
                                        if matched {
                                            VerificationMode::BaseRefShaMatch
                                        } else {
                                            VerificationMode::BaseRefShaMismatch
                                        },
                                        Some(expected),
                                        Some(tip),
                                    )
                                }
                                Err(_) => (
                                    false,
                                    VerificationMode::BranchTipResolveFailed,
                                    Some(expected),
                                    None,
                                ),
                            }
                        }
                        Err(_) => (false, VerificationMode::BaseRefResolveFailed, None, None), // Could not resolve base_ref - fail closed
                    }
                } else {
                    // No base_ref_sha and no base_ref - just verify branch exists.
                    // This is the minimal verification path for backward compatibility.
                    (
                        true,
                        VerificationMode::NoBaseRefBranchExistsOnly,
                        None,
                        None,
                    )
                }
            } else {
                (false, VerificationMode::BranchMissing, None, None) // Branch doesn't exist
            };

            let mode_str = match mode {
                VerificationMode::OnCreatedBranch => "on_created_branch",
                VerificationMode::DetachedHead => "detached_head",
                VerificationMode::BaseRefShaMatch => "base_ref_sha_match",
                VerificationMode::BaseRefShaMismatch => "base_ref_sha_mismatch",
                VerificationMode::BaseRefResolveFailed => "base_ref_resolve_failed",
                VerificationMode::BranchTipResolveFailed => "branch_tip_resolve_failed",
                VerificationMode::NoBaseRefBranchExistsOnly => "no_base_ref_branch_exists_only",
                VerificationMode::BranchMissing => "branch_missing",
            };

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
            metadata.insert(
                "branch_exists".to_string(),
                serde_json::json!(branch_exists),
            );
            metadata.insert("verified".to_string(), serde_json::json!(verified));
            metadata.insert(
                "on_created_branch".to_string(),
                serde_json::json!(on_created_branch),
            );
            metadata.insert(
                "detached_head".to_string(),
                serde_json::json!(in_detached_head_state),
            );
            // Rich audit metadata: verification mode and SHA details
            metadata.insert("verification_mode".to_string(), serde_json::json!(mode_str));
            if let Some(expected) = &expected_sha {
                metadata.insert("expected_sha".to_string(), serde_json::json!(expected));
            }
            if let Some(actual) = &actual_branch_tip {
                metadata.insert("actual_branch_tip".to_string(), serde_json::json!(actual));
            }
            if let Some(implicit) = contract
                .metadata
                .get("implicit_base_ref")
                .and_then(|v| v.as_bool())
            {
                metadata.insert("implicit_base_ref".to_string(), serde_json::json!(implicit));
            }

            return Ok(VerifyReceipt {
                verified,
                adapter_metadata: metadata,
            });
        }

        // Original ref-based verification for non-branch-create/non-tag operations
        // Determine the expected ref: prefer after_ref if present, otherwise before_ref
        let expected_ref = after_ref.as_ref().or(before_ref.as_ref());

        let _verified = match expected_ref {
            Some(expected) => current_sha == *expected,
            None => {
                // No refs available to verify against - conservatively fail closed
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: {
                        let mut m = JsonMap::new();
                        m.insert("repo_path".to_string(), serde_json::json!(repo_path));
                        m.insert("current_ref".to_string(), serde_json::json!(current_sha));
                        m.insert(
                            "reason".to_string(),
                            serde_json::json!("no before_ref or after_ref set in target"),
                        );
                        m
                    },
                });
            }
        };

        // Original ref-based verification for non-branch-create/non-tag operations
        // Determine the expected ref: prefer after_ref if present, otherwise before_ref
        let expected_ref = after_ref.as_ref().or(before_ref.as_ref());

        let verified = match expected_ref {
            Some(expected) => current_sha == *expected,
            None => {
                // No refs available to verify against - conservatively fail closed
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: {
                        let mut m = JsonMap::new();
                        m.insert("repo_path".to_string(), serde_json::json!(repo_path));
                        m.insert("current_ref".to_string(), serde_json::json!(current_sha));
                        m.insert(
                            "reason".to_string(),
                            serde_json::json!("no before_ref or after_ref set in target"),
                        );
                        m
                    },
                });
            }
        };

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
        if let Some(expected) = expected_ref {
            metadata.insert("expected_ref".to_string(), serde_json::json!(expected));
        }
        metadata.insert("verified".to_string(), serde_json::json!(verified));

        Ok(VerifyReceipt {
            verified,
            adapter_metadata: metadata,
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Compensate is the same as rollback for this slice
        self.rollback(contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        let (repo_path, before_ref, _) =
            Self::extract_git_target(&contract.target).map_err(AdapterError::from)?;
        let repo_path = self.canonical_repo_path_string(&repo_path)?;

        Self::validate_repo(&repo_path).map_err(AdapterError::from)?;

        // Check if this is a branch creation rollback (branch_name in metadata)
        // Only handle for GitBranchCreate to avoid conflicting with GitPush/GitPull
        // which also have branch_name in metadata from prepare()
        // GitBranchCreate rollback: also handle GitCommit for backward compat with old tests
        if matches!(
            contract.action_type,
            ActionType::GitBranchCreate | ActionType::GitCommit
        ) {
            if let Some(branch_name) = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str())
            {
                let current_branch =
                    Self::get_current_branch(&repo_path).map_err(AdapterError::from)?;
                if current_branch.as_deref() == Some(branch_name) {
                    return Err(AdapterError::Validation(format!(
                        "cannot delete branch '{}': it is currently checked out",
                        branch_name
                    )));
                }

                if Self::is_detached_head(&repo_path).map_err(AdapterError::from)? {
                    return Err(AdapterError::Validation(format!(
                        "cannot delete branch '{}': repository is in detached HEAD state; rollback requires a stable branch reference",
                        branch_name
                    )));
                }

                if !Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                    metadata.insert("idempotent".to_string(), serde_json::json!(true));
                    return Ok(RecoveryReceipt {
                        recovered: true,
                        adapter_metadata: metadata,
                    });
                }

                if !Self::can_branch_be_safe_deleted(&repo_path, branch_name)
                    .map_err(AdapterError::from)?
                {
                    return Err(AdapterError::Validation(format!(
                        "cannot delete branch '{}': branch has diverged/unmerged commits; \
                     safe deletion policy rejects deletion to prevent data loss; \
                     manual cleanup required",
                        branch_name
                    )));
                }

                Self::git_command(&repo_path, &["branch", "-d", branch_name])
                    .map_err(AdapterError::from)?;

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("deleted".to_string(), serde_json::json!(true));

                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitPush rollback: delete the remote ref (if we can)
        if matches!(contract.action_type, ActionType::GitPush) {
            let branch_name = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str());
            let remote_name = contract
                .metadata
                .get("remote_name")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");

            if let Some(branch_name) = branch_name {
                // Get credential helper for this remote (name only, not secrets).
                let cred_helper = Self::get_remote_credential_helper(&repo_path, remote_name)
                    .ok()
                    .flatten();

                // Try to delete the remote ref, but fail closed if it doesn't work
                // since we may not have permission to delete on the remote.
                // Uses env passthrough for auth delegation.
                let delete_result = Self::git_command_with_env(
                    &repo_path,
                    &["push", remote_name, &format!(":refs/heads/{}", branch_name)],
                    cred_helper.as_deref(),
                );

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("remote_name".to_string(), serde_json::json!(remote_name));

                match delete_result {
                    Ok(_) => {
                        metadata.insert("rolled_back".to_string(), serde_json::json!(true));
                        return Ok(RecoveryReceipt {
                            recovered: true,
                            adapter_metadata: metadata,
                        });
                    }
                    Err(e) => {
                        // Fail closed: if we can't roll back the push, return recovered=false
                        // with metadata describing the failure, matching fs/sqlite recovery pattern.
                        // This differs from the old behavior which propagated the error.
                        metadata.insert("rollback_failed".to_string(), serde_json::json!(true));
                        metadata.insert(
                            "failure_reason".to_string(),
                            serde_json::json!(format!(
                                "could not delete remote ref {}/{}: {}",
                                remote_name, branch_name, e
                            )),
                        );
                        return Ok(RecoveryReceipt {
                            recovered: false,
                            adapter_metadata: metadata,
                        });
                    }
                }
            }
        }

        // GitPull rollback: reset HEAD to captured before_ref
        if matches!(contract.action_type, ActionType::GitPull) {
            let before_head_sha = contract
                .metadata
                .get("before_head_sha")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "before_head_sha is required in contract.metadata for GitPull rollback"
                            .to_string(),
                    )
                })?;

            // Check if worktree is dirty
            if Self::is_worktree_dirty(&repo_path).map_err(AdapterError::from)? {
                return Err(AdapterError::Validation(
                    "rollback rejected: worktree has uncommitted changes; \
                     commit or stash them before retrying"
                        .to_string(),
                ));
            }

            let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
            if current_sha == before_head_sha {
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert(
                    "before_head_sha".to_string(),
                    serde_json::json!(before_head_sha),
                );
                metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
                metadata.insert("idempotent".to_string(), serde_json::json!(true));
                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            Self::git_command(&repo_path, &["reset", "--hard", before_head_sha])
                .map_err(AdapterError::from)?;

            let after_reset_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert(
                "before_head_sha".to_string(),
                serde_json::json!(before_head_sha),
            );
            metadata.insert(
                "current_ref".to_string(),
                serde_json::json!(after_reset_sha),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // GitFetch rollback: restore local ref to pre-fetch state
        if matches!(contract.action_type, ActionType::GitFetch) {
            let branch_name = contract
                .metadata
                .get("branch_name")
                .and_then(|v| v.as_str());
            let local_ref_existed = contract
                .metadata
                .get("local_ref_existed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let pre_fetch_ref = contract
                .metadata
                .get("pre_fetch_ref")
                .and_then(|v| v.as_str());

            // If ref didn't exist before fetch, nothing to restore
            if !local_ref_existed {
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert(
                    "compensated_with".to_string(),
                    serde_json::json!("no-op (ref did not exist before fetch)"),
                );
                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            // Ref existed before - restore it via reset
            if let Some(pre_ref) = pre_fetch_ref {
                // Get current branch tip
                if let Some(branch_name) = branch_name {
                    let current_tip = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    )
                    .ok();

                    // If already at pre_ref, it's idempotent
                    if current_tip.as_deref() == Some(pre_ref) {
                        let mut metadata = JsonMap::new();
                        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                        metadata.insert("before_head_sha".to_string(), serde_json::json!(pre_ref));
                        metadata.insert("current_ref".to_string(), serde_json::json!(pre_ref));
                        metadata.insert("idempotent".to_string(), serde_json::json!(true));
                        return Ok(RecoveryReceipt {
                            recovered: true,
                            adapter_metadata: metadata,
                        });
                    }

                    // Reset the local branch to its pre-fetch state (fail-closed if it fails)
                    let reset_result = Self::git_command(&repo_path, &["reset", "--hard", pre_ref]);

                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("pre_fetch_ref".to_string(), serde_json::json!(pre_ref));

                    if let Err(e) = reset_result {
                        // Fail closed: if reset fails, return recovered=false
                        // with metadata describing the failure, matching fs/sqlite recovery pattern.
                        // This differs from the old behavior which propagated the error.
                        metadata.insert("rollback_failed".to_string(), serde_json::json!(true));
                        metadata.insert(
                            "failure_reason".to_string(),
                            serde_json::json!(format!(
                                "git reset --hard {} failed for {}: {}",
                                pre_ref, repo_path, e
                            )),
                        );
                        return Ok(RecoveryReceipt {
                            recovered: false,
                            adapter_metadata: metadata,
                        });
                    }

                    let after_reset_sha = Self::git_command(
                        &repo_path,
                        &["rev-parse", &format!("{}^{{commit}}", branch_name)],
                    );

                    if let Err(e) = after_reset_sha {
                        // Reset succeeded but we couldn't verify the result - fail closed
                        // since we cannot confirm the repository is in the expected state.
                        metadata.insert("rollback_failed".to_string(), serde_json::json!(true));
                        metadata.insert(
                            "failure_reason".to_string(),
                            serde_json::json!(format!(
                                "reset succeeded but verification failed for {}: {}",
                                repo_path, e
                            )),
                        );
                        return Ok(RecoveryReceipt {
                            recovered: false,
                            adapter_metadata: metadata,
                        });
                    }

                    let after_reset_sha = after_reset_sha.unwrap();
                    metadata.insert(
                        "current_ref".to_string(),
                        serde_json::json!(after_reset_sha),
                    );
                    metadata.insert(
                        "compensated_with".to_string(),
                        serde_json::json!("reset to pre_fetch_ref"),
                    );

                    return Ok(RecoveryReceipt {
                        recovered: true,
                        adapter_metadata: metadata,
                    });
                }
            }

            // Ref existed but no pre_fetch_ref captured - cannot restore
            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert(
                "compensated_with".to_string(),
                serde_json::json!("no-op (ref existed but no pre_fetch_ref captured)"),
            );
            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // GitTagCreate rollback: delete the created tag
        if matches!(contract.action_type, ActionType::GitTagCreate) {
            if let Some(tag_name) = contract.metadata.get("tag_name").and_then(|v| v.as_str()) {
                if !Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)? {
                    let mut metadata = JsonMap::new();
                    metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                    metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                    metadata.insert("idempotent".to_string(), serde_json::json!(true));
                    return Ok(RecoveryReceipt {
                        recovered: true,
                        adapter_metadata: metadata,
                    });
                }

                Self::git_command(&repo_path, &["tag", "-d", tag_name])
                    .map_err(AdapterError::from)?;

                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                metadata.insert("deleted".to_string(), serde_json::json!(true));

                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }
        }

        // GitTagDelete rollback: recreate the deleted tag at the captured SHA
        if matches!(contract.action_type, ActionType::GitTagDelete) {
            let tag_name = contract
                .metadata
                .get("tag_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "tag_name is required in contract.metadata for GitTagDelete rollback"
                            .to_string(),
                    )
                })?;

            let tag_sha = contract
                .metadata
                .get("tag_sha")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "tag_sha is required in contract.metadata for GitTagDelete rollback"
                            .to_string(),
                    )
                })?;

            if Self::tag_exists(&repo_path, tag_name).map_err(AdapterError::from)? {
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
                metadata.insert("idempotent".to_string(), serde_json::json!(true));
                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            Self::git_command(&repo_path, &["tag", tag_name, tag_sha])
                .map_err(AdapterError::from)?;

            let restored_sha = Self::git_command(
                &repo_path,
                &["rev-parse", &format!("refs/tags/{}", tag_name)],
            )
            .map_err(AdapterError::from)?;

            if restored_sha != tag_sha {
                return Err(AdapterError::Internal(format!(
                    "GitTagDelete rollback SHA mismatch: expected {} but recreated tag points to {}",
                    tag_sha, restored_sha
                )));
            }

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("tag_name".to_string(), serde_json::json!(tag_name));
            metadata.insert("restored_sha".to_string(), serde_json::json!(restored_sha));

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // GitBranchDelete rollback: recreate the deleted branch at the captured SHA
        if matches!(contract.action_type, ActionType::GitBranchDelete) {
            let branch_name = contract
                .metadata
                .get("delete_branch_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "delete_branch_name is required in contract.metadata for GitBranchDelete rollback"
                            .to_string(),
                    )
                })?;

            let branch_tip_sha = contract
                .metadata
                .get("branch_tip_sha")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "branch_tip_sha is required in contract.metadata for GitBranchDelete rollback"
                            .to_string(),
                    )
                })?;

            if Self::branch_exists(&repo_path, branch_name).map_err(AdapterError::from)? {
                let mut metadata = JsonMap::new();
                metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
                metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
                metadata.insert("idempotent".to_string(), serde_json::json!(true));
                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            Self::git_command(&repo_path, &["branch", branch_name, branch_tip_sha])
                .map_err(AdapterError::from)?;

            let restored_sha = Self::git_command(
                &repo_path,
                &["rev-parse", &format!("{}^{{commit}}", branch_name)],
            )
            .map_err(AdapterError::from)?;

            if restored_sha != branch_tip_sha {
                return Err(AdapterError::Internal(format!(
                    "GitBranchDelete rollback SHA mismatch: expected {} but recreated branch points to {}",
                    branch_tip_sha, restored_sha
                )));
            }

            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("branch_name".to_string(), serde_json::json!(branch_name));
            metadata.insert("restored_sha".to_string(), serde_json::json!(restored_sha));

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Original ref-reset rollback for non-branch-create/non-tag operations
        let ref_to_reset_to = before_ref.as_ref().ok_or_else(|| {
            GitAdapterError::Validation("no before_ref set in target".to_string())
        })?;

        if Self::is_worktree_dirty(&repo_path).map_err(AdapterError::from)? {
            return Err(AdapterError::Validation(
                "rollback rejected: worktree has uncommitted changes; \
                 commit or stash them before retrying"
                    .to_string(),
            ));
        }

        let current_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;
        if current_sha == *ref_to_reset_to {
            let mut metadata = JsonMap::new();
            metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
            metadata.insert("before_ref".to_string(), serde_json::json!(ref_to_reset_to));
            metadata.insert("current_ref".to_string(), serde_json::json!(current_sha));
            metadata.insert("idempotent".to_string(), serde_json::json!(true));
            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        Self::git_command(&repo_path, &["reset", "--hard", ref_to_reset_to])
            .map_err(AdapterError::from)?;

        let after_reset_sha = Self::get_head_sha(&repo_path).map_err(AdapterError::from)?;

        let mut metadata = JsonMap::new();
        metadata.insert("repo_path".to_string(), serde_json::json!(repo_path));
        metadata.insert("before_ref".to_string(), serde_json::json!(ref_to_reset_to));
        metadata.insert(
            "current_ref".to_string(),
            serde_json::json!(after_reset_sha),
        );

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: metadata,
        })
    }
}

/// Register an unbounded adapter with a registry for tests or explicit unsafe builds.
#[cfg(any(test, feature = "unsafe-unbounded-adapters"))]
pub fn register_git_adapter(registry: &mut ferrum_rollback::AdapterRegistry) {
    registry.register(std::sync::Arc::new(GitRollbackAdapter::new_unbounded()));
}

#[cfg(test)]
mod tests;
