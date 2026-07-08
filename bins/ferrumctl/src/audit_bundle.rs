pub use ferrum_audit_bundle::{AuditBundleManifest, verify_bundle};

use ferrum_audit_bundle::{build_bundle_from_ndjson, export_bundle as export_bundle_impl};

#[cfg(test)]
use std::fs;
use std::path::Path;

/// Export a bundle to a directory from an NDJSON body.
///
/// `dir` must exist or will be created. `body` is the NDJSON response from the
/// server export endpoint. Returns the computed manifest.
pub fn export_bundle(dir: &Path, body: &str) -> anyhow::Result<AuditBundleManifest> {
    let bundle = build_bundle_from_ndjson(body)
        .map_err(|e| anyhow::anyhow!("failed to build audit bundle: {e}"))?;
    export_bundle_impl(dir, &bundle)
        .map_err(|e| anyhow::anyhow!("failed to export audit bundle: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_audit_bundle::{AUDIT_JSONL, MANIFEST_JSON, compute_content_hash};
    use ferrum_proto::{AuditAction, AuditResourceType};

    fn dummy_entry(
        id: i64,
        actor_id: &str,
        action: AuditAction,
        resource_id: &str,
    ) -> ferrum_proto::AuditLogEntry {
        ferrum_proto::AuditLogEntry {
            id,
            actor_id: actor_id.to_string(),
            action,
            resource_type: AuditResourceType::Token,
            resource_id: resource_id.to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: chrono::Utc::now(),
            content_hash: None,
            previous_hash: None,
        }
    }

    fn compute_hashes(entries: &mut [ferrum_proto::AuditLogEntry]) {
        let mut prev: Option<String> = None;
        for entry in entries {
            let hash = compute_content_hash(entry);
            entry.content_hash = Some(hash.clone());
            entry.previous_hash = prev.clone();
            prev = Some(hash);
        }
    }

    #[test]
    fn test_export_and_verify_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
            dummy_entry(2, "bob", AuditAction::TokenRevoke, "t2"),
        ];
        compute_hashes(&mut entries);
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let manifest = export_bundle(tmp.path(), &body).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.entry_count, 2);
        let verified = verify_bundle(tmp.path()).unwrap();
        assert_eq!(verified.merkle_root, manifest.merkle_root);
    }

    #[test]
    fn test_verify_bundle_tampered_content() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![dummy_entry(1, "alice", AuditAction::TokenCreate, "t1")];
        compute_hashes(&mut entries);
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        export_bundle(tmp.path(), &body).unwrap();
        let jsonl_path = tmp.path().join(AUDIT_JSONL);
        let tampered = fs::read_to_string(&jsonl_path)
            .unwrap()
            .replace("alice", "mallory");
        fs::write(&jsonl_path, tampered).unwrap();
        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("tampered content"),
            "expected tamper error, got: {}",
            msg
        );
    }

    #[test]
    fn test_verify_bundle_broken_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
            dummy_entry(2, "bob", AuditAction::TokenRevoke, "t2"),
        ];
        compute_hashes(&mut entries);
        entries[1].previous_hash = Some("badhash".to_string());
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        export_bundle(tmp.path(), &body).unwrap();
        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("broken chain"),
            "expected broken chain error, got: {}",
            msg
        );
    }

    #[test]
    fn test_verify_bundle_duplicate_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
        ];
        compute_hashes(&mut entries);
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        export_bundle(tmp.path(), &body).unwrap();
        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("duplicate"),
            "expected duplicate error, got: {}",
            msg
        );
    }

    #[test]
    fn test_verify_bundle_merkle_root_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![dummy_entry(1, "alice", AuditAction::TokenCreate, "t1")];
        compute_hashes(&mut entries);
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        export_bundle(tmp.path(), &body).unwrap();
        let manifest_path = tmp.path().join(MANIFEST_JSON);
        let mut manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["merkle_root"] = serde_json::Value::String("deadbeef".to_string());
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("merkle root mismatch"),
            "expected merkle root mismatch, got: {}",
            msg
        );
    }

    #[test]
    fn test_verify_bundle_entry_count_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entries = vec![dummy_entry(1, "alice", AuditAction::TokenCreate, "t1")];
        compute_hashes(&mut entries);
        let body = entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        export_bundle(tmp.path(), &body).unwrap();
        let jsonl_path = tmp.path().join(AUDIT_JSONL);
        fs::write(&jsonl_path, "\n").unwrap();
        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("entry count mismatch"),
            "expected count mismatch error, got: {}",
            msg
        );
    }
}
