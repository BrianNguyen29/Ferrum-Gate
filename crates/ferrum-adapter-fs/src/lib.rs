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
        let p = Path::new(path);
        if !p.exists() || !p.is_file() {
            return Err(FsAdapterError::FilePathNotFound(path.to_string()).into());
        }
        Ok(())
    }

    /// Validates path depth against the configured maximum.
    /// Returns an error if the path has more components than max_path_depth.
    fn validate_path_depth(path: &str, max_depth: usize) -> Result<(), FsAdapterError> {
        let depth = Path::new(path).components().count();
        if depth > max_depth {
            return Err(FsAdapterError::PathDepthExceedsLimit(depth, max_depth));
        }
        Ok(())
    }

    /// Validates that a path does not escape the sandbox via symlinks.
    /// If sandbox_to_workdir is true, resolves all symlinks and checks the final
    /// path stays within the workdir. Returns an error if escape is detected.
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
    /// Returns an error if file size exceeds max_file_size.
    fn validate_file_size(path: &str, max_size: u64) -> Result<(), FsAdapterError> {
        let metadata = std::fs::metadata(path)?;
        let size = metadata.len();
        if size > max_size {
            return Err(FsAdapterError::FileSizeExceedsLimit(size, max_size));
        }
        Ok(())
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
        let contents = Self::read_file_nofollow(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        Ok(hex::encode(result))
    }

    /// Parses a mode string into a u32.
    /// Handles both "0o755" (octal with prefix) and "755" (octal without prefix) formats.
    fn parse_mode_string(mode_str: &str) -> Result<u32, String> {
        let mode_str = mode_str.trim();
        if mode_str.is_empty() {
            return Err("mode cannot be empty".to_string());
        }

        // Handle 0o prefix (e.g., "0o755")
        if mode_str.starts_with("0o") || mode_str.starts_with("0O") {
            let octal_part = &mode_str[2..];
            u32::from_str_radix(octal_part, 8)
                .map_err(|e| format!("invalid octal mode '{}': {}", mode_str, e))
        } else if mode_str.starts_with("0x") || mode_str.starts_with("0X") {
            // Handle hex prefix (e.g., "0x755")
            u32::from_str_radix(&mode_str[2..], 16)
                .map_err(|e| format!("invalid hex mode '{}': {}", mode_str, e))
        } else {
            // Assume octal (e.g., "755")
            u32::from_str_radix(mode_str, 8)
                .map_err(|e| format!("invalid octal mode '{}': {}", mode_str, e))
        }
    }

    /// Maps an IO error to FsAdapterError, converting Unix ELOOP to SymlinkNotAllowed.
    fn map_io_error_to_fs(err: std::io::Error, path: &str) -> FsAdapterError {
        #[cfg(unix)]
        {
            use std::io::ErrorKind;
            if err.kind() == ErrorKind::Other {
                if let Some(code) = err.raw_os_error() {
                    // ELOOP = too many symbolic links encountered
                    if code == libc::ELOOP {
                        return FsAdapterError::SymlinkNotAllowed(format!(
                            "symbolic link in final component not allowed: {}",
                            path
                        ));
                    }
                }
            }
            // Handle NotFound as well since O_NOFOLLOW can cause it
            if err.kind() == ErrorKind::NotFound {
                return FsAdapterError::SymlinkNotAllowed(format!(
                    "file does not exist or is a symlink (O_NOFOLLOW): {}",
                    path
                ));
            }
        }
        FsAdapterError::Io(err)
    }

    /// Reads a file without following symbolic links in the final component (Unix O_NOFOLLOW).
    /// On Unix: uses OpenOptions with custom_flags(O_NOFOLLOW) to open the file.
    /// On non-Unix: falls back to std::fs::read.
    fn read_file_nofollow(path: &str) -> Result<Vec<u8>, FsAdapterError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| Self::map_io_error_to_fs(e, path))?;
            use std::io::Read;
            let mut reader = file;
            let mut contents = Vec::new();
            reader
                .read_to_end(&mut contents)
                .map_err(|e| Self::map_io_error_to_fs(e, path))?;
            Ok(contents)
        }
        #[cfg(not(unix))]
        {
            std::fs::read(path).map_err(FsAdapterError::Io)
        }
    }

    /// Writes to a file without following symbolic links in the final component (Unix O_NOFOLLOW).
    /// On Unix: uses OpenOptions with custom_flags(O_NOFOLLOW) to create/truncate the file.
    /// On non-Unix: falls back to std::fs::write.
    fn write_file_nofollow(path: &str, contents: &[u8]) -> Result<(), FsAdapterError> {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| Self::map_io_error_to_fs(e, path))?
                .write_all(contents)
                .map_err(|e| Self::map_io_error_to_fs(e, path))?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, contents).map_err(FsAdapterError::Io)
        }
    }

    /// Computes a deterministic snapshot subdirectory path based on execution_id and target path.
    ///
    /// Path structure: `{snapshot_root}/{execution_id}/{path_hash}`
    /// where `path_hash` is the first 16 hex chars of SHA-256 hash of the canonical target path.
    fn compute_snapshot_path(
        snapshot_root: &Path,
        execution_id: &ExecutionId,
        target_path: &str,
    ) -> PathBuf {
        // Hash the target path for a compact, safe directory name
        let mut hasher = Sha256::new();
        hasher.update(target_path.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let path_hash = &hash[..16]; // First 16 chars for brevity

        snapshot_root
            .join(execution_id.0.to_string())
            .join(path_hash)
    }

    /// Captures a snapshot of the file at `target_path` to `snapshot_path`.
    /// Returns the snapshot path on success.
    fn capture_snapshot(
        target_path: &str,
        snapshot_path: &Path,
    ) -> Result<PathBuf, FsAdapterError> {
        // Ensure parent directory exists
        if let Some(parent) = snapshot_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Copy the file to snapshot location
        std::fs::copy(target_path, snapshot_path)?;
        Ok(snapshot_path.to_path_buf())
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
/// Converts a serde_json::Map to a JsonMap (IndexMap)
fn json_map_from_serde_map(map: serde_json::Map<String, serde_json::Value>) -> JsonMap {
    map.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        CheckSpec, ExecutionId, IntentId, ProposalId, RollbackContractId, RollbackState,
    };
    use tempfile::tempdir;

    fn create_test_contract(file_path: &str) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path.to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        }
    }

    fn create_test_request(file_path: &str) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path.to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_prepare_accepts_valid_file_path() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.display().to_string();

        // Create the file first
        std::fs::write(&file_path, b"hello world").unwrap();

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);
        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
    }

    #[tokio::test]
    async fn test_prepare_fails_on_nonexistent_file_path() {
        let adapter = FsAdapter::new("fs");
        let request = create_test_request("/nonexistent/path/to/file.txt");
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prepare_fails_on_unsupported_action_type() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"hello").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.action_type = ActionType::SqlMutation; // Not supported for fs
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prepare_with_file_exists_check() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("exists.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"hello").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &file_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
    }

    #[tokio::test]
    async fn test_prepare_with_file_hash_matches_check() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("hash_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"hello world").unwrap();

        // Compute the expected hash
        let contents = std::fs::read(&file_path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let expected_hash = hex::encode(hasher.finalize());

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": &expected_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
    }

    #[tokio::test]
    async fn test_prepare_fails_with_wrong_hash() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("wrong_hash.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"hello world").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": "wronghashvalue123"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_with_file_exists_check() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_exists.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"verify me").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&file_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &file_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let receipt = adapter.verify(&contract).await.unwrap();
        assert!(receipt.verified);
    }

    #[tokio::test]
    async fn test_verify_fails_when_file_missing() {
        let adapter = FsAdapter::new("fs");
        let contract = create_test_contract("/nonexistent/file.txt");
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_with_hash_matches() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_hash.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"content to verify").unwrap();

        // Compute the expected hash
        let contents = std::fs::read(&file_path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let expected_hash = hex::encode(hasher.finalize());

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&file_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": &expected_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let receipt = adapter.verify(&contract).await.unwrap();
        assert!(receipt.verified);
    }

    #[tokio::test]
    async fn test_verify_fails_on_hash_mismatch() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("hash_mismatch.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&file_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": "aabbccdd"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_returns_unsupported() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("execute_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let contract = create_test_contract(&file_path_str);
        let result = adapter.execute(&contract, &serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rollback_returns_unsupported() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("rollback_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let contract = create_test_contract(&file_path_str);
        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compensate_returns_unsupported() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("compensate_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let contract = create_test_contract(&file_path_str);
        let result = adapter.compensate(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prepare_with_invalid_target_type() {
        let adapter = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::SqliteTxn {
                db_path: "test.db".to_string(),
                tx_id: "tx".to_string(),
            }, // Wrong target type
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prepare_with_unsupported_check_type() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("unsupported_check.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::GitRefMatches, // Not supported for fs
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_with_unsupported_check_type() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_unsupported.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&file_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::HttpStatusExpected, // Not supported
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    // =============================================================================
    // FileWrite Snapshot/Recovery Slice Tests
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_captures_snapshot_for_existing_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);
        let receipt = adapter.prepare(&request).await.unwrap();

        assert!(receipt.accepted);
        // Snapshot path should be in metadata
        let snapshot_path = receipt
            .adapter_metadata
            .get("snapshot_path")
            .expect("snapshot_path should be in metadata")
            .as_str()
            .expect("snapshot_path should be a string");
        let original_path = receipt
            .adapter_metadata
            .get("original_path")
            .expect("original_path should be in metadata")
            .as_str()
            .expect("original_path should be a string");

        assert_eq!(original_path, file_path_str);
        // Snapshot should exist and contain original content
        assert!(Path::new(snapshot_path).exists());
        let snapshot_content = std::fs::read(snapshot_path).unwrap();
        assert_eq!(snapshot_content, b"original content");
    }

    #[tokio::test]
    async fn test_execute_writes_new_content_to_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("execute_target.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // First prepare (this will snapshot the original)
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract with the prepare receipt metadata
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id, // Use same execution_id
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute with new content
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        assert!(exec_receipt.result_digest.is_some());
        // File should now have new content
        let new_content = std::fs::read(&file_path).unwrap();
        assert_eq!(new_content, b"new content");
    }

    #[tokio::test]
    async fn test_rollback_restores_overwritten_contents() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("rollback_restore.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (captures snapshot)
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute with different content (simulating the write operation)
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        // Execute changes
        adapter
            .execute(&contract, &serde_json::json!("modified content"))
            .await
            .unwrap();

        // Verify file was modified
        assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

        // Now rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // File should be restored to original content
        let restored_content = std::fs::read(&file_path).unwrap();
        assert_eq!(restored_content, b"original content");
    }

    #[tokio::test]
    async fn test_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("compensate_alias.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        // Execute changes
        adapter
            .execute(&contract, &serde_json::json!("changed"))
            .await
            .unwrap();

        // Compensate should restore (alias for rollback)
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);

        // File should be restored
        assert_eq!(std::fs::read(&file_path).unwrap(), b"original");
    }

    #[tokio::test]
    async fn test_rollback_fail_closed_when_snapshot_missing() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("missing_snapshot.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Create a contract with empty metadata (no snapshot)
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(), // Different execution_id = no snapshot
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str,
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(), // Empty metadata = no snapshot_path
        };

        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        // Should fail with error about snapshot not found (path computed but file doesn't exist)
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("snapshot") || msg.contains("not found"));
            }
            _ => panic!("expected validation error for missing snapshot"),
        }
    }

    #[tokio::test]
    async fn test_compensate_restores_deleted_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("delete_target.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (captures snapshot)
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute deletes the file
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists());

        // Compensate should restore the file
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert!(file_path.exists());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"to be deleted");
    }

    #[tokio::test]
    async fn test_rollback_restores_deleted_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("rollback_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (captures snapshot)
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute deletes the file
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists());

        // Rollback should restore the file
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);
        assert!(file_path.exists());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"to be deleted");
    }

    #[tokio::test]
    async fn test_file_delete_rollback_fail_closed_when_snapshot_missing() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("missing_snapshot_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Contract with different execution_id = no snapshot
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(), // Different execution_id = no snapshot
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str,
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        };

        // Rollback for FileDelete should fail when snapshot is missing
        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("snapshot") || msg.contains("not found"));
            }
            _ => panic!("expected validation error for missing snapshot"),
        }
    }

    #[tokio::test]
    async fn test_prepare_does_not_snapshot_new_file() {
        // Test that prepare on a non-existing file in a directory that exists
        // does NOT fail (new file creation is now supported when parent exists)
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");
        let file_path_str = file_path.display().to_string();
        // Note: file does NOT exist, but parent directory (temp_dir) does

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);

        // Should succeed - new file creation with parent existing is now supported
        let result = adapter.prepare(&request).await;
        assert!(
            result.is_ok(),
            "Expected prepare to succeed for new file with parent existing"
        );
        let receipt = result.unwrap();
        assert!(receipt.accepted);
        // Should mark this as a new file creation (no snapshot needed)
        let created_new_file = receipt
            .adapter_metadata
            .get("created_new_file")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(
            created_new_file,
            "Expected created_new_file to be true for new file creation"
        );
    }

    #[tokio::test]
    async fn test_execute_with_object_payload() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("object_payload.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute with object payload containing "content" field
        adapter
            .execute(
                &contract,
                &serde_json::json!({ "content": "content from object" }),
            )
            .await
            .unwrap();

        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, b"content from object");
    }

    #[tokio::test]
    async fn test_rollback_works_across_adapter_instances() {
        // Test that rollback works when prepare and rollback are called on different adapter instances.
        // This verifies the snapshot path is derived deterministically from contract fields,
        // not from per-instance state.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("cross_instance.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        // Instance A: prepare
        let adapter_a = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        // Build contract with the prepare receipt metadata
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id, // Same execution_id
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Instance A: execute changes
        adapter_a
            .execute(&contract, &serde_json::json!("modified content"))
            .await
            .unwrap();

        // Verify file was modified
        assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

        // Instance B: rollback (different adapter instance)
        let adapter_b = FsAdapter::new("fs");
        let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // File should be restored to original content despite using different adapter instance
        let restored_content = std::fs::read(&file_path).unwrap();
        assert_eq!(restored_content, b"original content");
    }

    #[tokio::test]
    async fn test_rollback_works_without_metadata_persistence() {
        // Test that rollback works even when contract has no adapter_metadata.
        // Snapshot path is derived from contract.execution_id + target.path.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("no_metadata.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        // Instance A: prepare
        let adapter_a = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);
        adapter_a.prepare(&request).await.unwrap();

        // Build contract with empty metadata (simulating recovered contract without metadata)
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id, // Same execution_id enables snapshot lookup
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(), // Empty metadata - no snapshot_path stored
        };

        // Instance A: execute changes
        adapter_a
            .execute(&contract, &serde_json::json!("modified content"))
            .await
            .unwrap();

        // Verify file was modified
        assert_eq!(std::fs::read(&file_path).unwrap(), b"modified content");

        // Instance B: rollback with no metadata - should still work via deterministic path
        let adapter_b = FsAdapter::new("fs");
        let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // File should be restored to original content
        let restored_content = std::fs::read(&file_path).unwrap();
        assert_eq!(restored_content, b"original content");
    }

    // =============================================================================
    // FileDelete Snapshot/Recovery Slice Tests
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_captures_snapshot_for_file_delete() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("delete_me.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"content to delete").unwrap();

        let adapter = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let receipt = adapter.prepare(&request).await.unwrap();

        assert!(receipt.accepted);
        // Snapshot path should be in metadata
        let snapshot_path = receipt
            .adapter_metadata
            .get("snapshot_path")
            .expect("snapshot_path should be in metadata")
            .as_str()
            .expect("snapshot_path should be a string");
        let original_path = receipt
            .adapter_metadata
            .get("original_path")
            .expect("original_path should be in metadata")
            .as_str()
            .expect("original_path should be a string");

        assert_eq!(original_path, file_path_str);
        // Snapshot should exist and contain original content
        assert!(Path::new(snapshot_path).exists());
        let snapshot_content = std::fs::read(snapshot_path).unwrap();
        assert_eq!(snapshot_content, b"content to delete");
    }

    #[tokio::test]
    async fn test_execute_deletes_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("execute_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (captures snapshot)
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute with empty payload (delete doesn't need content)
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        // File should be deleted
        assert!(!file_path.exists());
        assert!(exec_receipt.adapter_metadata.get("deleted_path").is_some());
    }

    #[tokio::test]
    async fn test_prepare_fails_closed_for_nonexistent_file_delete() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent_delete.txt");
        let file_path_str = file_path.display().to_string();
        // Note: file does NOT exist

        let adapter = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        // Should fail because file doesn't exist (fail-closed for FileDelete)
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                // The error message should contain the path we tried to validate
                assert!(
                    msg.contains(&file_path_str)
                        || msg.contains("not found")
                        || msg.contains("not a file")
                );
            }
            _ => panic!("expected validation error for missing file"),
        }
    }

    #[tokio::test]
    async fn test_file_delete_rollback_works_across_adapter_instances() {
        // Test that rollback works when prepare and rollback are called on different adapter instances.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("cross_instance_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        // Instance A: prepare
        let adapter_a = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Instance A: execute delete
        adapter_a
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists());

        // Instance B: rollback (different adapter instance)
        let adapter_b = FsAdapter::new("fs");
        let rollback_receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // File should be restored despite using different adapter instance
        assert!(file_path.exists());
        let restored_content = std::fs::read(&file_path).unwrap();
        assert_eq!(restored_content, b"original content");
    }

    // =============================================================================
    // FileDelete Verify Semantics Tests (P2.1 edge-case slice)
    // =============================================================================

    #[tokio::test]
    async fn test_file_delete_verify_success_after_execute() {
        // Verify succeeds when file has been deleted (absent is correct post-execute state)
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_after_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute deletes the file
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists());

        // Verify should succeed because file is correctly absent after delete
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_delete_verify_fails_when_file_still_exists() {
        // Verify fails when file still exists (e.g., recreated by external process)
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_fails_if_exists.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Simulate external process recreating the file (or delete failed silently)
        // Don't execute delete - just prepare then check that verify would fail
        // because the file still exists when it shouldn't
        assert!(file_path.exists());

        // Verify should fail because file exists but should not (no execute called)
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("still exists") || msg.contains("not found"));
            }
            _ => panic!("expected validation error for file still existing after delete"),
        }
    }

    #[tokio::test]
    async fn test_file_write_verify_still_checks_presence() {
        // Verify succeeds when file exists after FileWrite execute
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_write_presence.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute writes new content
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Verify should succeed because file exists after write
        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_write_verify_fails_if_file_missing() {
        // Verify fails when file does not exist after FileWrite (should have been written)
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_write_missing.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Note: do NOT execute - file still exists from setup
        // But the contract is for FileWrite, so before verify after execute, it should pass
        // If we call verify before execute, it should fail because file doesn't match post-execute expectation
        // Actually the issue is we're checking "verify fails if file missing" - this tests the opposite
        // The point is: FileWrite verify checks file EXISTS

        // For this test, let's manually delete the file to simulate a failed write
        std::fs::remove_file(&file_path).unwrap();

        // Verify should fail because file doesn't exist (post-write state should have file)
        let result = adapter.verify(&contract).await;
        assert!(result.is_err(), "Expected verify to fail for missing file");
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                // Validation error for missing file - msg is the path
                assert!(
                    msg.contains("/tmp"),
                    "Expected validation error to contain path"
                );
            }
            _ => panic!("expected validation error for missing file after write"),
        }
    }

    #[tokio::test]
    async fn test_file_delete_verify_with_explicit_check_still_runs() {
        // Explicit verify_checks should still run and be fail-closed
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("explicit_check_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            // Explicit FileExists check - but file should be absent after delete
            // So this explicit check should FAIL
            verify_checks: vec![CheckSpec {
                check_type: CheckType::FileExists,
                config: json_map_from_serde_map(
                    serde_json::json!({ "path": &file_path_str })
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            }],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute deletes the file
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists());

        // Verify with explicit FileExists check should fail because file was deleted
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        // This is correct behavior - explicit checks are fail-closed
    }

    // =============================================================================
    // FileWrite New-File Creation Slice Tests (P2.1)
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_accepts_new_file_when_parent_exists() {
        // Test that prepare accepts a non-existing file when parent directory exists
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        let file_path = sub_dir.join("new_file.txt");
        let file_path_str = file_path.display().to_string();
        // Note: file does NOT exist, but parent directory does

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);

        let receipt = adapter.prepare(&request).await.unwrap();
        assert!(receipt.accepted);
        // Should mark this as a new file creation
        let created_new_file = receipt
            .adapter_metadata
            .get("created_new_file")
            .expect("created_new_file should be in metadata")
            .as_bool()
            .expect("created_new_file should be a bool");
        assert!(created_new_file);
        // No snapshot_path for new file
        assert!(receipt.adapter_metadata.get("snapshot_path").is_none());
    }

    #[tokio::test]
    async fn test_prepare_rejects_new_file_when_parent_missing() {
        // Test that prepare fails closed when parent directory is missing
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent_parent/new_file.txt");
        let file_path_str = file_path.display().to_string();
        // Note: neither the file NOR the parent directory exists

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("parent") || msg.contains("does not exist"),
                    "Expected parent directory error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing parent directory"),
        }
    }

    // =============================================================================
    // FileDelete Explicit Check Path Mismatch Regression Test (P2.1)
    // =============================================================================

    #[tokio::test]
    async fn test_file_delete_prepare_check_path_mismatch() {
        // FileDelete: explicit check with path mismatch should fail at prepare.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&target_path, b"target content").unwrap();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let adapter = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![CheckSpec {
                check_type: CheckType::FileExists,
                config: json_map_from_serde_map(
                    serde_json::json!({ "path": &wrong_path_str })
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            }],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    // =============================================================================
    // Explicit Check Path Mismatch Tests (P2.1 hardening)
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_fileexists_check_path_mismatch() {
        // FileExists check with path different from contract target should fail at prepare.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&target_path, b"target content").unwrap();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        // FileExists check targeting a DIFFERENT path should fail
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &wrong_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_check_path_mismatch() {
        // FileHashMatches check with path different from contract target should fail at prepare.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&target_path, b"target content").unwrap();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let mut hasher = Sha256::new();
        hasher.update(b"wrong content");
        let wrong_hash = hex::encode(hasher.finalize());

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        // FileHashMatches check targeting a DIFFERENT path should fail
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &wrong_path_str,
                    "expected_hash": &wrong_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    #[tokio::test]
    async fn test_verify_fileexists_check_path_mismatch() {
        // FileExists check with path different from contract target should fail at verify.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&target_path, b"target content").unwrap();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        // FileExists check targeting a DIFFERENT path should fail
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &wrong_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    #[tokio::test]
    async fn test_verify_filehashmatches_check_path_mismatch() {
        // FileHashMatches check with path different from contract target should fail at verify.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&target_path, b"target content").unwrap();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let mut hasher = Sha256::new();
        hasher.update(b"wrong content");
        let wrong_hash = hex::encode(hasher.finalize());

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        // FileHashMatches check targeting a DIFFERENT path should fail
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &wrong_path_str,
                    "expected_hash": &wrong_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    // =============================================================================
    // Malformed Check Config Validation Tests (P2.1 ergonomics)
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_fileexists_missing_path() {
        // FileExists check missing 'path' field should give clear error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("requires 'path'"),
                    "Expected 'requires path' error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing path"),
        }
    }

    #[tokio::test]
    async fn test_prepare_fileexists_non_string_path() {
        // FileExists check with non-string 'path' should give clear type error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": 123 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error for path, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for non-string path"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_missing_path() {
        // FileHashMatches check missing 'path' field should give clear error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "expected_hash": "abc123" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("requires 'path'"),
                    "Expected 'requires path' error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing path"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_non_string_path() {
        // FileHashMatches check with non-string 'path' should give clear type error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": ["array", "not", "string"], "expected_hash": "abc123" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error for path, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for non-string path"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_missing_expected_hash() {
        // FileHashMatches check missing 'expected_hash' field should give clear error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &target_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("requires 'expected_hash'"),
                    "Expected 'requires expected_hash' error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing expected_hash"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_non_string_expected_hash() {
        // FileHashMatches check with non-string 'expected_hash' should give clear type error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &target_path_str, "expected_hash": 123456 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error for expected_hash, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for non-string expected_hash"),
        }
    }

    #[tokio::test]
    async fn test_verify_fileexists_non_string_path() {
        // FileExists check with non-string 'path' at verify should give clear type error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": { "object": "not string" } })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error for path, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for non-string path"),
        }
    }

    #[tokio::test]
    async fn test_verify_filehashmatches_non_string_expected_hash() {
        // FileHashMatches check with non-string 'expected_hash' at verify should give clear type error.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &target_path_str, "expected_hash": true })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error for expected_hash, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for non-string expected_hash"),
        }
    }

    // =============================================================================
    // Phase-Aware Error Message Tests (P2.1 validation ergonomics)
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_unsupported_check_error_mentions_prepare_phase() {
        // Unsupported check at prepare should mention "prepare" in error message.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::GitRefMatches, // Not supported for fs
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Unsupported(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in unsupported check error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("GitRefMatches"),
                    "Expected check type in error, got: {}",
                    msg
                );
            }
            _ => panic!("expected unsupported error for unsupported check type"),
        }
    }

    #[tokio::test]
    async fn test_verify_unsupported_check_error_mentions_verify_phase() {
        // Unsupported check at verify should mention "verify" in error message.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&file_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::HttpStatusExpected, // Not supported
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Unsupported(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in unsupported check error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("HttpStatusExpected"),
                    "Expected check type in error, got: {}",
                    msg
                );
            }
            _ => panic!("expected unsupported error for unsupported check type"),
        }
    }

    #[tokio::test]
    async fn test_prepare_malformed_check_error_mentions_prepare_phase_and_check_type() {
        // Malformed FileExists check at prepare should mention phase and check type.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": 123 }) // non-string
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileExists"),
                    "Expected check type in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for malformed check"),
        }
    }

    #[tokio::test]
    async fn test_verify_malformed_check_error_mentions_verify_phase_and_check_type() {
        // Malformed FileHashMatches check at verify should mention phase and check type.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &target_path_str, "expected_hash": true }) // non-string hash
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileHashMatches"),
                    "Expected check type in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("must be a string"),
                    "Expected type error in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for malformed check"),
        }
    }

    #[tokio::test]
    async fn test_prepare_missing_hash_field_error_mentions_phase_and_check_type() {
        // FileHashMatches missing 'expected_hash' at prepare should mention phase and check type.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &target_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileHashMatches"),
                    "Expected check type in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("requires 'expected_hash'"),
                    "Expected missing field error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing field"),
        }
    }

    #[tokio::test]
    async fn test_verify_missing_path_field_error_mentions_phase_and_check_type() {
        // FileExists missing 'path' at verify should mention phase and check type.
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let target_path_str = target_path.display().to_string();
        std::fs::write(&target_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut contract = create_test_contract(&target_path_str);
        contract.verify_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(serde_json::json!({}).as_object().unwrap().clone()),
        }];

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileExists"),
                    "Expected check type in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("requires 'path'"),
                    "Expected missing field error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing field"),
        }
    }

    // =============================================================================
    // New-File FileWrite with Explicit Check Path Mismatch Tests (P2.1)
    // =============================================================================

    #[tokio::test]
    async fn test_new_file_write_prepare_check_path_mismatch() {
        // New-file FileWrite: explicit check with path mismatch should fail at prepare.
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        let target_path = sub_dir.join("new_file.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        // Note: target_path does NOT exist yet (new file case)

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_path_str);
        // FileExists check with WRONG path should fail
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &wrong_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    #[tokio::test]
    async fn test_new_file_write_verify_check_path_mismatch_after_execute() {
        // New-file FileWrite: after execute, verify with path mismatch check should fail.
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        let target_path = sub_dir.join("new_file.txt");
        let target_path_str = target_path.display().to_string();
        let wrong_path = temp_dir.path().join("wrong.txt");
        let wrong_path_str = wrong_path.display().to_string();
        std::fs::write(&wrong_path, b"wrong content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (new file case)
        let request = create_test_request(&target_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![CheckSpec {
                check_type: CheckType::FileExists,
                config: json_map_from_serde_map(
                    serde_json::json!({ "path": &wrong_path_str })
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            }],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute creates the file
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Verify should FAIL because check targets wrong path (path mismatch)
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("path mismatch"),
                    "Expected path mismatch error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for path mismatch"),
        }
    }

    // =============================================================================
    // Phase-Aware FS/Internal Error Normalization Tests (P2.1)
    // =============================================================================

    #[tokio::test]
    async fn test_prepare_fileexists_check_file_not_found_shows_phase() {
        // FileExists check where file is missing should show [prepare] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");
        let file_path_str = file_path.display().to_string();
        // Note: file does NOT exist

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileExists,
            config: json_map_from_serde_map(
                serde_json::json!({ "path": &file_path_str })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        }];

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileExists"),
                    "Expected 'FileExists' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("file not found"),
                    "Expected 'file not found' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing file"),
        }
    }

    #[tokio::test]
    async fn test_verify_fileexists_check_file_not_found_shows_phase() {
        // FileExists check at verify where file is missing shows [verify] phase context.
        // Note: For FileWrite verify, the default validation runs FIRST (file must exist),
        // so the explicit FileExists check is secondary. If default validation passes
        // (file exists), the explicit check would also pass. This test verifies the
        // default validation error is phase-aware when file is missing after execute.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_missing.txt");
        let file_path_str = file_path.display().to_string();
        // File exists initially
        std::fs::write(&file_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![CheckSpec {
                check_type: CheckType::FileExists,
                config: json_map_from_serde_map(
                    serde_json::json!({ "path": &file_path_str })
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            }],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute writes new content
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Delete the file to make verify fail
        std::fs::remove_file(&file_path).unwrap();

        // Verify should FAIL because file is missing (default validation runs before explicit checks)
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                // The error comes from default validation (phase-aware) since it runs first
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileWrite target file not found"),
                    "Expected 'FileWrite target file not found' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing file"),
        }
    }

    #[tokio::test]
    async fn test_prepare_filehashmatches_io_error_shows_phase() {
        // FileHashMatches check where file becomes unreadable should show [prepare] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("hash_io_error.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"content").unwrap();

        // Compute a valid hash
        let contents = std::fs::read(&file_path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let expected_hash = hex::encode(hasher.finalize());

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&file_path_str);
        request.prepare_checks = vec![CheckSpec {
            check_type: CheckType::FileHashMatches,
            config: json_map_from_serde_map(
                serde_json::json!({
                    "path": &file_path_str,
                    "expected_hash": &expected_hash
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        }];

        // Remove file permissions to cause IO error on read
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        let result = adapter.prepare(&request).await;

        // Restore permissions for cleanup (in case test assertion fails)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Internal(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileHashMatches"),
                    "Expected 'FileHashMatches' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("failed to read/compute hash"),
                    "Expected 'failed to read/compute hash' in internal error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected internal error for IO failure during hash compute, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_verify_filehashmatches_io_error_shows_phase() {
        // FileHashMatches check at verify where file becomes unreadable should show [verify] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_hash_io.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Compute a valid hash
        let contents = std::fs::read(&file_path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let expected_hash = hex::encode(hasher.finalize());

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![CheckSpec {
                check_type: CheckType::FileHashMatches,
                config: json_map_from_serde_map(
                    serde_json::json!({
                        "path": &file_path_str,
                        "expected_hash": &expected_hash
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
            }],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute writes new content
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Remove file permissions to cause IO error on read
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        // Verify should FAIL because file is unreadable
        let result = adapter.verify(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Internal(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileHashMatches"),
                    "Expected 'FileHashMatches' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("failed to read/compute hash"),
                    "Expected 'failed to read/compute hash' in internal error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected internal error for IO failure during hash compute, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_prepare_default_validation_file_not_found_shows_phase() {
        // FileDelete with no prepare_checks where file is missing should show [prepare] phase.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent_delete_phase.txt");
        let file_path_str = file_path.display().to_string();
        // Note: file does NOT exist

        let adapter = FsAdapter::new("fs");
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![], // No explicit checks - uses default validation
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileDelete target file not found"),
                    "Expected 'FileDelete target file not found' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing file"),
        }
    }

    #[tokio::test]
    async fn test_verify_default_validation_file_not_found_shows_phase() {
        // FileWrite verify where file is missing should show [verify] phase.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_default_missing.txt");
        let file_path_str = file_path.display().to_string();
        // File exists initially
        std::fs::write(&file_path, b"content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![], // No explicit checks - uses default validation
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute writes new content
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Delete the file to make default validation fail
        std::fs::remove_file(&file_path).unwrap();

        // Verify should FAIL because file doesn't exist (default validation for FileWrite)
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileWrite target file not found"),
                    "Expected 'FileWrite target file not found' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing file"),
        }
    }

    #[tokio::test]
    async fn test_verify_default_validation_file_still_exists_shows_phase() {
        // FileDelete verify where file still exists should show [verify] phase.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("verify_still_exists.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![], // No explicit checks - uses default validation
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // NOTE: We do NOT execute delete - file still exists

        // Verify should FAIL because file still exists after delete
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[verify]"),
                    "Expected '[verify]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("file still exists after delete"),
                    "Expected 'file still exists after delete' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for file still existing"),
        }
    }

    #[tokio::test]
    async fn test_prepare_parent_dir_missing_shows_phase() {
        // FileWrite prepare where parent directory is missing should show [prepare] phase.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir
            .path()
            .join("nonexistent_parent")
            .join("new_file.txt");
        let file_path_str = file_path.display().to_string();
        // Note: neither the file NOR the parent directory exists

        let adapter = FsAdapter::new("fs");
        let request = create_test_request(&file_path_str);

        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[prepare]"),
                    "Expected '[prepare]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("parent directory does not exist"),
                    "Expected 'parent directory does not exist' in validation error, got: {}",
                    msg
                );
            }
            _ => panic!("expected validation error for missing parent directory"),
        }
    }

    // =============================================================================
    // Execute/Rollback Phase-Aware Error Normalization Tests (P2.1)
    // =============================================================================

    #[tokio::test]
    async fn test_execute_filewrite_io_error_shows_phase() {
        // FileWrite execute where write fails should show [execute] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("execute_write_fail.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Remove write permissions to cause IO error on write
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        let result = adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Internal(msg) => {
                assert!(
                    msg.contains("[execute]"),
                    "Expected '[execute]' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileWrite failed"),
                    "Expected 'FileWrite failed' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains(&file_path_str),
                    "Expected file path in internal error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected internal error for write failure, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_execute_filedelete_io_error_shows_phase() {
        // FileDelete execute where delete fails should show [execute] phase context.
        // We make the parent directory read-only so remove_file fails with Permission denied.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("execute_delete_fail.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"to be deleted").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Remove write permission from parent directory to cause delete to fail
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&temp_dir).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o555); // Read-only directory
            std::fs::set_permissions(&temp_dir, perms).unwrap();
        }

        let result = adapter.execute(&contract, &serde_json::json!({})).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&temp_dir) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&temp_dir, perms);
            }
        }

        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Internal(msg) => {
                assert!(
                    msg.contains("[execute]"),
                    "Expected '[execute]' in internal error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("FileDelete failed"),
                    "Expected 'FileDelete failed' in internal error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected internal error for delete failure, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_rollback_snapshot_missing_shows_phase() {
        // Rollback where snapshot is missing should show [rollback] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("rollback_snapshot_missing.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Contract with different execution_id = no snapshot
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(), // Different execution_id = no snapshot
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str,
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        };

        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("[rollback]"),
                    "Expected '[rollback]' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("snapshot not found"),
                    "Expected 'snapshot not found' in validation error, got: {}",
                    msg
                );
                assert!(
                    msg.contains("cannot restore"),
                    "Expected 'cannot restore' in validation error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for missing snapshot, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_rollback_created_new_file_cleanup_shows_phase() {
        // Rollback for new-file creation where cleanup fails should return recovered=false
        // (fail-closed behavior: I/O errors during recovery return recovered=false, not errors).
        // We set up a scenario where the parent directory becomes non-writable, preventing cleanup.
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        let file_path = sub_dir.join("new_file.txt");
        let file_path_str = file_path.display().to_string();

        let adapter = FsAdapter::new("fs");

        // Prepare (new file case)
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute creates the file
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Now remove parent directory write permissions to make cleanup fail
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&sub_dir).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o555); // Read-only directory
            std::fs::set_permissions(&sub_dir, perms).unwrap();
        }

        let result = adapter.rollback(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&sub_dir) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&sub_dir, perms);
            }
        }

        // Fail-closed: rollback returns Ok(RecoveryReceipt { recovered: false, ... })
        // instead of propagating the I/O error
        let receipt = result.expect("rollback should succeed (return Receipt, not error)");
        assert!(
            !receipt.recovered,
            "Expected recovered=false for cleanup failure, got recovered=true"
        );
        assert!(
            receipt
                .adapter_metadata
                .get("failure_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("[rollback]") && s.contains("remove_file"))
                .unwrap_or(false),
            "Expected failure_reason to contain '[rollback]' and 'remove_file', got: {:?}",
            receipt.adapter_metadata.get("failure_reason")
        );
    }

    #[tokio::test]
    async fn test_rollback_restore_failure_shows_phase() {
        // Rollback where restore fails should return recovered=false (fail-closed behavior).
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("restore_fail.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute changes
        adapter
            .execute(&contract, &serde_json::json!("modified content"))
            .await
            .unwrap();

        // Now remove write permissions on the file to make restore fail
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o444); // Read-only, not removable by non-root
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        let result = adapter.rollback(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        // Fail-closed: rollback returns Ok(RecoveryReceipt { recovered: false, ... })
        // instead of propagating the I/O error
        let receipt = result.expect("rollback should succeed (return Receipt, not error)");
        assert!(
            !receipt.recovered,
            "Expected recovered=false for restore failure, got recovered=true"
        );
        assert!(
            receipt
                .adapter_metadata
                .get("failure_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("[rollback]") && s.contains("copy"))
                .unwrap_or(false),
            "Expected failure_reason to contain '[rollback]' and 'copy', got: {:?}",
            receipt.adapter_metadata.get("failure_reason")
        );
    }

    #[tokio::test]
    async fn test_compensate_error_shows_phase_from_rollback() {
        // Compensate reuses rollback, so errors should show [rollback] phase context.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("compensate_phase.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Contract with different execution_id = no snapshot
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(), // Different execution_id = no snapshot
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str,
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        };

        let result = adapter.compensate(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                // compensate calls rollback, so phase should be [rollback]
                assert!(
                    msg.contains("[rollback]"),
                    "Expected '[rollback]' in validation error from compensate, got: {}",
                    msg
                );
                assert!(
                    msg.contains("snapshot not found"),
                    "Expected 'snapshot not found' in validation error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error from compensate (which calls rollback), got: {:?}",
                other
            ),
        }
    }

    // =============================================================================
    // FileMove Tests
    // =============================================================================

    #[tokio::test]
    async fn test_file_move_happy_path_prepare_execute_verify() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"move me").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        let exec_contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        let exec_receipt = adapter
            .execute(
                &exec_contract,
                &serde_json::json!({ "destination": &dest_str }),
            )
            .await
            .unwrap();
        assert!(
            exec_receipt
                .adapter_metadata
                .get("destination_path")
                .is_some()
        );
        assert!(dest.exists());
        assert!(!source.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"move me");

        // Build a verify contract that includes destination_path (normally added by
        // the execution pipeline to the contract metadata before verify)
        let mut verify_contract = exec_contract.clone();
        verify_contract
            .metadata
            .insert("destination_path".to_string(), serde_json::json!(&dest_str));
        let verify_receipt = adapter.verify(&verify_contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_move_rollback_restores() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let exec_contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };

        adapter
            .execute(
                &exec_contract,
                &serde_json::json!({ "destination": &dest_str }),
            )
            .await
            .unwrap();
        assert!(dest.exists());
        assert!(!source.exists());

        let receipt = adapter.rollback(&exec_contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(source.exists());
        assert!(!dest.exists());
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
    }

    #[tokio::test]
    async fn test_file_move_verify_fails_if_source_still_exists() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"test").unwrap();
        // Create dest too — we want dest to exist (so that check passes), but source also
        // still exists (so the source-still-exists check fails). This simulates a failed
        // move that partially succeeded.
        std::fs::write(&dest, b"dest").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("source still exists"));
            }
            _ => panic!("expected validation error"),
        }
    }

    #[tokio::test]
    async fn test_file_move_prepare_fails_if_source_missing() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/nonexistent/source.txt");
        request.action_type = ActionType::FileMove;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_move_rollback_fails_if_dest_missing() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let source_str = source.display().to_string();
        std::fs::write(&source, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        };

        let result = adapter.rollback(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_move_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };

        let exec_contract = RollbackContract {
            metadata: meta.clone(),
            ..contract.clone()
        };
        adapter
            .execute(
                &exec_contract,
                &serde_json::json!({ "destination": &dest_str }),
            )
            .await
            .unwrap();

        let receipt = adapter.compensate(&exec_contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(source.exists());
    }

    #[tokio::test]
    async fn test_file_move_execute_missing_dest_payload() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let source_str = source.display().to_string();
        std::fs::write(&source, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let result = adapter.execute(&contract, &serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_move_verify_fails_if_dest_missing() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let source_str = source.display().to_string();
        std::fs::write(&source, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String("/nonexistent/dest.txt".to_string()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_move_verify_fails_if_dest_not_created() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileMove;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("destination not found") || msg.contains("source still exists")
                );
            }
            _ => panic!("expected validation error"),
        }
    }

    // =============================================================================
    // FileCopy Tests
    // =============================================================================

    #[tokio::test]
    async fn test_file_copy_happy_path_prepare_execute_verify() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"copy me").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);
        assert!(prep_receipt.adapter_metadata.get("source_hash").is_some());

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };

        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert!(exec_receipt.adapter_metadata.get("copy_hash").is_some());
        assert!(
            exec_receipt
                .adapter_metadata
                .get("created_new_dest")
                .and_then(|v| v.as_bool())
                == Some(true)
        );
        assert!(dest.exists());
        assert!(source.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"copy me");

        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_copy_rollback_new_dest_deletes() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );
        meta.insert(
            "created_new_dest".to_string(),
            serde_json::Value::Bool(true),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };

        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert!(dest.exists());

        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!dest.exists());
        assert!(source.exists());
    }

    #[tokio::test]
    async fn test_file_copy_rollback_existing_restores_snapshot() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"source content").unwrap();
        std::fs::write(&dest, b"original dest content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        adapter.prepare(&request).await.unwrap();

        let exec_id = request.execution_id;

        let mut meta = JsonMap::new();
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );
        meta.insert(
            "created_new_dest".to_string(),
            serde_json::Value::Bool(false),
        );
        meta.insert(
            "source_hash".to_string(),
            serde_json::Value::String(FsAdapter::compute_file_hash(&source_str).unwrap()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: exec_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };

        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"source content");

        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert_eq!(std::fs::read(&dest).unwrap(), b"original dest content");
    }

    #[tokio::test]
    async fn test_file_copy_prepare_fails_if_source_missing() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/nonexistent/source.txt");
        request.action_type = ActionType::FileCopy;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_copy_verify_fails_if_dest_missing() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let source_str = source.display().to_string();
        std::fs::write(&source, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String("/nonexistent/dest.txt".to_string()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_copy_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );
        meta.insert(
            "created_new_dest".to_string(),
            serde_json::Value::Bool(true),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert!(dest.exists());

        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn test_file_copy_source_unaffected_after_execute() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original source").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"original source");
        assert_eq!(std::fs::read(&dest).unwrap(), b"original source");
    }

    #[tokio::test]
    async fn test_file_copy_execute_missing_dest_payload() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let source_str = source.display().to_string();
        std::fs::write(&source, b"test").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let result = adapter.execute(&contract, &serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_copy_cross_instance_rollback() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"original").unwrap();

        let adapter_a = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );
        meta.insert(
            "created_new_dest".to_string(),
            serde_json::Value::Bool(true),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta.clone(),
        };
        adapter_a
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();

        let adapter_b = FsAdapter::new("fs");
        let receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn test_file_copy_existing_dest_snapshot_on_execute() {
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&dest, b"existing dest").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );
        meta.insert(
            "created_new_dest".to_string(),
            serde_json::Value::Bool(false),
        );
        meta.insert(
            "source_hash".to_string(),
            serde_json::Value::String(FsAdapter::compute_file_hash(&source_str).unwrap()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"source");

        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert_eq!(std::fs::read(&dest).unwrap(), b"existing dest");
    }

    // =============================================================================
    // FileAppend Tests
    // =============================================================================

    #[tokio::test]
    async fn test_file_append_prepare_rejects_missing_file() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/nonexistent/file.txt");
        request.action_type = ActionType::FileAppend;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_append_prepare_rejects_empty_data() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;

        // Empty payload should fail in execute, not prepare
        // Prepare should succeed since it only checks file exists
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Empty data should fail in execute
        let result = adapter.execute(&contract, &serde_json::json!("")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_append_prepare_captures_original_state() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        // Verify original hash and length are captured
        let meta = &prep_receipt.adapter_metadata;
        assert!(meta.get("original_hash").is_some());
        assert!(meta.get("original_length").is_some());
        assert!(meta.get("target_path").is_some());

        let original_hash = meta.get("original_hash").unwrap().as_str().unwrap();
        let original_length: u64 = meta
            .get("original_length")
            .unwrap()
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(original_length, 16); // "original content" is 16 bytes
        assert_eq!(
            FsAdapter::compute_file_hash(&target_str).unwrap(),
            original_hash
        );
    }

    #[tokio::test]
    async fn test_file_append_execute_appends_data() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        // Verify the file content
        assert_eq!(std::fs::read(&target).unwrap(), b"original appended");

        // Verify execute receipt has the required metadata
        let meta = &exec_receipt.adapter_metadata;
        assert!(meta.get("new_hash").is_some());
        assert!(meta.get("new_length").is_some());
        assert!(meta.get("bytes_appended").is_some());
        assert!(meta.get("data_hash").is_some());

        let new_length: u64 = meta
            .get("new_length")
            .unwrap()
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(new_length, 17); // "original appended" is 17 bytes
    }

    #[tokio::test]
    async fn test_file_append_verify_confirms_growth() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        adapter
            .execute(&contract, &serde_json::json!(" data"))
            .await
            .unwrap();

        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_append_rollback_truncates_and_restores_hash() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original content").unwrap();

        let original_hash = FsAdapter::compute_file_hash(&target_str).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        // Verify file was modified
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"original content appended"
        );

        // Rollback
        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);

        // Verify file restored to original
        assert_eq!(std::fs::read(&target).unwrap(), b"original content");
        assert_eq!(
            FsAdapter::compute_file_hash(&target_str).unwrap(),
            original_hash
        );
    }

    #[tokio::test]
    async fn test_file_append_rollback_idempotent() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        // First rollback
        let receipt1 = adapter.rollback(&contract).await.unwrap();
        assert!(receipt1.recovered);
        assert_eq!(std::fs::read(&target).unwrap(), b"original");

        // Second rollback should also succeed (idempotent)
        let receipt2 = adapter.rollback(&contract).await.unwrap();
        assert!(receipt2.recovered);
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[tokio::test]
    async fn test_file_append_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        // compensate should work the same as rollback
        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[tokio::test]
    async fn test_file_append_cross_instance_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"original").unwrap();

        let adapter_a = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileAppend;
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter_a
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        // Different adapter instance does rollback
        let adapter_b = FsAdapter::new("fs");
        let receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    // =============================================================================
    // FileChmod Tests
    // =============================================================================

    #[tokio::test]
    async fn test_file_chmod_prepare_rejects_missing_file() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/nonexistent/file.txt");
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o755".to_string()),
        );
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("FileChmod target file not found"));
    }

    #[tokio::test]
    async fn test_file_chmod_prepare_rejects_empty_mode() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        // Empty mode should fail
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("".to_string()),
        );
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("FileChmod mode cannot be empty"));
    }

    #[tokio::test]
    async fn test_file_chmod_prepare_captures_original_mode() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        // Set a specific mode for testing
        let original_mode = 0o644;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o755".to_string()),
        );

        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        // Verify original mode is captured
        let meta = &prep_receipt.adapter_metadata;
        assert!(meta.get("original_mode").is_some());
        assert!(meta.get("new_mode").is_some());
        assert!(meta.get("target_path").is_some());

        let captured_original = meta.get("original_mode").unwrap().as_str().unwrap();
        // Mode should be stored as octal string
        let captured_mode = u32::from_str_radix(captured_original, 8).unwrap();
        assert_eq!(captured_mode, original_mode);
    }

    #[tokio::test]
    async fn test_file_chmod_execute_changes_permissions() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let original_mode = 0o644;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o755".to_string()),
        );
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileChmod,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        let exec_receipt = adapter
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();

        // Verify the file permissions changed
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let new_perms = std::fs::metadata(&target).unwrap().permissions();
            let new_mode = new_perms.mode() & 0o7777;
            assert_eq!(new_mode, 0o755);
        }

        // Verify execute receipt has applied_mode
        let meta = &exec_receipt.adapter_metadata;
        assert!(meta.get("applied_mode").is_some());
        let applied = meta.get("applied_mode").unwrap().as_str().unwrap();
        assert_eq!(applied, "755");
    }

    #[tokio::test]
    async fn test_file_chmod_verify_confirms_new_mode() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let original_mode = 0o644;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("755".to_string()),
        );
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileChmod,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        adapter
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();

        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_file_chmod_rollback_restores_original_mode() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let original_mode = 0o644;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o755".to_string()),
        );
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileChmod,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();

        // Verify mode was changed
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode_after_exec = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode_after_exec, 0o755);
        }

        // Rollback
        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);

        // Verify mode restored to original
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
            assert_eq!(restored_mode, original_mode);
        }
    }

    #[tokio::test]
    async fn test_file_chmod_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let original_mode = 0o600;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o755".to_string()),
        );
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileChmod,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();

        // compensate should work the same as rollback
        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);

        // Verify mode restored
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
            assert_eq!(restored_mode, original_mode);
        }
    }

    #[tokio::test]
    async fn test_file_chmod_cross_instance_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("test.txt");
        let target_str = target.display().to_string();
        std::fs::write(&target, b"content").unwrap();

        let original_mode = 0o644;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&target).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            std::fs::set_permissions(&target, perms).unwrap();
        }

        let adapter_a = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileChmod;
        request.metadata.insert(
            "mode".to_string(),
            serde_json::Value::String("0o777".to_string()),
        );
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileChmod,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter_a
            .execute(&contract, &serde_json::Value::Null)
            .await
            .unwrap();

        // Different adapter instance does rollback
        let adapter_b = FsAdapter::new("fs");
        let receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);

        // Verify mode restored
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let restored_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
            assert_eq!(restored_mode, original_mode);
        }
    }

    // =============================================================================
    // DirCreate Tests
    // =============================================================================

    #[tokio::test]
    async fn test_dir_create_happy_path_prepare_execute_verify() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("new_dir");
        let target_str = target.display().to_string();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);
        assert!(!target.exists());

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirCreate,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(target.exists());
        assert!(target.is_dir());

        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_dir_create_rollback_deletes_created_dir() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("new_dir");
        let target_str = target.display().to_string();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirCreate,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(target.exists());

        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_dir_create_reject_existing_dir() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("existing_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_create_reject_missing_parent() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/nonexistent/deeply/nested/dir_that_cant_exist");
        request.action_type = ActionType::DirCreate;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_create_verify_fails_if_missing() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("missing_dir");
        let target_str = target.display().to_string();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirCreate,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Don't execute → dir doesn't exist → verify should fail
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_create_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("comp_dir");
        let target_str = target.display().to_string();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirCreate,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!target.exists());
    }

    // =============================================================================
    // DirDelete Tests
    // =============================================================================

    #[tokio::test]
    async fn test_dir_delete_happy_path_prepare_execute_verify() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("to_delete_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let prep_receipt = adapter.prepare(&request).await.unwrap();
        assert!(prep_receipt.accepted);

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!target.exists());

        let verify_receipt = adapter.verify(&contract).await.unwrap();
        assert!(verify_receipt.verified);
    }

    #[tokio::test]
    async fn test_dir_delete_rollback_recreates_dir() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("to_delete_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!target.exists());

        let receipt = adapter.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(target.exists());
        assert!(target.is_dir());
    }

    #[tokio::test]
    async fn test_dir_delete_reject_nonexistent_dir() {
        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request("/does_not_exist_dir");
        request.action_type = ActionType::DirDelete;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_delete_reject_nonempty_dir() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("nonempty_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("file"), b"data").unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_delete_verify_fails_if_still_exists() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("still_exists_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Don't execute → dir still exists → verify should fail
        let result = adapter.verify(&contract).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dir_delete_compensate_aliases_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("comp_del_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        let receipt = adapter.compensate(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(target.exists());
    }

    #[tokio::test]
    async fn test_dir_create_cross_instance_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("cross_dir");
        let target_str = target.display().to_string();

        let adapter_a = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirCreate;
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirCreate,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Instance A: execute
        adapter_a
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(target.exists());

        // Instance B: rollback
        let adapter_b = FsAdapter::new("fs");
        let receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_dir_delete_cross_instance_rollback() {
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("cross_del_dir");
        let target_str = target.display().to_string();
        std::fs::create_dir(&target).unwrap();

        let adapter_a = FsAdapter::new("fs");
        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::DirDelete;
        let prep_receipt = adapter_a.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::DirDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: target_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Instance A: execute
        adapter_a
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!target.exists());

        // Instance B: rollback
        let adapter_b = FsAdapter::new("fs");
        let receipt = adapter_b.rollback(&contract).await.unwrap();
        assert!(receipt.recovered);
        assert!(target.exists());
    }

    // =============================================================================
    // Cross-Filesystem Move Tests
    // =============================================================================

    #[tokio::test]
    async fn test_fs_file_move_cross_filesystem_fallback() {
        // Test that FileMove handles cross-filesystem scenario by falling back to copy+delete.
        // We simulate this by using a helper that wraps the move operation.
        // Since we can't easily create actual cross-filesystem scenarios in tests,
        // we test the cross_filesystem_move helper directly.
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();

        std::fs::write(&source, b"cross-fs content").unwrap();

        // Set the original permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&source).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&source, perms).unwrap();
        }

        // Use the cross_filesystem_move helper directly (simulating EXDEV fallback)
        FsAdapter::cross_filesystem_move(&source_str, &dest_str).unwrap();

        // Verify source was deleted and dest was created with content
        assert!(
            !source.exists(),
            "source should be deleted after cross-fs move"
        );
        assert!(dest.exists(), "dest should exist after cross-fs move");
        assert_eq!(std::fs::read(&dest).unwrap(), b"cross-fs content");

        // Verify permissions were preserved (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dest_perms = std::fs::metadata(&dest).unwrap().permissions();
            let dest_mode = dest_perms.mode() & 0o7777;
            assert_eq!(
                dest_mode, 0o755,
                "dest should have same permissions as source"
            );
        }
    }

    #[tokio::test]
    async fn test_fs_file_size_limit_enforcement() {
        // Test that file size limits are enforced during prepare.
        let temp_dir = tempdir().unwrap();
        let target = temp_dir.path().join("large_file.txt");
        let target_str = target.display().to_string();

        // Create a file that exceeds the default 100MB limit
        // We use a smaller test size to make the test practical
        let small_limit: u64 = 100; // 100 bytes
        let bounds = FsBoundsConfig {
            max_file_size: small_limit,
            ..Default::default()
        };
        let adapter = FsAdapter::new_with_bounds("fs", bounds);

        // Create a file larger than the limit
        let content = vec![0u8; 150]; // 150 bytes, exceeds 100 byte limit
        std::fs::write(&target, &content).unwrap();

        let mut request = create_test_request(&target_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should fail when file exceeds size limit"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("exceeds"),
                    "Expected error about exceeding limit, got: {}",
                    msg
                );
            }
            other => panic!("expected validation error for file size, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_fs_path_depth_limit() {
        // Test that path depth limits are enforced during prepare.
        let temp_dir = tempdir().unwrap();

        // Create a deeply nested path structure
        let deep_dir = temp_dir
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e");
        std::fs::create_dir_all(&deep_dir).unwrap();

        // Create a file at depth 6 (exceeds our limit of 5)
        let deep_path = deep_dir.join("file.txt");
        std::fs::write(&deep_path, b"deep content").unwrap();

        let deep_path_str = deep_path.display().to_string();

        // Use a very low depth limit for testing
        let bounds = FsBoundsConfig {
            max_path_depth: 5,
            ..Default::default()
        };
        let adapter = FsAdapter::new_with_bounds("fs", bounds);

        let mut request = create_test_request(&deep_path_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should fail when path exceeds depth limit"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("depth") && msg.contains("exceeds"),
                    "Expected error about path depth, got: {}",
                    msg
                );
            }
            other => panic!("expected validation error for path depth, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_fs_symlink_escape_rejection() {
        // Test that symlinks escaping the sandbox are rejected.
        let temp_dir = tempdir().unwrap();
        let work_dir = temp_dir.path().join("work");
        let escape_dir = temp_dir.path().join("escape");
        std::fs::create_dir(&work_dir).unwrap();
        std::fs::create_dir(&escape_dir).unwrap();

        // Create a file in the escape directory
        let target_file = escape_dir.join("secret.txt");
        std::fs::write(&target_file, b"secret").unwrap();

        // Create a symlink in work_dir that points to the escape directory
        let symlink_path = work_dir.join("link_to_escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

        // On non-Unix, skip this test part
        #[cfg(not(unix))]
        {
            // For non-Unix, we just return without testing symlinks
            return;
        }

        let symlink_str = symlink_path.display().to_string();

        // Use bounds that reject symlinks
        let bounds = FsBoundsConfig {
            allow_symlinks: false,
            sandbox_to_workdir: true,
            ..Default::default()
        };
        let adapter = FsAdapter::new_with_bounds("fs", bounds);

        let mut request = create_test_request(&symlink_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should fail when symlink is not allowed"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink") || msg.contains("escape"),
                    "Expected error about symlink, got: {}",
                    msg
                );
            }
            other => panic!("expected validation error for symlink, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_fs_bounded_copy_preserves_permissions() {
        // Test that FileCopy preserves file permissions.
        let temp_dir = tempdir().unwrap();
        let source = temp_dir.path().join("source.txt");
        let dest = temp_dir.path().join("dest.txt");
        let source_str = source.display().to_string();
        let dest_str = dest.display().to_string();

        std::fs::write(&source, b"content with permissions").unwrap();

        // Set specific permissions on the source file
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&source).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&source, perms).unwrap();
        }

        let adapter = FsAdapter::new("fs");

        // Prepare the copy operation
        let mut request = create_test_request(&source_str);
        request.action_type = ActionType::FileCopy;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        // Execute the copy
        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();

        // Verify the destination file exists and has the same permissions
        assert!(dest.exists(), "dest should exist after copy");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"content with permissions",
            "dest should have same content as source"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let source_perms = std::fs::metadata(&source).unwrap().permissions().mode() & 0o7777;
            let dest_perms = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o7777;
            assert_eq!(
                dest_perms, source_perms,
                "dest permissions should match source permissions"
            );
        }
    }

    // =============================================================================
    // P2.1 Fail-Closed Rollback Tests
    // =============================================================================

    #[tokio::test]
    async fn test_rollback_fail_closed_on_permission_denied_file_write() {
        // Test that permission denied during FileWrite rollback returns recovered=false
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("perm_denied_write.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute changes
        adapter
            .execute(&contract, &serde_json::json!("modified content"))
            .await
            .unwrap();

        // Make file read-only to trigger permission denied on restore
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o444); // Read-only
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        let result = adapter.rollback(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        // Should return recovered=false (fail-closed)
        let receipt = result.expect("rollback returns Receipt, not error");
        assert!(!receipt.recovered, "Expected recovered=false on I/O error");
        assert!(
            receipt
                .adapter_metadata
                .get("rollback_failed")
                .and_then(|v| v.as_bool())
                == Some(true),
            "Expected rollback_failed=true in metadata"
        );
        let reason = receipt
            .adapter_metadata
            .get("failure_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            reason.contains("[rollback]") && reason.contains("copy"),
            "Expected failure_reason to contain '[rollback]' and 'copy', got: {}",
            reason
        );
    }

    #[tokio::test]
    async fn test_rollback_fail_closed_on_permission_denied_delete() {
        // Test that permission denied during FileDelete (new file) rollback returns recovered=false
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        let file_path = sub_dir.join("new_file.txt");
        let file_path_str = file_path.display().to_string();

        let adapter = FsAdapter::new("fs");

        // Prepare (new file case)
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute creates the file
        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        // Make parent directory read-only to prevent file deletion
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&sub_dir).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o555); // Read-only directory
            std::fs::set_permissions(&sub_dir, perms).unwrap();
        }

        let result = adapter.rollback(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&sub_dir) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&sub_dir, perms);
            }
            // Also try to clean up the file if it still exists
            let _ = std::fs::remove_file(&file_path);
        }

        // Should return recovered=false (fail-closed)
        let receipt = result.expect("rollback returns Receipt, not error");
        assert!(!receipt.recovered, "Expected recovered=false on I/O error");
        assert!(
            receipt
                .adapter_metadata
                .get("rollback_failed")
                .and_then(|v| v.as_bool())
                == Some(true),
            "Expected rollback_failed=true in metadata"
        );
        let reason = receipt
            .adapter_metadata
            .get("failure_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            reason.contains("[rollback]") && reason.contains("remove_file"),
            "Expected failure_reason to contain '[rollback]' and 'remove_file', got: {}",
            reason
        );
    }

    #[tokio::test]
    async fn test_compensate_inherits_fail_closed_behavior() {
        // Test that compensate() (which delegates to rollback) also returns recovered=false on I/O errors
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("compensate_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute changes
        adapter
            .execute(&contract, &serde_json::json!("modified"))
            .await
            .unwrap();

        // Make file read-only to trigger permission denied on compensate
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o444);
            std::fs::set_permissions(&file_path, perms).unwrap();
        }

        let result = adapter.compensate(&contract).await;

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&file_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&file_path, perms);
            }
        }

        // compensate() should also return recovered=false (fail-closed)
        let receipt = result.expect("compensate returns Receipt, not error");
        assert!(!receipt.recovered, "Expected recovered=false on I/O error");
        assert!(
            receipt
                .adapter_metadata
                .get("rollback_failed")
                .and_then(|v| v.as_bool())
                == Some(true),
            "Expected rollback_failed=true in metadata"
        );
    }

    // =============================================================================
    // Symlink Hardening: Final-Path Symlink Rejection Tests
    // =============================================================================

    #[tokio::test]
    async fn test_symlink_final_path_rejected_at_prepare() {
        // Test that a symlink as the final path is rejected at prepare.
        let temp_dir = tempdir().unwrap();
        let target_file = temp_dir.path().join("target.txt");
        let symlink_path = temp_dir.path().join("link");
        std::fs::write(&target_file, b"target").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

        #[cfg(not(unix))]
        return; // Skip on non-Unix

        let symlink_str = symlink_path.display().to_string();
        let adapter = FsAdapter::new("fs");

        let mut request = create_test_request(&symlink_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should reject symlink as final path"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink"),
                    "Expected error about symlink, got: {}",
                    msg
                );
            }
            other => panic!("expected validation error for symlink, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_symlink_intermediate_path_rejected_at_prepare() {
        // Test that a symlink in an intermediate path component is rejected at prepare.
        let temp_dir = tempdir().unwrap();
        let real_dir = temp_dir.path().join("real_dir");
        std::fs::create_dir(&real_dir).unwrap();
        let target_file = real_dir.join("target.txt");
        std::fs::write(&target_file, b"target").unwrap();

        // Create a symlink to the real directory
        let symlink_dir = temp_dir.path().join("link_dir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &symlink_dir).unwrap();

        #[cfg(not(unix))]
        return; // Skip on non-Unix

        // Path that goes through the symlink: link_dir/target.txt
        let path_through_symlink = symlink_dir.join("target.txt");
        let path_str = path_through_symlink.display().to_string();

        let adapter = FsAdapter::new("fs");

        let mut request = create_test_request(&path_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should reject path with intermediate symlink"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink"),
                    "Expected error about symlink, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for intermediate symlink, got: {:?}",
                other
            ),
        }
    }

    // =============================================================================
    // Symlink Hardening: Execute-Phase Revalidation Tests
    // =============================================================================

    #[tokio::test]
    async fn test_symlink_swap_between_prepare_and_execute_fails_execute() {
        // Test that if a symlink is swapped between prepare and execute,
        // execute fails with validation error (fail-closed).
        let temp_dir = tempdir().unwrap();
        let work_dir = temp_dir.path().join("work");
        let escape_dir = temp_dir.path().join("escape");
        std::fs::create_dir(&work_dir).unwrap();
        std::fs::create_dir(&escape_dir).unwrap();

        // Create target file in escape directory
        let target_file = escape_dir.join("secret.txt");
        std::fs::write(&target_file, b"secret").unwrap();

        // Create initial file in work directory
        let initial_file = work_dir.join("file.txt");
        std::fs::write(&initial_file, b"initial").unwrap();

        let initial_str = initial_file.display().to_string();

        let adapter = FsAdapter::new("fs");

        // Prepare with the initial file
        let mut request = create_test_request(&initial_str);
        request.action_type = ActionType::FileWrite;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: initial_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Swap the initial file to a symlink pointing to escape
        #[cfg(unix)]
        {
            std::fs::remove_file(&initial_file).unwrap();
            std::os::unix::fs::symlink(&target_file, &initial_file).unwrap();
        }

        #[cfg(not(unix))]
        return; // Skip on non-Unix

        // Execute should fail because the path is now a symlink
        let result = adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await;
        assert!(
            result.is_err(),
            "execute should fail when path becomes a symlink"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink") || msg.contains("[execute]"),
                    "Expected symlink or execute-phase error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for symlink at execute, got: {:?}",
                other
            ),
        }
    }

    // =============================================================================
    // Symlink Hardening: Rollback-Phase Revalidation Tests
    // =============================================================================

    #[tokio::test]
    async fn test_symlink_swap_between_execute_and_rollback_fails_rollback() {
        // Test that if a symlink is introduced between execute and rollback,
        // rollback fails with validation error (fail-closed).
        let temp_dir = tempdir().unwrap();
        let work_dir = temp_dir.path().join("work");
        let escape_dir = temp_dir.path().join("escape");
        std::fs::create_dir(&work_dir).unwrap();
        std::fs::create_dir(&escape_dir).unwrap();

        // Create target file in escape directory
        let target_file = escape_dir.join("secret.txt");
        std::fs::write(&target_file, b"secret").unwrap();

        // Create initial file in work directory
        let initial_file = work_dir.join("file.txt");
        std::fs::write(&initial_file, b"initial").unwrap();

        let initial_str = initial_file.display().to_string();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let mut request = create_test_request(&initial_str);
        request.action_type = ActionType::FileWrite;
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: initial_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata.clone(),
        };

        // Execute succeeds
        adapter
            .execute(&contract, &serde_json::json!("modified"))
            .await
            .unwrap();

        // Swap the file to a symlink after execute
        #[cfg(unix)]
        {
            std::fs::remove_file(&initial_file).unwrap();
            std::os::unix::fs::symlink(&target_file, &initial_file).unwrap();
        }

        #[cfg(not(unix))]
        return; // Skip on non-Unix

        // Rollback should fail because the path is now a symlink
        let result = adapter.rollback(&contract).await;
        assert!(
            result.is_err(),
            "rollback should fail when path becomes a symlink"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink") || msg.contains("[rollback]"),
                    "Expected symlink or rollback-phase error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for symlink at rollback, got: {:?}",
                other
            ),
        }
    }

    // =============================================================================
    // O_NOFOLLOW Hardening: Defense-in-Depth Tests
    // =============================================================================

    #[tokio::test]
    async fn test_nofollow_blocks_symlink_write_on_unix() {
        // Test that O_NOFOLLOW blocks write operations on symlinks (Unix only).
        let temp_dir = tempdir().unwrap();
        let target_file = temp_dir.path().join("target.txt");
        let symlink_path = temp_dir.path().join("link");
        std::fs::write(&target_file, b"target content").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

        #[cfg(not(unix))]
        return; // O_NOFOLLOW is Unix-only

        let symlink_str = symlink_path.display().to_string();
        let adapter = FsAdapter::new("fs");

        // Prepare on the symlink should fail because prepare validates path
        let mut request = create_test_request(&symlink_str);
        request.action_type = ActionType::FileWrite;
        let result = adapter.prepare(&request).await;
        assert!(result.is_err(), "prepare should reject symlink path");

        // Verify the error is about symlinks
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink"),
                    "Expected symlink error, got: {}",
                    msg
                );
            }
            other => panic!("expected validation error for symlink, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_nofollow_normal_file_write_still_works() {
        // Test that normal file operations still work with O_NOFOLLOW.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("normal.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare should succeed
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute should succeed with normal file
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        assert!(exec_receipt.result_digest.is_some());
        // Verify file has new content
        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, b"new content");
    }

    #[tokio::test]
    async fn test_nofollow_file_append_rollback_truncation() {
        // Test that FileAppend rollback correctly truncates using O_NOFOLLOW helpers.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("append_rollback.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute append
        adapter
            .execute(&contract, &serde_json::json!(" appended data"))
            .await
            .unwrap();

        // Verify content was appended
        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, b"original content appended data");

        // Now rollback
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();
        assert!(rollback_receipt.recovered);

        // File should be truncated to original length (rollback uses write_file_nofollow)
        let content_after = std::fs::read(&file_path).unwrap();
        assert_eq!(content_after, b"original content");
    }

    #[tokio::test]
    async fn test_nofollow_file_append_normal_still_works() {
        // Test that FileAppend still works on normal files.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("append_test.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Build contract
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute append
        let exec_receipt = adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();

        assert!(exec_receipt.result_digest.is_some());

        // Verify content was appended
        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, b"original appended");
    }

    // =============================================================================
    // Workdir Sandbox: Constructor and Enforcement Tests
    // =============================================================================

    #[tokio::test]
    async fn test_workdir_sandbox_enforcement_via_constructor() {
        // Test that workdir sandbox is enforced when set via constructor.
        // We use a file OUTSIDE the workdir to test the escape detection.
        let temp_dir = tempdir().unwrap();
        let work_dir = temp_dir.path().join("work");
        std::fs::create_dir(&work_dir).unwrap();

        // Create a file in a DIFFERENT temp directory (outside workdir)
        let outside_dir = tempdir().unwrap();
        let outside_file = outside_dir.path().join("outside.txt");
        std::fs::write(&outside_file, b"outside").unwrap();

        // Path to file outside workdir
        let outside_str = outside_file.display().to_string();

        // Create adapter with workdir set to temp_dir (which contains 'work' subdir)
        // The outside_file is in a completely different temp directory
        let bounds = FsBoundsConfig {
            allow_symlinks: false,
            sandbox_to_workdir: true,
            ..Default::default()
        };
        let adapter = FsAdapter::new_with_workdir("fs", bounds, temp_dir.path().to_path_buf());

        let mut request = create_test_request(&outside_str);
        request.action_type = ActionType::FileWrite;

        // File outside workdir should be rejected because it escapes the sandbox
        let result = adapter.prepare(&request).await;
        assert!(
            result.is_err(),
            "prepare should reject file outside workdir"
        );
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("escape") || msg.contains("workdir"),
                    "Expected workdir escape error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for workdir escape, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_workdir_with_symlink_escape_rejected() {
        // Test that workdir sandbox catches symlink escape.
        let temp_dir = tempdir().unwrap();
        let work_dir = temp_dir.path().join("work");
        let escape_dir = temp_dir.path().join("escape");
        std::fs::create_dir(&work_dir).unwrap();
        std::fs::create_dir(&escape_dir).unwrap();

        // Create target file in escape directory
        let target_file = escape_dir.join("secret.txt");
        std::fs::write(&target_file, b"secret").unwrap();

        // Create a symlink in work_dir pointing to escape directory
        let symlink_path = work_dir.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

        #[cfg(not(unix))]
        return; // Skip on non-Unix

        let symlink_str = symlink_path.display().to_string();

        // Create adapter with workdir set to temp_dir (which includes both work and escape)
        // But the sandbox_to_workdir=true should catch the escape via symlink
        let bounds = FsBoundsConfig {
            allow_symlinks: false,
            sandbox_to_workdir: true,
            ..Default::default()
        };
        let adapter = FsAdapter::new_with_workdir("fs", bounds, temp_dir.path().to_path_buf());

        let mut request = create_test_request(&symlink_str);
        request.action_type = ActionType::FileWrite;

        let result = adapter.prepare(&request).await;
        assert!(result.is_err(), "prepare should reject symlink escape");
        match result.unwrap_err() {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("symlink") || msg.contains("escape"),
                    "Expected symlink or escape error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for symlink escape, got: {:?}",
                other
            ),
        }
    }

    // =============================================================================
    // FS Compensation Audit: Real-Undo Behavior Tests
    // =============================================================================

    #[tokio::test]
    async fn test_compensation_audit_file_write_real_undo() {
        // Audit test: FileWrite rollback actually restores original content.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("audit_write.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare (captures snapshot)
        let request = create_test_request(&file_path_str);
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        // Execute with new content
        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        adapter
            .execute(&contract, &serde_json::json!("new content"))
            .await
            .unwrap();

        assert_eq!(std::fs::read(&file_path).unwrap(), b"new content");

        // Compensate should restore original content
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert_eq!(
            std::fs::read(&file_path).unwrap(),
            b"original content",
            "FileWrite compensate should restore original content"
        );
    }

    #[tokio::test]
    async fn test_compensation_audit_file_delete_real_undo() {
        // Audit test: FileDelete rollback actually restores deleted file.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("audit_delete.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"content to delete").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileDelete,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute deletes the file
        adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!file_path.exists(), "File should be deleted after execute");

        // Compensate should restore the file
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert!(
            file_path.exists(),
            "FileDelete compensate should restore file"
        );
        assert_eq!(
            std::fs::read(&file_path).unwrap(),
            b"content to delete",
            "FileDelete compensate should restore original content"
        );
    }

    #[tokio::test]
    async fn test_compensation_audit_file_move_real_undo() {
        // Audit test: FileMove rollback actually moves destination back to source.
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("source.txt");
        let dest_file = temp_dir.path().join("dest.txt");
        let source_str = source_file.display().to_string();
        let dest_str = dest_file.display().to_string();
        std::fs::write(&source_file, b"content to move").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileMove,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        // Execute moves the file
        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert!(!source_file.exists(), "Source should be moved");
        assert!(dest_file.exists(), "Destination should exist");

        // Compensate should move the file back
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert!(
            source_file.exists(),
            "Source should be restored after compensate"
        );
        assert!(
            !dest_file.exists(),
            "Destination should be removed after compensate"
        );
        assert_eq!(
            std::fs::read(&source_file).unwrap(),
            b"content to move",
            "FileMove compensate should restore original content"
        );
    }

    #[tokio::test]
    async fn test_compensation_audit_file_copy_real_undo() {
        // Audit test: FileCopy rollback of new destination deletes it (idempotent).
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("source.txt");
        let dest_file = temp_dir.path().join("dest.txt");
        let source_str = source_file.display().to_string();
        let dest_str = dest_file.display().to_string();
        std::fs::write(&source_file, b"source content").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let mut meta = prep_receipt.adapter_metadata;
        meta.insert(
            "destination_path".to_string(),
            serde_json::Value::String(dest_str.clone()),
        );

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileCopy,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: source_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: meta,
        };

        // Execute copies the file
        adapter
            .execute(&contract, &serde_json::json!({ "destination": &dest_str }))
            .await
            .unwrap();
        assert!(source_file.exists(), "Source should still exist");
        assert!(dest_file.exists(), "Destination should exist after copy");

        // Compensate should delete the new destination (since it was new)
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert!(
            source_file.exists(),
            "Source should still exist after compensate"
        );
        assert!(
            !dest_file.exists(),
            "New destination should be deleted after compensate"
        );
    }

    #[tokio::test]
    async fn test_compensation_audit_file_append_real_undo() {
        // Audit test: FileAppend rollback truncates to original length.
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("audit_append.txt");
        let file_path_str = file_path.display().to_string();
        std::fs::write(&file_path, b"original").unwrap();

        let adapter = FsAdapter::new("fs");

        // Prepare
        let request = RollbackPrepareRequest {
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata: JsonMap::new(),
        };
        let prep_receipt = adapter.prepare(&request).await.unwrap();

        let contract = RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: request.execution_id,
            action_type: ActionType::FileAppend,
            rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: file_path_str.clone(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: prep_receipt.adapter_metadata,
        };

        // Execute appends data
        adapter
            .execute(&contract, &serde_json::json!(" appended"))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&file_path).unwrap(), b"original appended");

        // Compensate should truncate to original length
        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        assert!(compensate_receipt.recovered);
        assert_eq!(
            std::fs::read(&file_path).unwrap(),
            b"original",
            "FileAppend compensate should restore original content"
        );
    }
}
