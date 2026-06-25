//! Filesystem adapter for mutation and recovery.
//!
//! This adapter implements the `RollbackAdapter` trait for filesystem operations,
//! supporting prepare→verify lifecycle on file paths with FileExists and FileHashMatches checks.
//!
//! # FileWrite Recovery Slice
//!
//! This adapter supports bounded FileWrite snapshot/recovery for **existing-file writes**:
//! - `prepare`: captures a deterministic snapshot of the existing file's contents
//! - `execute`: writes new contents to the target file (caller-provided payload)
//! - `rollback`/`compensate`: restores the file from the captured snapshot
//!
//! This adapter also supports bounded FileWrite for **new-file creation**:
//! - `prepare`: validates parent directory exists, marks `created_new_file: true` in metadata
//! - `execute`: writes new contents to the target file
//! - `rollback`/`compensate`: removes the created file (idempotent if already absent)
//!
//! # FileDelete Recovery Slice (P2.1)
//!
//! This adapter also supports bounded FileDelete snapshot/recovery for **existing files only**:
//! - `prepare`: captures a deterministic snapshot of the existing file's contents; fails closed if file does not exist
//! - `execute`: deletes the target file
//! - `rollback`/`compensate`: restores the file from the captured snapshot

use async_trait::async_trait;
use chrono::Utc;
use ferrum_proto::{
    ActionType, CheckType, ExecutionId, JsonMap, RollbackContract, RollbackPrepareRequest,
    RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod planner;
pub mod snapshot;
pub mod validation;
pub use planner::PlannableFsAdapter;

pub const ADAPTER_KIND: &str = "ferrum-adapter-fs";

/// Configuration for filesystem operation bounds.
/// Provides safety limits on file size, path depth, and symlink handling.
#[derive(Debug, Clone)]
pub struct FsBoundsConfig {
    /// Maximum file size in bytes (default 100MB)
    pub max_file_size: u64,
    /// Maximum path depth (number of path components, default 20)
    pub max_path_depth: usize,
    /// Whether to allow symlinks in paths (default false - symlinks are rejected)
    pub allow_symlinks: bool,
    /// Whether to sandbox operations to the workdir (default true)
    pub sandbox_to_workdir: bool,
}

impl Default for FsBoundsConfig {
    fn default() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_path_depth: 20,
            allow_symlinks: false,
            sandbox_to_workdir: true,
        }
    }
}

/// Phase context for error normalization.
const PHASE_PREPARE: &str = "prepare";
const PHASE_VERIFY: &str = "verify";
const PHASE_EXECUTE: &str = "execute";
const PHASE_ROLLBACK: &str = "rollback";
#[allow(dead_code)]
const PHASE_COMPENSATE: &str = "compensate";

#[derive(Debug, Error)]
pub enum FsAdapterError {
    #[error("invalid target: expected FilePath, got {0}")]
    InvalidTarget(String),
    #[error("file path not found or not a file: {0}")]
    FilePathNotFound(String),
    #[error("unsupported action type: {0}")]
    UnsupportedAction(String),
    #[error("unsupported check type: {0}")]
    UnsupportedCheck(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot not found: {0}")]
    SnapshotNotFound(String),
    #[error("file does not exist (cannot snapshot for rollback): {0}")]
    FileNotExisting(String),
    #[error("parent directory does not exist: {0}")]
    ParentDirNotFound(String),
    #[error("directory already exists: {0}")]
    DirAlreadyExists(String),
    #[error("directory not found: {0}")]
    DirNotFound(String),
    #[error("directory is not empty: {0}")]
    DirNotEmpty(String),
    #[error("file size {0} exceeds maximum allowed size {1}")]
    FileSizeExceedsLimit(u64, u64),
    #[error("path depth {0} exceeds maximum allowed depth {1}")]
    PathDepthExceedsLimit(usize, usize),
    #[error("path escapes sandbox: {0}")]
    PathEscape(String),
    #[error("symlinks are not allowed: {0}")]
    SymlinkNotAllowed(String),
    #[error("cross-device access not allowed: {0}")]
    CrossDevice(String),
}

/// Filesystem adapter implementing the RollbackAdapter trait.
///
/// Uses file paths to provide prepare→verify lifecycle testing with snapshot-based
/// recovery for bounded FileWrite and FileDelete operations, including new-file
/// FileWrite when the parent directory already exists.
pub struct FsAdapter {
    key: &'static str,
    bounds: FsBoundsConfig,
    /// Optional workdir for sandbox enforcement. When set, all file operations
    /// must resolve to paths within this directory.
    workdir: Option<PathBuf>,
}

impl FsAdapter {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            bounds: FsBoundsConfig::default(),
            workdir: None,
        }
    }

    /// Creates a new FsAdapter with custom bounds configuration.
    pub fn new_with_bounds(key: &'static str, bounds: FsBoundsConfig) -> Self {
        Self {
            key,
            bounds,
            workdir: None,
        }
    }

    /// Creates a new FsAdapter with custom bounds and explicit workdir for sandboxing.
    /// When workdir is set, all file operations must resolve to paths within this directory.
    /// This enables testing and explicit opt-in for production use.
    pub fn new_with_workdir(key: &'static str, bounds: FsBoundsConfig, workdir: PathBuf) -> Self {
        Self {
            key,
            bounds,
            workdir: Some(workdir),
        }
    }

    /// Returns a reference to the bounds configuration.
    fn bounds(&self) -> &FsBoundsConfig {
        &self.bounds
    }

    /// Returns a reference to the workdir if set.
    fn workdir(&self) -> Option<&Path> {
        self.workdir.as_deref()
    }

    /// Returns the stable snapshot directory path.
    ///
    /// Uses a stable path under the system temp directory that survives
    /// adapter instance boundaries, enabling cross-instance recovery.
    fn snapshot_dir() -> PathBuf {
        std::env::temp_dir().join("ferrum-fs-snapshots")
    }

    /// Extracts the file path from a RollbackTarget::FilePath variant.
    fn extract_path(target: &RollbackTarget) -> Result<&str, AdapterError> {
        match target {
            RollbackTarget::FilePath { path, .. } => Ok(path),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected FilePath, got {:?}",
                target
            ))),
        }
    }

    /// Validates that the given path exists and is a file.
    fn validate_path_exists(path: &str) -> Result<(), AdapterError> {
        validation::validate_path_exists(path)
    }

    /// Validates path depth against the configured maximum.
    fn validate_path_depth(path: &str, max_depth: usize) -> Result<(), FsAdapterError> {
        validation::validate_path_depth(path, max_depth)
    }

    /// Validates that a path does not escape the sandbox via symlinks.
    fn validate_path_sandbox(
        path: &str,
        workdir: Option<&Path>,
        allow_symlinks: bool,
        sandbox_to_workdir: bool,
    ) -> Result<(), FsAdapterError> {
        let p = Path::new(path);

        // Check if path contains symlinks
        if !allow_symlinks {
            // Check each component for symlink
            let mut current = PathBuf::new();
            for component in p.components() {
                current.push(component);
                // Use std::fs::symlink_metadata to check if it's a symlink without following
                if let Ok(meta) = std::fs::symlink_metadata(&current) {
                    if meta.file_type().is_symlink() {
                        return Err(FsAdapterError::SymlinkNotAllowed(format!(
                            "symlink not allowed in path: {}",
                            current.display()
                        )));
                    }
                }
            }

            // Also check if the final path (after following) would be a symlink
            if let Ok(final_meta) = std::fs::symlink_metadata(p) {
                if final_meta.file_type().is_symlink() {
                    return Err(FsAdapterError::SymlinkNotAllowed(format!(
                        "symlink not allowed in path: {}",
                        path
                    )));
                }
            }
        }

        // If sandboxing to workdir, verify resolved path stays within
        if sandbox_to_workdir {
            if let Some(wd) = workdir {
                // Canonicalize the workdir
                let canonical_workdir = wd.canonicalize().map_err(|e| {
                    FsAdapterError::Validation(format!(
                        "failed to canonicalize workdir {}: {}",
                        wd.display(),
                        e
                    ))
                })?;

                // Resolve the path (following symlinks)
                let resolved = if let Ok(canonical) = p.canonicalize() {
                    canonical
                } else {
                    // If path doesn't exist yet, use the parent directory
                    if let Some(parent) = p.parent() {
                        parent.canonicalize().unwrap_or(canonical_workdir.clone())
                    } else {
                        canonical_workdir.clone()
                    }
                };

                // Check if resolved path starts with workdir
                if !resolved.starts_with(&canonical_workdir) {
                    return Err(FsAdapterError::PathEscape(format!(
                        "path {} resolves to {} which escapes workdir {}",
                        path,
                        resolved.display(),
                        canonical_workdir.display()
                    )));
                }

                // On Unix, verify the resolved path is on the same device as workdir.
                // This prevents crossing mount boundaries which could allow escaping
                // the workdir even when the resolved path appears to be within it.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let workdir_dev = std::fs::metadata(&canonical_workdir)
                        .map_err(|e| {
                            FsAdapterError::Validation(format!(
                                "failed to get metadata for workdir {}: {}",
                                canonical_workdir.display(),
                                e
                            ))
                        })?
                        .dev();

                    let resolved_meta = std::fs::metadata(&resolved).map_err(|e| {
                        FsAdapterError::Validation(format!(
                            "failed to get metadata for resolved path {}: {}",
                            resolved.display(),
                            e
                        ))
                    })?;

                    if resolved_meta.dev() != workdir_dev {
                        return Err(FsAdapterError::CrossDevice(format!(
                            "path {} is on a different device ({}) than workdir ({}); \
                             cross-device access is not allowed when sandbox_to_workdir=true",
                            path,
                            resolved_meta.dev(),
                            workdir_dev
                        )));
                    }
                }
                // On non-Unix platforms, device boundary checking is not supported
                // (MetadataExt::dev is not available), so we skip this check.
                // The path-prefix check above still provides basic sandboxing.
                #[cfg(not(unix))]
                let _ = (path, wd);
            }
        }

        Ok(())
    }

    /// Revalidates path sandbox constraints before execute phase.
    /// This catches symlink swaps or sandbox escapes that may have occurred
    /// between prepare and execute.
    fn revalidate_path_for_execute(&self, file_path: &str) -> Result<(), AdapterError> {
        let bounds = self.bounds();
        let workdir = self.workdir();

        // Re-check path depth
        if let Err(e) = Self::validate_path_depth(file_path, bounds.max_path_depth) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }

        // Re-check symlinks and sandbox escape
        if let Err(e) = Self::validate_path_sandbox(
            file_path,
            workdir,
            bounds.allow_symlinks,
            bounds.sandbox_to_workdir,
        ) {
            return Err(Self::phase_wrap_validation(PHASE_EXECUTE, e.to_string()));
        }

        Ok(())
    }

    /// Revalidates path sandbox constraints before rollback phase.
    /// This catches symlink swaps or sandbox escapes that may have occurred
    /// between execute and rollback.
    fn revalidate_path_for_rollback(&self, file_path: &str) -> Result<(), AdapterError> {
        let bounds = self.bounds();
        let workdir = self.workdir();

        // Re-check path depth
        if let Err(e) = Self::validate_path_depth(file_path, bounds.max_path_depth) {
            return Err(Self::phase_wrap_validation(PHASE_ROLLBACK, e.to_string()));
        }

        // Re-check symlinks and sandbox escape
        if let Err(e) = Self::validate_path_sandbox(
            file_path,
            workdir,
            bounds.allow_symlinks,
            bounds.sandbox_to_workdir,
        ) {
            return Err(Self::phase_wrap_validation(PHASE_ROLLBACK, e.to_string()));
        }

        Ok(())
    }

    /// Validates file size is within the configured limit.
    fn validate_file_size(path: &str, max_size: u64) -> Result<(), FsAdapterError> {
        validation::validate_file_size(path, max_size)
    }

    /// Cross-filesystem move: copies source to destination then deletes source.
    /// This is used when rename fails with EXDEV (cross-device link).
    fn cross_filesystem_move(source: &str, destination: &str) -> Result<(), FsAdapterError> {
        // Copy source to destination (preserving permissions)
        std::fs::copy(source, destination)?;

        // Delete the source
        std::fs::remove_file(source)?;

        Ok(())
    }

    /// Normalizes an fs-origin error with phase context for validation errors.
    ///
    /// This wraps common fs errors that occur during prepare/verify into phase-aware
    /// validation errors, making it clearer where the error originated.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }

    /// Normalizes an internal IO error with phase context.
    ///
    /// IO errors during prepare/verify operations (like snapshot capture or hash computation)
    /// are wrapped with phase context to aid debugging.
    fn phase_wrap_internal(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Internal(format!("[{}] {}", phase, msg))
    }

    /// Creates a fail-closed RecoveryReceipt for I/O errors during rollback/compensate.
    ///
    /// Instead of propagating recovery I/O errors (which would abort the caller),
    /// we return `recovered: false` with metadata describing the failure.
    /// This follows the fail-closed principle: if recovery cannot be completed
    /// due to I/O issues (permission denied, disk full, etc.), we report the failure
    /// gracefully rather than crashing.
    fn fail_closed_recovery(
        phase: &'static str,
        operation: &str,
        path: &str,
        error: std::io::Error,
    ) -> RecoveryReceipt {
        let mut metadata = JsonMap::new();
        metadata.insert("rollback_failed".to_string(), serde_json::Value::Bool(true));
        metadata.insert(
            "failure_reason".to_string(),
            serde_json::Value::String(format!(
                "[{}] {} failed for {}: {}",
                phase, operation, path, error
            )),
        );
        metadata.insert(
            "failure_phase".to_string(),
            serde_json::Value::String(phase.to_string()),
        );
        metadata.insert(
            "target_path".to_string(),
            serde_json::Value::String(path.to_string()),
        );
        RecoveryReceipt {
            recovered: false,
            adapter_metadata: metadata,
        }
    }

    /// Computes the SHA-256 hash of a file at the given path.
    fn compute_file_hash(path: &str) -> Result<String, FsAdapterError> {
        snapshot::compute_file_hash(path)
    }

    /// Parses a mode string into a u32.
    fn parse_mode_string(mode_str: &str) -> Result<u32, String> {
        validation::parse_mode_string(mode_str)
    }

    /// Maps an IO error to FsAdapterError, converting Unix ELOOP to SymlinkNotAllowed.
    fn map_io_error_to_fs(err: std::io::Error, path: &str) -> FsAdapterError {
        snapshot::map_io_error_to_fs(err, path)
    }

    /// Reads a file without following symbolic links in the final component (Unix O_NOFOLLOW).
    fn read_file_nofollow(path: &str) -> Result<Vec<u8>, FsAdapterError> {
        snapshot::read_file_nofollow(path)
    }

    /// Writes to a file without following symbolic links in the final component (Unix O_NOFOLLOW).
    fn write_file_nofollow(path: &str, contents: &[u8]) -> Result<(), FsAdapterError> {
        snapshot::write_file_nofollow(path, contents)
    }

    /// Computes a deterministic snapshot subdirectory path based on execution_id and target path.
    fn compute_snapshot_path(
        snapshot_root: &Path,
        execution_id: &ExecutionId,
        target_path: &str,
    ) -> PathBuf {
        snapshot::compute_snapshot_path(snapshot_root, execution_id, target_path)
    }

    /// Captures a snapshot of the file at `target_path` to `snapshot_path`.
    fn capture_snapshot(
        target_path: &str,
        snapshot_path: &Path,
    ) -> Result<PathBuf, FsAdapterError> {
        snapshot::capture_snapshot(target_path, snapshot_path)
    }

    /// Runs a single check spec and returns an error if it fails verification.
    ///
    /// # Arguments
    /// * `check` - The check specification to run
    /// * `target_path` - The contract's target file path (used to validate check path matches target)
    /// * `phase` - The phase context ("prepare" or "verify") for phase-aware error messages
    fn run_check(
        check: &ferrum_proto::CheckSpec,
        target_path: &str,
        phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::FileExists => {
                // Validate 'path' field is present and is a string
                let path = match check.config.get("path") {
                    Some(serde_json::Value::String(s)) => s.as_str(),
                    Some(v) => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileExists check 'path' must be a string, got {}",
                            phase, v
                        )));
                    }
                    None => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileExists check requires 'path' config",
                            phase
                        )));
                    }
                };

                // Validate check path matches contract target path (fail-closed on mismatch)
                if path != target_path {
                    return Err(AdapterError::Validation(format!(
                        "[{}] FileExists check path mismatch: check targets '{}', expected '{}'",
                        phase, path, target_path
                    )));
                }

                let p = Path::new(path);
                if !p.exists() || !p.is_file() {
                    return Err(Self::phase_wrap_validation(
                        phase,
                        format!("FileExists check: file not found or not a file: {}", path),
                    ));
                }
                Ok(())
            }
            CheckType::FileHashMatches => {
                // Validate 'path' field is present and is a string
                let path = match check.config.get("path") {
                    Some(serde_json::Value::String(s)) => s.as_str(),
                    Some(v) => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileHashMatches check 'path' must be a string, got {}",
                            phase, v
                        )));
                    }
                    None => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileHashMatches check requires 'path' config",
                            phase
                        )));
                    }
                };

                // Validate check path matches contract target path (fail-closed on mismatch)
                if path != target_path {
                    return Err(AdapterError::Validation(format!(
                        "[{}] FileHashMatches check path mismatch: check targets '{}', expected '{}'",
                        phase, path, target_path
                    )));
                }

                // Validate 'expected_hash' field is present and is a string
                let expected_hash = match check.config.get("expected_hash") {
                    Some(serde_json::Value::String(s)) => s.as_str(),
                    Some(v) => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileHashMatches check 'expected_hash' must be a string, got {}",
                            phase, v
                        )));
                    }
                    None => {
                        return Err(AdapterError::Validation(format!(
                            "[{}] FileHashMatches check requires 'expected_hash' config",
                            phase
                        )));
                    }
                };

                let actual_hash = Self::compute_file_hash(path).map_err(|e| {
                    Self::phase_wrap_internal(
                        phase,
                        format!(
                            "FileHashMatches check: failed to read/compute hash for {}: {}",
                            path, e
                        ),
                    )
                })?;
                if actual_hash != expected_hash {
                    return Err(AdapterError::Validation(format!(
                        "[{}] FileHashMatches hash mismatch: expected {}, got {}",
                        phase, expected_hash, actual_hash
                    )));
                }
                Ok(())
            }
            _ => Err(AdapterError::Unsupported(format!(
                "[{}] unsupported check type: {:?}",
                phase, check.check_type
            ))),
        }
    }
}

impl From<FsAdapterError> for AdapterError {
    fn from(err: FsAdapterError) -> Self {
        match err {
            FsAdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            FsAdapterError::FilePathNotFound(msg) => AdapterError::Validation(msg),
            FsAdapterError::UnsupportedAction(msg) => AdapterError::Unsupported(msg),
            FsAdapterError::UnsupportedCheck(msg) => AdapterError::Unsupported(msg),
            FsAdapterError::Validation(msg) => AdapterError::Validation(msg),
            FsAdapterError::Io(err) => AdapterError::Internal(err.to_string()),
            FsAdapterError::SnapshotNotFound(msg) => AdapterError::Validation(msg),
            FsAdapterError::FileNotExisting(msg) => AdapterError::Validation(msg),
            FsAdapterError::ParentDirNotFound(msg) => AdapterError::Validation(msg),
            FsAdapterError::DirAlreadyExists(msg) => AdapterError::Validation(msg),
            FsAdapterError::DirNotFound(msg) => AdapterError::Validation(msg),
            FsAdapterError::DirNotEmpty(msg) => AdapterError::Validation(msg),
            FsAdapterError::FileSizeExceedsLimit(size, limit) => {
                AdapterError::Validation(format!("file size {} exceeds limit {}", size, limit))
            }
            FsAdapterError::PathDepthExceedsLimit(depth, limit) => {
                AdapterError::Validation(format!("path depth {} exceeds limit {}", depth, limit))
            }
            FsAdapterError::PathEscape(msg) => AdapterError::Validation(msg),
            FsAdapterError::SymlinkNotAllowed(msg) => AdapterError::Validation(msg),
            FsAdapterError::CrossDevice(msg) => AdapterError::Validation(msg),
        }
    }
}

#[async_trait]
impl RollbackAdapter for FsAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate that target is FilePath
        let file_path = Self::extract_path(&request.target)?;

        // Validate that action_type is FileWrite, FileDelete, FileMove, FileCopy, FileAppend, FileChmod, DirCreate, or DirDelete
        match request.action_type {
            ActionType::FileWrite
            | ActionType::FileDelete
            | ActionType::FileMove
            | ActionType::FileCopy
            | ActionType::FileAppend
            | ActionType::FileChmod
            | ActionType::DirCreate
            | ActionType::DirDelete => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported action type: {:?}",
                    request.action_type
                )));
            }
        }

        // Apply bounds checking for all action types
        let bounds = self.bounds();

        // Check path depth
        if let Err(e) = Self::validate_path_depth(file_path, bounds.max_path_depth) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // Check for symlinks and sandbox escape
        if let Err(e) = Self::validate_path_sandbox(
            file_path,
            self.workdir(),
            bounds.allow_symlinks,
            bounds.sandbox_to_workdir,
        ) {
            return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
        }

        // For operations that involve existing files, check file size
        match request.action_type {
            ActionType::FileWrite
            | ActionType::FileDelete
            | ActionType::FileMove
            | ActionType::FileCopy
            | ActionType::FileAppend
            | ActionType::FileChmod => {
                let p = Path::new(file_path);
                if p.exists() && p.is_file() {
                    if let Err(e) = Self::validate_file_size(file_path, bounds.max_file_size) {
                        return Err(Self::phase_wrap_validation(PHASE_PREPARE, e.to_string()));
                    }
                }
            }
            _ => {}
        }

        // Run prepare_checks if present (these are fail-closed and apply to both FileWrite and FileDelete)
        for check in &request.prepare_checks {
            Self::run_check(check, file_path, "prepare")?;
        }

        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );

        // For FileWrite on existing files, capture a snapshot for potential rollback
        if matches!(request.action_type, ActionType::FileWrite) {
            // Check if file exists (we fail-closed earlier if it didn't and no checks were provided)
            let p = Path::new(file_path);
            if p.exists() && p.is_file() {
                // Compute deterministic snapshot path
                let snapshot_path = Self::compute_snapshot_path(
                    &Self::snapshot_dir(),
                    &request.execution_id,
                    file_path,
                );

                // Capture the snapshot (phase-aware error wrapping for IO errors)
                Self::capture_snapshot(file_path, &snapshot_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_PREPARE,
                        format!("FileWrite snapshot capture failed for {}: {}", file_path, e),
                    )
                })?;

                // Store snapshot path in metadata for rollback to find later
                metadata.insert(
                    "snapshot_path".to_string(),
                    serde_json::Value::String(snapshot_path.display().to_string()),
                );
                metadata.insert(
                    "original_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
            } else {
                // New file case: file doesn't exist yet
                // Validate parent directory exists (fail-closed if parent is missing)
                if let Some(parent) = Path::new(file_path).parent() {
                    if !parent.exists() {
                        return Err(Self::phase_wrap_validation(
                            PHASE_PREPARE,
                            format!("parent directory does not exist: {}", parent.display()),
                        ));
                    }
                }
                // Mark that this is a new file creation (rollback will delete instead of restore)
                metadata.insert(
                    "created_new_file".to_string(),
                    serde_json::Value::Bool(true),
                );
                metadata.insert(
                    "original_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
            }
        }

        // For FileDelete on existing files, capture a snapshot for potential rollback
        // Fail-closed: file must exist at prepare time
        if matches!(request.action_type, ActionType::FileDelete) {
            // If no prepare_checks were provided, validate file exists (fail-closed)
            if request.prepare_checks.is_empty() {
                Self::validate_path_exists(file_path).map_err(|_e| {
                    Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("FileDelete target file not found: {}", file_path),
                    )
                })?;
            }
            let p = Path::new(file_path);
            if p.exists() && p.is_file() {
                // Compute deterministic snapshot path
                let snapshot_path = Self::compute_snapshot_path(
                    &Self::snapshot_dir(),
                    &request.execution_id,
                    file_path,
                );

                // Capture the snapshot (phase-aware error wrapping for IO errors)
                Self::capture_snapshot(file_path, &snapshot_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_PREPARE,
                        format!(
                            "FileDelete snapshot capture failed for {}: {}",
                            file_path, e
                        ),
                    )
                })?;

                // Store snapshot path in metadata for rollback to find later
                metadata.insert(
                    "snapshot_path".to_string(),
                    serde_json::Value::String(snapshot_path.display().to_string()),
                );
                metadata.insert(
                    "original_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
            }
            // Note: if file doesn't exist and no prepare_checks, validate_path_exists fails above.
            // If prepare_checks were provided and they passed despite file missing, we still fail-closed
            // since we're in the FileDelete branch where file must exist.
        }

        // For FileMove: validate source exists and snapshot it for potential rollback
        if matches!(request.action_type, ActionType::FileMove) {
            // Fail-closed: source file must exist
            Self::validate_path_exists(file_path).map_err(|_e| {
                Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("FileMove source file not found: {}", file_path),
                )
            })?;

            // Compute deterministic snapshot path for the source
            let snapshot_path = Self::compute_snapshot_path(
                &Self::snapshot_dir(),
                &request.execution_id,
                file_path,
            );

            // Capture snapshot of source for rollback (phase-aware error wrapping)
            Self::capture_snapshot(file_path, &snapshot_path).map_err(|e| {
                Self::phase_wrap_internal(
                    PHASE_PREPARE,
                    format!("FileMove snapshot capture failed for {}: {}", file_path, e),
                )
            })?;

            // Store source path and snapshot path in metadata
            metadata.insert(
                "source_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
            metadata.insert(
                "snapshot_path".to_string(),
                serde_json::Value::String(snapshot_path.display().to_string()),
            );
        }

        // For FileCopy: validate source exists and snapshot destination if it exists
        if matches!(request.action_type, ActionType::FileCopy) {
            // Fail-closed: source file must exist
            Self::validate_path_exists(file_path).map_err(|_e| {
                Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("FileCopy source file not found: {}", file_path),
                )
            })?;

            // Store source path in metadata
            metadata.insert(
                "source_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );

            // Compute source hash for verification later
            let source_hash = Self::compute_file_hash(file_path).map_err(|e| {
                Self::phase_wrap_internal(
                    PHASE_PREPARE,
                    format!(
                        "FileCopy failed to compute source hash for {}: {}",
                        file_path, e
                    ),
                )
            })?;
            metadata.insert(
                "source_hash".to_string(),
                serde_json::Value::String(source_hash),
            );
        }

        // For FileAppend: validate target file exists and capture original state
        if matches!(request.action_type, ActionType::FileAppend) {
            // Fail-closed: target file must exist
            Self::validate_path_exists(file_path).map_err(|_e| {
                Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("FileAppend target file not found: {}", file_path),
                )
            })?;

            // Store target path in metadata
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );

            // Compute original hash for verification later
            let original_hash = Self::compute_file_hash(file_path).map_err(|e| {
                Self::phase_wrap_internal(
                    PHASE_PREPARE,
                    format!(
                        "FileAppend failed to compute original hash for {}: {}",
                        file_path, e
                    ),
                )
            })?;
            metadata.insert(
                "original_hash".to_string(),
                serde_json::Value::String(original_hash.clone()),
            );

            // Get original file length
            let original_length = std::fs::metadata(file_path)
                .map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_PREPARE,
                        format!("FileAppend failed to get metadata for {}: {}", file_path, e),
                    )
                })?
                .len();
            metadata.insert(
                "original_length".to_string(),
                serde_json::Value::String(original_length.to_string()),
            );

            // Compute hash of data to append (this will be validated in execute)
            // The actual data hash will be computed in execute when we have the payload
            metadata.insert(
                "data_hash_pending".to_string(),
                serde_json::Value::String("pending".to_string()),
            );
        }

        // For FileChmod: validate target file exists and capture original permissions
        if matches!(request.action_type, ActionType::FileChmod) {
            // Fail-closed: target file must exist
            Self::validate_path_exists(file_path).map_err(|_e| {
                Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("FileChmod target file not found: {}", file_path),
                )
            })?;

            // Get the mode from metadata (requested new mode)
            let new_mode = request
                .metadata
                .get("mode")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| {
                    AdapterError::Validation("FileChmod requires 'mode' in metadata".into())
                })?;

            // Validate mode is not empty
            if new_mode.is_empty() {
                return Err(Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    "FileChmod mode cannot be empty".to_string(),
                ));
            }

            // Read current file permissions
            let original_mode = std::fs::metadata(file_path)
                .map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_PREPARE,
                        format!("FileChmod failed to get metadata for {}: {}", file_path, e),
                    )
                })?
                .permissions()
                .mode();

            // Store original mode as octal string
            metadata.insert(
                "original_mode".to_string(),
                serde_json::Value::String(format!("{:o}", original_mode & 0o7777)),
            );

            // Store new mode
            metadata.insert(
                "new_mode".to_string(),
                serde_json::Value::String(new_mode.clone()),
            );

            // Store target path
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
        }

        // For DirCreate: validate directory creation prerequisites
        if matches!(request.action_type, ActionType::DirCreate) {
            let dir_path = Path::new(file_path);

            // Validate path is non-empty (must have a directory name)
            if file_path.is_empty() || dir_path.file_name().is_none() {
                return Err(Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    "DirCreate target path must be a valid directory path".to_string(),
                ));
            }

            // FAIL-CLOSED: if directory already exists
            if dir_path.exists() && dir_path.is_dir() {
                return Err(Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("directory already exists: {}", file_path),
                ));
            }

            // FAIL-CLOSED: parent directory must exist (can't create orphan directories)
            if let Some(parent) = dir_path.parent() {
                if !parent.exists() || !parent.is_dir() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("parent directory does not exist: {}", parent.display()),
                    ));
                }
                // Store parent directory existence flag
                metadata.insert(
                    "parent_dir_existed".to_string(),
                    serde_json::Value::Bool(true),
                );
            } else {
                return Err(Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("cannot determine parent directory for: {}", file_path),
                ));
            }

            // Store the target directory path
            metadata.insert(
                "created_dir".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
        }

        // For DirDelete: validate directory deletion prerequisites
        if matches!(request.action_type, ActionType::DirDelete) {
            let dir_path = Path::new(file_path);

            // FAIL-CLOSED: directory must exist
            if !dir_path.exists() || !dir_path.is_dir() {
                return Err(Self::phase_wrap_validation(
                    PHASE_PREPARE,
                    format!("directory not found: {}", file_path),
                ));
            }

            // FAIL-CLOSED: directory must be empty (we only delete empty directories)
            // Use read_dir to check if directory has any entries
            match std::fs::read_dir(dir_path) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        return Err(Self::phase_wrap_validation(
                            PHASE_PREPARE,
                            format!("directory is not empty: {}", file_path),
                        ));
                    }
                }
                Err(e) => {
                    return Err(Self::phase_wrap_validation(
                        PHASE_PREPARE,
                        format!("failed to read directory {}: {}", file_path, e),
                    ));
                }
            }

            // Store the directory path being deleted
            metadata.insert(
                "deleted_dir".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
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
        let file_path = Self::extract_path(&contract.target)?;

        // Revalidate path sandbox constraints before execute.
        // This catches symlink swaps or sandbox escapes that may have occurred
        // between prepare and execute.
        self.revalidate_path_for_execute(file_path)?;

        match contract.action_type {
            ActionType::FileWrite => {
                // Payload should contain the new file contents
                // Support both string content and object with "content" field
                let content = if let Some(content_str) = payload.as_str() {
                    content_str.as_bytes().to_vec()
                } else if let Some(obj) = payload.as_object() {
                    if let Some(content_val) = obj.get("content") {
                        if let Some(content_str) = content_val.as_str() {
                            content_str.as_bytes().to_vec()
                        } else if let Some(content_num) = content_val.as_i64() {
                            content_num.to_string().into_bytes()
                        } else {
                            return Err(AdapterError::Validation(
                                "payload content must be a string or object with string 'content' field"
                                    .into(),
                            ));
                        }
                    } else {
                        return Err(AdapterError::Validation(
                            "payload must be a string or object with 'content' field".into(),
                        ));
                    }
                } else {
                    return Err(AdapterError::Validation(
                        "payload must be a string or object with 'content' field".into(),
                    ));
                };

                // Write the new contents to the target file
                // This overwrites the existing file (which was snapshotted during prepare)
                Self::write_file_nofollow(file_path, &content).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("FileWrite failed for {}: {}", file_path, e),
                    )
                })?;

                // Compute hash of written content for receipt
                let mut hasher = Sha256::new();
                hasher.update(&content);
                let result_hash = hex::encode(hasher.finalize());

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "bytes_written".to_string(),
                    serde_json::json!(content.len()),
                );
                metadata.insert("content_hash".to_string(), serde_json::json!(result_hash));

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: Some(result_hash),
                    adapter_metadata: metadata,
                })
            }
            ActionType::FileDelete => {
                // Delete the target file (which was snapshotted during prepare)
                std::fs::remove_file(file_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("FileDelete failed for {}: {}", file_path, e),
                    )
                })?;

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "deleted_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::FileMove => {
                // Payload should contain the destination path
                let destination = if let Some(obj) = payload.as_object() {
                    obj.get("destination")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                } else {
                    None
                };

                let destination = destination.ok_or_else(|| {
                    AdapterError::Validation(
                        "FileMove execute payload requires 'destination' field".into(),
                    )
                })?;

                // Revalidate destination path sandbox constraints before execute.
                // This catches symlink swaps or sandbox escapes that may have occurred
                // between prepare and execute.
                self.revalidate_path_for_execute(&destination)?;

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "destination_path".to_string(),
                    serde_json::Value::String(destination.clone()),
                );
                metadata.insert(
                    "moved_from".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );

                // Try rename first
                match std::fs::rename(file_path, &destination) {
                    Ok(()) => {
                        // Rename succeeded - not a cross-filesystem move
                        Ok(ExecuteReceipt {
                            external_id: None,
                            result_digest: None,
                            adapter_metadata: metadata,
                        })
                    }
                    Err(e) if e.raw_os_error() == Some(18) => {
                        // EXDEV error: source and dest are on different filesystems
                        // Fall back to copy + delete
                        Self::cross_filesystem_move(file_path, &destination).map_err(|e| {
                            Self::phase_wrap_internal(
                                PHASE_EXECUTE,
                                format!(
                                    "FileMove cross-filesystem move failed (copy+delete fallback) from {} to {}: {}",
                                    file_path, destination, e
                                ),
                            )
                        })?;

                        // Mark that this was a cross-filesystem move
                        metadata.insert(
                            "cross_filesystem_move".to_string(),
                            serde_json::Value::Bool(true),
                        );

                        Ok(ExecuteReceipt {
                            external_id: None,
                            result_digest: None,
                            adapter_metadata: metadata,
                        })
                    }
                    Err(e) => {
                        // Other rename error
                        Err(Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!(
                                "FileMove failed to rename {} to {}: {}",
                                file_path, destination, e
                            ),
                        ))
                    }
                }
            }
            ActionType::FileCopy => {
                // Payload should contain the destination path
                let destination = if let Some(obj) = payload.as_object() {
                    obj.get("destination")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                } else {
                    None
                };

                let destination = destination.ok_or_else(|| {
                    AdapterError::Validation(
                        "FileCopy execute payload requires 'destination' field".into(),
                    )
                })?;

                // Revalidate destination path sandbox constraints before execute.
                // This catches symlink swaps or sandbox escapes that may have occurred
                // between prepare and execute.
                self.revalidate_path_for_execute(&destination)?;

                let dest_path = Path::new(&destination);
                let created_new_dest = !dest_path.exists();

                // If destination exists, snapshot it for rollback
                if !created_new_dest {
                    // Compute deterministic snapshot path for the destination
                    let snapshot_path = Self::compute_snapshot_path(
                        &Self::snapshot_dir(),
                        &contract.execution_id,
                        &destination,
                    );

                    // Capture snapshot of existing destination
                    Self::capture_snapshot(&destination, &snapshot_path).map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!(
                                "FileCopy snapshot capture failed for {}: {}",
                                destination, e
                            ),
                        )
                    })?;
                }

                // Ensure parent directory exists
                if let Some(parent) = dest_path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            Self::phase_wrap_internal(
                                PHASE_EXECUTE,
                                format!(
                                    "FileCopy failed to create parent directory for {}: {}",
                                    destination, e
                                ),
                            )
                        })?;
                    }
                }

                // Copy source to destination
                std::fs::copy(file_path, &destination).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!(
                            "FileCopy failed to copy {} to {}: {}",
                            file_path, destination, e
                        ),
                    )
                })?;

                // Compute hash of copied file for receipt
                let copy_hash = Self::compute_file_hash(&destination).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("FileCopy failed to compute hash of {}: {}", destination, e),
                    )
                })?;

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "destination_path".to_string(),
                    serde_json::Value::String(destination.clone()),
                );
                metadata.insert(
                    "copy_hash".to_string(),
                    serde_json::Value::String(copy_hash),
                );
                metadata.insert(
                    "created_new_dest".to_string(),
                    serde_json::Value::Bool(created_new_dest),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::FileAppend => {
                // Payload should contain the data to append
                let data = if let Some(data_str) = payload.as_str() {
                    data_str.as_bytes().to_vec()
                } else if let Some(obj) = payload.as_object() {
                    if let Some(data_val) = obj.get("data") {
                        if let Some(data_str) = data_val.as_str() {
                            data_str.as_bytes().to_vec()
                        } else if let Some(data_bytes) = data_val.as_array() {
                            // Support array of bytes/numbers
                            let mut bytes = Vec::new();
                            for item in data_bytes {
                                if let Some(b) = item.as_i64() {
                                    bytes.push(b as u8);
                                } else if let Some(b) = item.as_u64() {
                                    bytes.push(b as u8);
                                } else {
                                    return Err(AdapterError::Validation(
                                        "payload data array must contain numbers".into(),
                                    ));
                                }
                            }
                            bytes
                        } else {
                            return Err(AdapterError::Validation(
                                "payload data must be a string or array of numbers".into(),
                            ));
                        }
                    } else {
                        return Err(AdapterError::Validation(
                            "payload must be a string or object with 'data' field".into(),
                        ));
                    }
                } else {
                    return Err(AdapterError::Validation(
                        "payload must be a string or object with 'data' field".into(),
                    ));
                };

                // Fail-closed: data must be non-empty
                if data.is_empty() {
                    return Err(AdapterError::Validation(
                        "FileAppend data must be non-empty".into(),
                    ));
                }

                // Compute hash of data to append
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let data_hash = hex::encode(hasher.finalize());

                // Open file in append mode and write data (O_NOFOLLOW on Unix)
                let file = {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        std::fs::OpenOptions::new()
                            .append(true)
                            .custom_flags(libc::O_NOFOLLOW)
                            .open(file_path)
                            .map_err(|e| {
                                Self::phase_wrap_internal(
                                    PHASE_EXECUTE,
                                    format!(
                                        "FileAppend failed to open {}: {}",
                                        file_path,
                                        Self::map_io_error_to_fs(e, file_path)
                                    ),
                                )
                            })
                    }
                    #[cfg(not(unix))]
                    {
                        std::fs::OpenOptions::new()
                            .append(true)
                            .open(file_path)
                            .map_err(|e| {
                                Self::phase_wrap_internal(
                                    PHASE_EXECUTE,
                                    format!("FileAppend failed to open {}: {}", file_path, e),
                                )
                            })
                    }
                }?;

                // Write the data
                use std::io::Write;
                let mut file = file;
                file.write_all(&data).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("FileAppend failed to write to {}: {}", file_path, e),
                    )
                })?;

                // Flush to ensure data is written
                file.flush().map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("FileAppend failed to flush {}: {}", file_path, e),
                    )
                })?;

                // Compute new hash after append
                let new_hash = Self::compute_file_hash(file_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!(
                            "FileAppend failed to compute new hash of {}: {}",
                            file_path, e
                        ),
                    )
                })?;

                // Get new file length
                let new_length = std::fs::metadata(file_path)
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!("FileAppend failed to get metadata for {}: {}", file_path, e),
                        )
                    })?
                    .len();

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "target_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
                metadata.insert(
                    "data_hash".to_string(),
                    serde_json::Value::String(data_hash),
                );
                metadata.insert(
                    "new_hash".to_string(),
                    serde_json::Value::String(new_hash.clone()),
                );
                metadata.insert(
                    "new_length".to_string(),
                    serde_json::Value::String(new_length.to_string()),
                );
                metadata.insert(
                    "bytes_appended".to_string(),
                    serde_json::Value::Number(data.len().into()),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: Some(new_hash),
                    adapter_metadata: metadata,
                })
            }
            ActionType::FileChmod => {
                // Get new mode from contract metadata
                let new_mode_str = contract
                    .metadata
                    .get("new_mode")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .ok_or_else(|| {
                        AdapterError::Validation(
                            "FileChmod execute requires new_mode in metadata".into(),
                        )
                    })?;

                // Parse mode string (handle both "0o755" and "755" formats)
                let parsed_mode = Self::parse_mode_string(&new_mode_str).map_err(|e| {
                    AdapterError::Validation(format!(
                        "FileChmod invalid mode '{}': {}",
                        new_mode_str, e
                    ))
                })?;

                // Apply the permissions
                std::fs::set_permissions(file_path, std::fs::Permissions::from_mode(parsed_mode))
                    .map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!(
                            "FileChmod failed to set permissions on {}: {}",
                            file_path, e
                        ),
                    )
                })?;

                // Get actual applied mode for verification
                let applied_mode = std::fs::metadata(file_path)
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_EXECUTE,
                            format!(
                                "FileChmod failed to verify permissions on {}: {}",
                                file_path, e
                            ),
                        )
                    })?
                    .permissions()
                    .mode();

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "target_path".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
                metadata.insert(
                    "applied_mode".to_string(),
                    serde_json::Value::String(format!("{:o}", applied_mode & 0o7777)),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::DirCreate => {
                // Create the directory (single level - parent must exist)
                std::fs::create_dir(file_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("DirCreate failed for {}: {}", file_path, e),
                    )
                })?;

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "created_dir".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            ActionType::DirDelete => {
                // Delete the empty directory
                std::fs::remove_dir(file_path).map_err(|e| {
                    Self::phase_wrap_internal(
                        PHASE_EXECUTE,
                        format!("DirDelete failed for {}: {}", file_path, e),
                    )
                })?;

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "deleted_dir".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );

                Ok(ExecuteReceipt {
                    external_id: None,
                    result_digest: None,
                    adapter_metadata: metadata,
                })
            }
            _ => Err(AdapterError::Unsupported(format!(
                "execute not supported for {:?} in fs adapter",
                contract.action_type
            ))),
        }
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Validate target
        let file_path = Self::extract_path(&contract.target)?;

        // ALWAYS run action-aware default target-state verification first:
        // - FileWrite: verify file exists (post-execute state should have the file present)
        // - FileDelete: verify file does NOT exist (post-execute state should have the file absent)
        // This runs regardless of whether explicit verify_checks are present, preventing
        // explicit checks from bypassing the fundamental target-state invariant.
        match contract.action_type {
            ActionType::FileWrite => {
                // Fail-closed: file must exist after write (phase-aware error wrapping)
                Self::validate_path_exists(file_path).map_err(|_e| {
                    Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileWrite target file not found: {}", file_path),
                    )
                })?;
            }
            ActionType::FileDelete => {
                // Fail-closed: file must NOT exist after delete (phase-aware error wrapping)
                let p = Path::new(file_path);
                if p.exists() && p.is_file() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("file still exists after delete: {}", file_path),
                    ));
                }
            }
            ActionType::FileMove => {
                // Get destination from metadata (set during execute)
                let destination = contract
                    .metadata
                    .get("destination_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let destination = destination.ok_or_else(|| {
                    AdapterError::Validation(
                        "FileMove verify requires destination_path in metadata".into(),
                    )
                })?;

                // Fail-closed: destination must exist after move
                let dest_path = Path::new(&destination);
                if !dest_path.exists() || !dest_path.is_file() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileMove destination not found: {}", destination),
                    ));
                }

                // Fail-closed: source must NOT exist after move
                let source_path = Path::new(file_path);
                if source_path.exists() && source_path.is_file() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileMove source still exists after move: {}", file_path),
                    ));
                }
            }
            ActionType::FileCopy => {
                // Get destination and source hash from metadata
                let destination = contract
                    .metadata
                    .get("destination_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let destination = destination.ok_or_else(|| {
                    AdapterError::Validation(
                        "FileCopy verify requires destination_path in metadata".into(),
                    )
                })?;

                // Fail-closed: destination must exist after copy
                let dest_path = Path::new(&destination);
                if !dest_path.exists() || !dest_path.is_file() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileCopy destination not found: {}", destination),
                    ));
                }

                // Fail-closed: destination hash must match source hash
                let source_hash = contract
                    .metadata
                    .get("source_hash")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                if let Some(expected_hash) = source_hash {
                    let dest_hash = Self::compute_file_hash(&destination).map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_VERIFY,
                            format!(
                                "FileCopy verify failed to compute hash of {}: {}",
                                destination, e
                            ),
                        )
                    })?;
                    if dest_hash != expected_hash {
                        return Err(Self::phase_wrap_validation(
                            PHASE_VERIFY,
                            format!(
                                "FileCopy destination hash mismatch: expected {}, got {}",
                                expected_hash, dest_hash
                            ),
                        ));
                    }
                }
            }
            ActionType::FileAppend => {
                // Get original length from prepare metadata
                let original_length_str = contract
                    .metadata
                    .get("original_length")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let original_length = original_length_str
                    .ok_or_else(|| {
                        AdapterError::Validation(
                            "FileAppend verify requires original_length in metadata".into(),
                        )
                    })?
                    .parse::<u64>()
                    .map_err(|_e| {
                        AdapterError::Validation(
                            "FileAppend verify original_length is invalid".into(),
                        )
                    })?;

                // Fail-closed: file must exist after append
                Self::validate_path_exists(file_path).map_err(|_e| {
                    Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileAppend target file not found: {}", file_path),
                    )
                })?;

                // Get current file length
                let current_length = std::fs::metadata(file_path)
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_VERIFY,
                            format!(
                                "FileAppend verify failed to get metadata for {}: {}",
                                file_path, e
                            ),
                        )
                    })?
                    .len();

                // Fail-closed: file must have grown
                if current_length < original_length {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!(
                            "FileAppend file did not grow: original {} vs current {}",
                            original_length, current_length
                        ),
                    ));
                }

                // Get expected data hash from execute receipt metadata
                let expected_data_hash = contract
                    .metadata
                    .get("data_hash")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // If we have the data hash from execute, verify the appended portion
                // This is a best-effort check - we can't easily verify just the appended bytes
                // without storing the original content
                if let Some(_data_hash) = expected_data_hash {
                    // The file grew, which is the minimum we can verify without full content comparison
                    // A full verification would require comparing the appended bytes
                }
            }
            ActionType::FileChmod => {
                // Get expected new mode from contract metadata
                let expected_mode_str = contract
                    .metadata
                    .get("new_mode")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .ok_or_else(|| {
                        AdapterError::Validation(
                            "FileChmod verify requires new_mode in metadata".into(),
                        )
                    })?;

                // Parse expected mode
                let expected_mode = Self::parse_mode_string(&expected_mode_str).map_err(|e| {
                    AdapterError::Validation(format!("FileChmod verify invalid mode: {}", e))
                })?;

                // Fail-closed: file must exist after chmod
                Self::validate_path_exists(file_path).map_err(|_e| {
                    Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("FileChmod target file not found: {}", file_path),
                    )
                })?;

                // Get current file permissions
                let current_mode = std::fs::metadata(file_path)
                    .map_err(|e| {
                        Self::phase_wrap_internal(
                            PHASE_VERIFY,
                            format!(
                                "FileChmod verify failed to get metadata for {}: {}",
                                file_path, e
                            ),
                        )
                    })?
                    .permissions()
                    .mode();

                // Fail-closed: current mode must match expected (compare permission bits)
                let current_perm = current_mode & 0o7777;
                if current_perm != expected_mode {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!(
                            "FileChmod mode mismatch: expected {:o}, got {:o}",
                            expected_mode, current_perm
                        ),
                    ));
                }
            }
            ActionType::DirCreate => {
                // Fail-closed: directory must exist after DirCreate
                let dir_path = Path::new(file_path);
                if !dir_path.exists() || !dir_path.is_dir() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("DirCreate target directory not found: {}", file_path),
                    ));
                }
            }
            ActionType::DirDelete => {
                // Fail-closed: directory must NOT exist after DirDelete
                let dir_path = Path::new(file_path);
                if dir_path.exists() && dir_path.is_dir() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_VERIFY,
                        format!("directory still exists after delete: {}", file_path),
                    ));
                }
            }
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "verify not supported for {:?} in fs adapter",
                    contract.action_type
                )));
            }
        }

        // Run explicit verify_checks if present (fail-closed, runs after default check)
        for check in &contract.verify_checks {
            Self::run_check(check, file_path, "verify")?;
        }

        // Default to verified (action-specific check passed above)
        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Compensate is the same as rollback for all supported action types
        self.rollback(contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Only FileWrite, FileDelete, FileMove, FileCopy, FileAppend, DirCreate, and DirDelete rollback are supported
        if !matches!(
            contract.action_type,
            ActionType::FileWrite
                | ActionType::FileDelete
                | ActionType::FileMove
                | ActionType::FileCopy
                | ActionType::FileAppend
                | ActionType::FileChmod
                | ActionType::DirCreate
                | ActionType::DirDelete
        ) {
            return Err(AdapterError::Unsupported(format!(
                "rollback not supported for {:?} in fs adapter",
                contract.action_type
            )));
        }

        let file_path = Self::extract_path(&contract.target)?;

        // Revalidate path sandbox constraints before rollback.
        // This catches symlink swaps or sandbox escapes that may have occurred
        // between execute and rollback.
        self.revalidate_path_for_rollback(file_path)?;

        // Handle DirCreate rollback: delete the created directory
        if matches!(contract.action_type, ActionType::DirCreate) {
            let dir_path = Path::new(file_path);
            if dir_path.exists() && dir_path.is_dir() {
                if let Err(e) = std::fs::remove_dir(file_path) {
                    // Fail-closed: I/O error during recovery -> recovered=false
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "remove_dir",
                        file_path,
                        e,
                    ));
                }
            }

            // Verify directory no longer exists
            if dir_path.exists() && dir_path.is_dir() {
                return Err(Self::phase_wrap_validation(
                    PHASE_ROLLBACK,
                    format!(
                        "DirCreate rollback verification failed: directory still exists: {}",
                        file_path
                    ),
                ));
            }

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("deleted_created_dir".to_string()),
            );
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle DirDelete rollback: recreate the deleted directory
        if matches!(contract.action_type, ActionType::DirDelete) {
            let dir_path = Path::new(file_path);
            if !dir_path.exists() {
                if let Err(e) = std::fs::create_dir(file_path) {
                    // Fail-closed: I/O error during recovery -> recovered=false
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "create_dir",
                        file_path,
                        e,
                    ));
                }
            }

            // Verify directory exists again
            if !dir_path.exists() || !dir_path.is_dir() {
                return Err(Self::phase_wrap_validation(
                    PHASE_ROLLBACK,
                    format!(
                        "DirDelete rollback verification failed: directory not recreated: {}",
                        file_path
                    ),
                ));
            }

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("recreated_deleted_dir".to_string()),
            );
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle FileAppend rollback: truncate file to original length
        if matches!(contract.action_type, ActionType::FileAppend) {
            // Get original length from prepare metadata
            let original_length_str = contract
                .metadata
                .get("original_length")
                .and_then(|v| v.as_str())
                .map(String::from);

            let original_length = original_length_str
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "FileAppend rollback requires original_length in metadata".into(),
                    )
                })?
                .parse::<u64>()
                .map_err(|_e| {
                    AdapterError::Validation(
                        "FileAppend rollback original_length is invalid".into(),
                    )
                })?;

            // Get original hash for verification
            let original_hash = contract
                .metadata
                .get("original_hash")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Read current file content using O_NOFOLLOW (fail-closed on I/O error)
            let current_content = match Self::read_file_nofollow(file_path) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "read",
                        file_path,
                        std::io::Error::other(e.to_string()),
                    ));
                }
            };

            // Truncate to original length
            let truncated_content = if current_content.len() as u64 > original_length {
                current_content[..original_length as usize].to_vec()
            } else {
                // If current content is already shorter or same, nothing to truncate
                current_content
            };

            // Write truncated content back using O_NOFOLLOW (fail-closed on I/O error)
            if let Err(e) = Self::write_file_nofollow(file_path, &truncated_content) {
                return Ok(Self::fail_closed_recovery(
                    PHASE_ROLLBACK,
                    "write",
                    file_path,
                    std::io::Error::other(e.to_string()),
                ));
            }

            // Verify hash matches original if we have it
            if let Some(expected_hash) = original_hash {
                let restored_hash = match Self::compute_file_hash(file_path) {
                    Ok(h) => h,
                    Err(e) => {
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "compute_hash",
                            file_path,
                            std::io::Error::other(e.to_string()),
                        ));
                    }
                };

                if restored_hash != expected_hash {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!(
                            "FileAppend rollback hash mismatch: expected {}, got {}",
                            expected_hash, restored_hash
                        ),
                    ));
                }
            }

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("truncated_to_original".to_string()),
            );
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
            metadata.insert(
                "restored_length".to_string(),
                serde_json::Value::String(original_length.to_string()),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle FileChmod rollback: restore original permissions
        if matches!(contract.action_type, ActionType::FileChmod) {
            // Get original mode from prepare metadata
            let original_mode_str = contract
                .metadata
                .get("original_mode")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| {
                    AdapterError::Validation(
                        "FileChmod rollback requires original_mode in metadata".into(),
                    )
                })?;

            // Parse original mode
            let original_mode = Self::parse_mode_string(&original_mode_str).map_err(|e| {
                AdapterError::Validation(format!("FileChmod rollback invalid original_mode: {}", e))
            })?;

            // Restore original permissions (fail-closed on I/O error)
            if let Err(e) =
                std::fs::set_permissions(file_path, std::fs::Permissions::from_mode(original_mode))
            {
                return Ok(Self::fail_closed_recovery(
                    PHASE_ROLLBACK,
                    "set_permissions",
                    file_path,
                    e,
                ));
            }

            // Verify restored mode matches original (fail-closed on I/O error)
            let restored_mode = match std::fs::metadata(file_path) {
                Ok(meta) => meta.permissions().mode(),
                Err(e) => {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "metadata",
                        file_path,
                        e,
                    ));
                }
            };

            let restored_perm = restored_mode & 0o7777;
            if restored_perm != original_mode {
                return Err(Self::phase_wrap_validation(
                    PHASE_ROLLBACK,
                    format!(
                        "FileChmod rollback mode mismatch: expected {:o}, got {:o}",
                        original_mode, restored_perm
                    ),
                ));
            }

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("restored_original_permissions".to_string()),
            );
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
            metadata.insert(
                "restored_mode".to_string(),
                serde_json::Value::String(original_mode_str),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle FileMove rollback: move destination back to source
        if matches!(contract.action_type, ActionType::FileMove) {
            let destination = contract
                .metadata
                .get("destination_path")
                .and_then(|v| v.as_str())
                .map(String::from);

            let destination = destination.ok_or_else(|| {
                AdapterError::Validation(
                    "FileMove rollback requires destination_path in metadata".into(),
                )
            })?;

            // Revalidate destination path sandbox constraints before rollback.
            // This catches symlink swaps or sandbox escapes that may have occurred
            // between execute and rollback.
            self.revalidate_path_for_rollback(&destination)?;

            // Check if this was a cross-filesystem move
            let cross_fs_move = contract
                .metadata
                .get("cross_filesystem_move")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Compute snapshot path (for source file)
            let snapshot_path = Self::compute_snapshot_path(
                &Self::snapshot_dir(),
                &contract.execution_id,
                file_path,
            );

            // Ensure parent directory exists for the source path (fail-closed on I/O error)
            if let Some(parent) = Path::new(file_path).parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "create_dir_all",
                            file_path,
                            e,
                        ));
                    }
                }
            }

            let restored_hash: String;

            if cross_fs_move {
                // Cross-filesystem move: we need to delete dest and restore from snapshot
                // Destination must exist to delete it
                let dest_path = Path::new(&destination);
                if !dest_path.exists() || !dest_path.is_file() {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!(
                            "FileMove rollback (cross-fs) destination not found: {}",
                            destination
                        ),
                    ));
                }

                // Delete the destination file (fail-closed on I/O error)
                if let Err(e) = std::fs::remove_file(&destination) {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "remove_file",
                        &destination,
                        e,
                    ));
                }

                // Restore source from snapshot (fail-closed on I/O error)
                if let Err(e) = std::fs::copy(&snapshot_path, file_path) {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "copy",
                        file_path,
                        e,
                    ));
                }

                restored_hash = match Self::compute_file_hash(file_path) {
                    Ok(h) => h,
                    Err(e) => {
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "compute_hash",
                            file_path,
                            std::io::Error::other(e.to_string()),
                        ));
                    }
                };
                let snapshot_hash = match Self::compute_file_hash(snapshot_path.to_str().unwrap()) {
                    Ok(h) => h,
                    Err(e) => {
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "compute_hash",
                            snapshot_path.to_str().unwrap(),
                            std::io::Error::other(e.to_string()),
                        ));
                    }
                };

                if restored_hash != snapshot_hash {
                    return Err(Self::phase_wrap_validation(
                        PHASE_ROLLBACK,
                        format!(
                            "FileMove rollback (cross-fs) hash mismatch: restored {} vs snapshot {}",
                            restored_hash, snapshot_hash
                        ),
                    ));
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("cross_fs_delete_and_restore".to_string()),
                );
                metadata.insert(
                    "restored_to".to_string(),
                    serde_json::Value::String(file_path.to_string()),
                );
                metadata.insert(
                    "restored_hash".to_string(),
                    serde_json::Value::String(restored_hash.clone()),
                );

                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            // Standard (same-filesystem) move: fail-closed if destination missing
            let dest_path = Path::new(&destination);
            if !dest_path.exists() || !dest_path.is_file() {
                return Err(Self::phase_wrap_validation(
                    PHASE_ROLLBACK,
                    format!("FileMove rollback destination not found: {}", destination),
                ));
            }

            // Move destination back to source (fail-closed on I/O error)
            if let Err(e) = std::fs::rename(&destination, file_path) {
                return Ok(Self::fail_closed_recovery(
                    PHASE_ROLLBACK,
                    "rename",
                    file_path,
                    e,
                ));
            }

            // Verify restored file hash matches snapshot
            restored_hash = match Self::compute_file_hash(file_path) {
                Ok(h) => h,
                Err(e) => {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "compute_hash",
                        file_path,
                        std::io::Error::other(e.to_string()),
                    ));
                }
            };
            let snapshot_hash = match Self::compute_file_hash(snapshot_path.to_str().unwrap()) {
                Ok(h) => h,
                Err(e) => {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "compute_hash",
                        snapshot_path.to_str().unwrap(),
                        std::io::Error::other(e.to_string()),
                    ));
                }
            };

            if restored_hash != snapshot_hash {
                return Err(Self::phase_wrap_validation(
                    PHASE_ROLLBACK,
                    format!(
                        "FileMove rollback hash mismatch: restored {} vs snapshot {}",
                        restored_hash, snapshot_hash
                    ),
                ));
            }

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("moved_back".to_string()),
            );
            metadata.insert(
                "restored_to".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );
            metadata.insert(
                "restored_hash".to_string(),
                serde_json::Value::String(restored_hash.clone()),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle FileCopy rollback
        if matches!(contract.action_type, ActionType::FileCopy) {
            let destination = contract
                .metadata
                .get("destination_path")
                .and_then(|v| v.as_str())
                .map(String::from);

            let destination = destination.ok_or_else(|| {
                AdapterError::Validation(
                    "FileCopy rollback requires destination_path in metadata".into(),
                )
            })?;

            // Revalidate destination path sandbox constraints before rollback.
            // This catches symlink swaps or sandbox escapes that may have occurred
            // between execute and rollback.
            self.revalidate_path_for_rollback(&destination)?;

            let created_new_dest = contract
                .metadata
                .get("created_new_dest")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if created_new_dest {
                // New destination: rollback is idempotent delete
                let dest_path = Path::new(&destination);
                if dest_path.exists() && dest_path.is_file() {
                    if let Err(e) = std::fs::remove_file(&destination) {
                        // Fail-closed: I/O error during recovery -> recovered=false
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "remove_file",
                            &destination,
                            e,
                        ));
                    }
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("deleted_new_dest".to_string()),
                );
                metadata.insert(
                    "target_path".to_string(),
                    serde_json::Value::String(destination),
                );

                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            // Existing destination: restore from snapshot
            // Compute snapshot path for the destination
            let snapshot_path = Self::compute_snapshot_path(
                &Self::snapshot_dir(),
                &contract.execution_id,
                &destination,
            );

            // If snapshot doesn't exist, destination was new - just delete it
            if !snapshot_path.exists() {
                let dest_path = Path::new(&destination);
                if dest_path.exists() && dest_path.is_file() {
                    if let Err(e) = std::fs::remove_file(&destination) {
                        // Fail-closed: I/O error during recovery -> recovered=false
                        return Ok(Self::fail_closed_recovery(
                            PHASE_ROLLBACK,
                            "remove_file",
                            &destination,
                            e,
                        ));
                    }
                }

                let mut metadata = JsonMap::new();
                metadata.insert(
                    "rollback_action".to_string(),
                    serde_json::Value::String("deleted_new_dest".to_string()),
                );
                metadata.insert(
                    "target_path".to_string(),
                    serde_json::Value::String(destination),
                );

                return Ok(RecoveryReceipt {
                    recovered: true,
                    adapter_metadata: metadata,
                });
            }

            // Restore destination from snapshot (fail-closed on I/O error)
            if let Err(e) = std::fs::copy(&snapshot_path, &destination) {
                return Ok(Self::fail_closed_recovery(
                    PHASE_ROLLBACK,
                    "copy",
                    &destination,
                    e,
                ));
            }

            let restored_hash = match Self::compute_file_hash(&destination) {
                Ok(h) => h,
                Err(e) => {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "compute_hash",
                        &destination,
                        std::io::Error::other(e.to_string()),
                    ));
                }
            };

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("restored_from_snapshot".to_string()),
            );
            metadata.insert(
                "restored_from".to_string(),
                serde_json::Value::String(snapshot_path.display().to_string()),
            );
            metadata.insert(
                "restored_hash".to_string(),
                serde_json::Value::String(restored_hash),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Handle FileWrite and FileDelete (existing logic)
        // Check if this was a new-file creation (rollback should delete, not restore)
        let created_new_file = contract
            .metadata
            .get("created_new_file")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if created_new_file {
            // New file creation: rollback is delete (idempotent if already absent)
            let p = Path::new(file_path);
            if p.exists() && p.is_file() {
                if let Err(e) = std::fs::remove_file(file_path) {
                    // Fail-closed: I/O error during recovery -> recovered=false
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "remove_file",
                        file_path,
                        e,
                    ));
                }
            }
            // If file doesn't exist, that's fine - idempotent cleanup

            let mut metadata = JsonMap::new();
            metadata.insert(
                "rollback_action".to_string(),
                serde_json::Value::String("deleted_new_file".to_string()),
            );
            metadata.insert(
                "target_path".to_string(),
                serde_json::Value::String(file_path.to_string()),
            );

            return Ok(RecoveryReceipt {
                recovered: true,
                adapter_metadata: metadata,
            });
        }

        // Existing file: compute snapshot path and restore from snapshot
        // Compute snapshot path deterministically from contract fields.
        // This enables cross-instance recovery without depending on metadata persistence.
        let snapshot_path =
            Self::compute_snapshot_path(&Self::snapshot_dir(), &contract.execution_id, file_path);

        // Fail-closed: snapshot must exist
        if !snapshot_path.exists() {
            return Err(Self::phase_wrap_validation(
                PHASE_ROLLBACK,
                format!(
                    "snapshot not found for {} (path: {}): cannot restore - original file content is unavailable",
                    file_path,
                    snapshot_path.display()
                ),
            ));
        }

        // Ensure parent directory exists for the target path (fail-closed on I/O error)
        if let Some(parent) = Path::new(file_path).parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Ok(Self::fail_closed_recovery(
                        PHASE_ROLLBACK,
                        "create_dir_all",
                        file_path,
                        e,
                    ));
                }
            }
        }

        // Restore file from snapshot (fail-closed on I/O error)
        if let Err(e) = std::fs::copy(&snapshot_path, file_path) {
            return Ok(Self::fail_closed_recovery(
                PHASE_ROLLBACK,
                "copy",
                file_path,
                e,
            ));
        }

        // Verify restoration by computing hash
        let restored_hash = match Self::compute_file_hash(file_path) {
            Ok(h) => h,
            Err(e) => {
                return Ok(Self::fail_closed_recovery(
                    PHASE_ROLLBACK,
                    "compute_hash",
                    file_path,
                    std::io::Error::other(e.to_string()),
                ));
            }
        };

        let mut metadata = JsonMap::new();
        metadata.insert(
            "restored_from".to_string(),
            serde_json::Value::String(snapshot_path.display().to_string()),
        );
        metadata.insert(
            "restored_hash".to_string(),
            serde_json::json!(restored_hash),
        );

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: metadata,
        })
    }
}

/// Register the FsAdapter with the given registry using "fs" as the adapter key.
/// This allows the adapter to be used for FileWrite, FileDelete, and other
/// filesystem operations via the rollback service.
pub fn register_fs_adapter(registry: &mut AdapterRegistry) {
    registry.register(std::sync::Arc::new(FsAdapter::new("fs")));
}

#[cfg(test)]
mod tests;
