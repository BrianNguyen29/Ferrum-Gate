use std::ffi::OsString;
use std::path::{Component, Path as StdPath, PathBuf};

use ferrum_proto::ResourceSelector;

/// Validates that `resource_bindings` is a subset of `resource_scope`.
///
/// Returns `Ok(())` if all capability resource bindings are within the intent's
/// resource scope, `Err(reason)` if any binding exceeds the scope.
///
/// Uses component-aware lexical path matching, with canonicalization when an
/// existing ancestor is available. For example:
/// - binding path `/tmp/subdir/file.txt` is within scope path `/tmp` ✓
/// - binding path `/tmp2/file.txt` is not within scope path `/tmp` ✓
/// - binding path `/other/file.txt` is NOT within scope path `/tmp` ✗
///
/// An empty `resource_bindings` is always valid (represents no specific resources).
/// An empty `resource_scope` with non-empty `resource_bindings` is always invalid.
pub(crate) fn validate_resource_bindings_subset_of_scope(
    resource_bindings: &[ferrum_proto::ResourceBinding],
    resource_scope: &[ResourceSelector],
) -> Result<(), String> {
    // Empty bindings is always valid (no specific resources requested)
    if resource_bindings.is_empty() {
        return Ok(());
    }

    // Empty scope with non-empty bindings = invalid (cannot expand beyond empty scope)
    if resource_scope.is_empty() {
        return Err("resource scope is empty but capability has resource bindings".to_string());
    }

    for binding in resource_bindings {
        let covered = match binding {
            ferrum_proto::ResourceBinding::File {
                path,
                mode,
                required_hash,
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::FilesystemPath {
                    path: scope_path,
                    mode: scope_mode,
                    content_hash,
                } = scope
                {
                    path_within_scope(path, scope_path)
                        && mode_allows(scope_mode, mode)
                        && content_hash
                            .as_ref()
                            .is_none_or(|hash| required_hash.as_ref() == Some(hash))
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::Git {
                repo_path,
                allowed_refs,
                mode,
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::GitRepository {
                    repo_path: scope_repo_path,
                    allowed_refs: scope_refs,
                    mode: scope_mode,
                } = scope
                {
                    path_within_scope(repo_path, scope_repo_path)
                        && list_is_subset(allowed_refs, scope_refs)
                        && mode_allows(scope_mode, mode)
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::Sqlite {
                db_path,
                tables,
                mode,
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::SqliteDatabase {
                    db_path: scope_db_path,
                    tables: scope_tables,
                    mode: scope_mode,
                } = scope
                {
                    path_within_scope(db_path, scope_db_path)
                        && list_is_subset(tables, scope_tables)
                        && mode_allows(scope_mode, mode)
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::Http {
                method,
                base_url,
                path_prefix,
                mode,
                ..
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::HttpEndpoint {
                    method: scope_method,
                    base_url: scope_base_url,
                    path_prefix: scope_path_prefix,
                    mode: scope_mode,
                } = scope
                {
                    method == scope_method
                        && http_base_within_scope(base_url, scope_base_url)
                        && url_path_within_scope(path_prefix, scope_path_prefix)
                        && mode_allows(scope_mode, mode)
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::EmailDraft {
                recipients, mode, ..
            } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::EmailDraft {
                        recipient_allowlist,
                        mode: scope_mode,
                        ..
                    } = scope
                    {
                        // Email matching: recipient must end with an allowlist entry.
                        // E.g., "user@example.com" ends with "@example.com" ✓
                        recipients
                            .iter()
                            .all(|r| recipient_allowlist.iter().any(|a| r.ends_with(a)))
                            && mode_allows(scope_mode, mode)
                    } else {
                        false
                    }
                })
            }
        };

        if !covered {
            return Err(format!(
                "capability resource binding {:?} is not within intent resource scope",
                binding
            ));
        }
    }

    Ok(())
}

fn mode_allows(scope: &ferrum_proto::ResourceMode, binding: &ferrum_proto::ResourceMode) -> bool {
    scope == binding
        || matches!(scope, ferrum_proto::ResourceMode::Admin)
        || matches!(
            (scope, binding),
            (
                ferrum_proto::ResourceMode::ReadWrite,
                ferrum_proto::ResourceMode::Read
                    | ferrum_proto::ResourceMode::Write
                    | ferrum_proto::ResourceMode::ReadWrite
            )
        )
}

fn list_is_subset(binding: &[String], scope: &[String]) -> bool {
    scope.is_empty() || binding.iter().all(|item| scope.contains(item))
}

fn lexical_normalize(path: &str) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in StdPath::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => return None,
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    Some(normalized)
}

fn canonicalize_with_missing_tail(path: &StdPath) -> Option<PathBuf> {
    let mut ancestor = path;
    let mut tail: Vec<OsString> = Vec::new();
    while !ancestor.exists() {
        tail.push(ancestor.file_name()?.to_os_string());
        ancestor = ancestor.parent()?;
    }
    let mut resolved = ancestor.canonicalize().ok()?;
    for component in tail.into_iter().rev() {
        resolved.push(component);
    }
    Some(resolved)
}

fn path_within_scope(candidate: &str, scope: &str) -> bool {
    let Some(candidate) = lexical_normalize(candidate) else {
        return false;
    };
    let Some(scope) = lexical_normalize(scope) else {
        return false;
    };
    if !candidate.starts_with(&scope) {
        return false;
    }

    match (
        canonicalize_with_missing_tail(&candidate),
        canonicalize_with_missing_tail(&scope),
    ) {
        (Some(candidate), Some(scope)) => candidate.starts_with(scope),
        _ => true,
    }
}

fn url_path_within_scope(candidate: &str, scope: &str) -> bool {
    path_within_scope(
        &format!("/{}", candidate.trim_start_matches('/')),
        &format!("/{}", scope.trim_start_matches('/')),
    )
}

fn http_base_within_scope(candidate: &str, scope: &str) -> bool {
    let Ok(candidate) = reqwest::Url::parse(candidate) else {
        return false;
    };
    let Ok(scope) = reqwest::Url::parse(scope) else {
        return false;
    };
    candidate.scheme() == scope.scheme()
        && candidate.host_str() == scope.host_str()
        && candidate.port_or_known_default() == scope.port_or_known_default()
        && candidate.username() == scope.username()
        && candidate.password() == scope.password()
        && url_path_within_scope(candidate.path(), scope.path())
}
