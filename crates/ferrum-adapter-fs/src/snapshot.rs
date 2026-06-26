use std::path::{Path, PathBuf};

use ferrum_proto::ExecutionId;
use sha2::{Digest, Sha256};

use crate::FsAdapterError;

/// Computes a deterministic snapshot subdirectory path based on execution_id and target path.
///
/// Path structure: `{snapshot_root}/{execution_id}/{path_hash}`
/// where `path_hash` is the first 16 hex chars of SHA-256 hash of the canonical target path.
pub fn compute_snapshot_path(
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
pub fn capture_snapshot(
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

/// Computes the SHA-256 hash of a file at the given path.
pub fn compute_file_hash(path: &str) -> Result<String, FsAdapterError> {
    let contents = read_file_nofollow(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Reads a file without following symbolic links in the final component (Unix O_NOFOLLOW).
/// On Unix: uses OpenOptions with custom_flags(O_NOFOLLOW) to open the file.
/// On non-Unix: falls back to std::fs::read.
pub fn read_file_nofollow(path: &str) -> Result<Vec<u8>, FsAdapterError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| map_io_error_to_fs(e, path))?;
        use std::io::Read;
        let mut reader = file;
        let mut contents = Vec::new();
        reader
            .read_to_end(&mut contents)
            .map_err(|e| map_io_error_to_fs(e, path))?;
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
pub fn write_file_nofollow(path: &str, contents: &[u8]) -> Result<(), FsAdapterError> {
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
            .map_err(|e| map_io_error_to_fs(e, path))?
            .write_all(contents)
            .map_err(|e| map_io_error_to_fs(e, path))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents).map_err(FsAdapterError::Io)
    }
}

/// Maps an IO error to FsAdapterError, converting Unix ELOOP to SymlinkNotAllowed.
pub fn map_io_error_to_fs(err: std::io::Error, path: &str) -> FsAdapterError {
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
