use super::*;
use ferrum_proto::{ResourceSelector, RollbackTarget};

#[test]
fn test_infer_git_adapter_key_git_repository() {
    let scope = vec![ResourceSelector::GitRepository {
        repo_path: "/tmp/test-repo".to_string(),
        allowed_refs: vec!["main".to_string(), "develop".to_string()],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    }];
    assert_eq!(infer_git_adapter_key(&scope), "git");
}

#[test]
fn test_infer_git_adapter_key_no_git() {
    let scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp/file.txt".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
        content_hash: None,
    }];
    assert_eq!(infer_git_adapter_key(&scope), "noop");
}

#[test]
fn test_infer_git_adapter_key_empty_scope() {
    let scope: Vec<ResourceSelector> = vec![];
    assert_eq!(infer_git_adapter_key(&scope), "noop");
}

#[test]
fn test_infer_git_adapter_key_mixed_scope() {
    let scope = vec![
        ResourceSelector::FilesystemPath {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::ReadWrite,
            content_hash: None,
        },
        ResourceSelector::GitRepository {
            repo_path: "/tmp/test-repo".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::ReadWrite,
        },
    ];
    assert_eq!(infer_git_adapter_key(&scope), "git");
}

#[test]
fn test_determine_rollback_target_from_bindings_git_ref() {
    let scope = vec![ResourceSelector::GitRepository {
        repo_path: "/opt/myrepo".to_string(),
        allowed_refs: vec!["main".to_string()],
        mode: ferrum_proto::ResourceMode::ReadWrite,
    }];
    let target = determine_rollback_target_from_bindings(&scope);
    match target {
        RollbackTarget::GitRef {
            repo_path,
            before_ref,
            after_ref,
        } => {
            assert_eq!(repo_path, "/opt/myrepo");
            assert!(before_ref.is_none());
            assert!(after_ref.is_none());
        }
        other => panic!("expected GitRef target, got {:?}", other),
    }
}

#[test]
fn test_determine_rollback_target_from_bindings_generic_fallback() {
    let scope = vec![ResourceSelector::FilesystemPath {
        path: "/tmp/file.txt".to_string(),
        mode: ferrum_proto::ResourceMode::ReadWrite,
        content_hash: None,
    }];
    let target = determine_rollback_target_from_bindings(&scope);
    match target {
        RollbackTarget::Generic {
            namespace,
            identifier,
        } => {
            assert_eq!(namespace, "unknown");
            assert_eq!(identifier, "binding");
        }
        other => panic!("expected Generic fallback, got {:?}", other),
    }
}

#[test]
fn test_determine_rollback_target_from_bindings_empty_scope() {
    let scope: Vec<ResourceSelector> = vec![];
    let target = determine_rollback_target_from_bindings(&scope);
    match target {
        RollbackTarget::Generic {
            namespace,
            identifier,
        } => {
            assert_eq!(namespace, "unknown");
            assert_eq!(identifier, "binding");
        }
        other => panic!("expected Generic fallback, got {:?}", other),
    }
}

#[test]
fn test_determine_rollback_target_from_bindings_first_git_wins() {
    // When multiple git repos are in scope, returns the first one
    let scope = vec![
        ResourceSelector::GitRepository {
            repo_path: "/repo/one".to_string(),
            allowed_refs: vec![],
            mode: ferrum_proto::ResourceMode::Read,
        },
        ResourceSelector::GitRepository {
            repo_path: "/repo/two".to_string(),
            allowed_refs: vec![],
            mode: ferrum_proto::ResourceMode::Read,
        },
    ];
    let target = determine_rollback_target_from_bindings(&scope);
    match target {
        RollbackTarget::GitRef { repo_path, .. } => {
            assert_eq!(repo_path, "/repo/one");
        }
        other => panic!("expected GitRef target, got {:?}", other),
    }
}
