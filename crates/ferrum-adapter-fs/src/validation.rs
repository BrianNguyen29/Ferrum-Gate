use ferrum_rollback::AdapterError;
use std::path::Path;

use crate::FsAdapterError;

/// Validates that the given path exists and is a file.
pub fn validate_path_exists(path: &str) -> Result<(), AdapterError> {
    let p = Path::new(path);
    if !p.exists() || !p.is_file() {
        return Err(FsAdapterError::FilePathNotFound(path.to_string()).into());
    }
    Ok(())
}

/// Validates path depth against the configured maximum.
/// Returns an error if the path has more components than max_path_depth.
pub fn validate_path_depth(path: &str, max_depth: usize) -> Result<(), FsAdapterError> {
    let depth = Path::new(path).components().count();
    if depth > max_depth {
        return Err(FsAdapterError::PathDepthExceedsLimit(depth, max_depth));
    }
    Ok(())
}

/// Validates file size is within the configured limit.
/// Returns an error if file size exceeds max_file_size.
pub fn validate_file_size(path: &str, max_size: u64) -> Result<(), FsAdapterError> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();
    if size > max_size {
        return Err(FsAdapterError::FileSizeExceedsLimit(size, max_size));
    }
    Ok(())
}

/// Parses a mode string into a u32.
/// Handles both "0o755" (octal with prefix) and "755" (octal without prefix) formats.
pub fn parse_mode_string(mode_str: &str) -> Result<u32, String> {
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
