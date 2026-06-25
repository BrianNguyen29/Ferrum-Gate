use anyhow::{Context, Result, bail};
use ferrum_proto::AuditLogEntry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const AUDIT_JSONL: &str = "audit.jsonl";
const MANIFEST_JSON: &str = "manifest.json";

/// Manifest for a portable audit bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditBundleManifest {
    pub version: u32,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub first_hash: String,
    pub last_hash: String,
    pub merkle_root: String,
    pub entry_count: usize,
}

/// Compute the deterministic SHA-256 content hash for an audit log entry.
///
/// Mirrors the server-side logic in `crates/ferrum-store/src/sqlite/audit_log.rs`.
fn compute_content_hash(entry: &AuditLogEntry) -> String {
    let canonical = serde_json::json!({
        "actor_id": entry.actor_id,
        "action": entry.action.to_string(),
        "resource_type": entry.resource_type.to_string(),
        "resource_id": entry.resource_id,
        "result": entry.result,
        "metadata": entry.metadata,
        "created_at": entry.created_at.to_rfc3339(),
    });
    let bytes = serde_json::to_vec(&canonical).expect("canonical serialization");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
}

/// Compute a deterministic Merkle root over a list of hex-encoded content hashes.
///
/// Mirrors the server-side logic in `crates/ferrum-store/src/merkle.rs`.
fn compute_merkle_root(hashes: &[String]) -> String {
    if hashes.is_empty() {
        return String::new();
    }

    let mut level: Vec<Vec<u8>> = hashes
        .iter()
        .map(|h| {
            let bytes = hex::decode(h).expect("valid hex content_hash");
            let mut hasher = Sha256::new();
            hasher.update([0x00]);
            hasher.update(&bytes);
            hasher.finalize().to_vec()
        })
        .collect();

    while level.len() > 1 {
        let mut next_level = Vec::new();
        let mut i = 0;
        while i < level.len() {
            let left = &level[i];
            let right = if i + 1 < level.len() {
                &level[i + 1]
            } else {
                left
            };
            let mut hasher = Sha256::new();
            hasher.update([0x01]);
            hasher.update(left);
            hasher.update(right);
            next_level.push(hasher.finalize().to_vec());
            i += 2;
        }
        level = next_level;
    }

    hex::encode(&level[0])
}

/// Export a bundle to a directory from an NDJSON body.
///
/// `dir` must exist or will be created.
/// `body` is the NDJSON response from the server export endpoint.
pub fn export_bundle(dir: &Path, body: &str) -> Result<AuditBundleManifest> {
    if !dir.exists() {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create bundle directory {}", dir.display()))?;
    }

    let jsonl_path = dir.join(AUDIT_JSONL);
    let manifest_path = dir.join(MANIFEST_JSON);

    // Parse entries to compute the manifest.
    let mut entries = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: AuditLogEntry = serde_json::from_str(line)
            .with_context(|| format!("failed to parse audit log entry: {}", line))?;
        entries.push(entry);
    }

    // Write `audit.jsonl` verbatim.
    fs::write(&jsonl_path, body)
        .with_context(|| format!("failed to write {}", jsonl_path.display()))?;

    // Compute manifest fields.
    let hashed_entries: Vec<&AuditLogEntry> = entries
        .iter()
        .filter(|e| e.content_hash.is_some())
        .collect();
    let first_hash = hashed_entries
        .first()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    let last_hash = hashed_entries
        .last()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    let hashes: Vec<String> = hashed_entries
        .iter()
        .map(|e| e.content_hash.clone().unwrap())
        .collect();
    let merkle_root = compute_merkle_root(&hashes);
    let entry_count = entries.len();

    let manifest = AuditBundleManifest {
        version: 1,
        exported_at: chrono::Utc::now(),
        first_hash,
        last_hash,
        merkle_root,
        entry_count,
    };

    let manifest_json =
        serde_json::to_string_pretty(&manifest).with_context(|| "failed to serialize manifest")?;
    fs::write(&manifest_path, manifest_json)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    Ok(manifest)
}

/// Verify a bundle directory.
///
/// Returns the manifest if valid, or an error describing the failure.
pub fn verify_bundle(dir: &Path) -> Result<AuditBundleManifest> {
    let manifest_path = dir.join(MANIFEST_JSON);
    let jsonl_path = dir.join(AUDIT_JSONL);

    if !manifest_path.exists() {
        bail!("bundle manifest not found: {}", manifest_path.display());
    }
    if !jsonl_path.exists() {
        bail!("bundle audit log not found: {}", jsonl_path.display());
    }

    let manifest: AuditBundleManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    if manifest.version != 1 {
        bail!("unsupported bundle version: {}", manifest.version);
    }

    let body = fs::read_to_string(&jsonl_path)
        .with_context(|| format!("failed to read {}", jsonl_path.display()))?;

    let mut entries = Vec::new();
    let mut seen_ids = HashSet::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: AuditLogEntry = serde_json::from_str(line)
            .with_context(|| format!("failed to parse audit log entry in bundle: {}", line))?;
        if !seen_ids.insert(entry.id) {
            bail!("duplicate audit log entry id: {}", entry.id);
        }
        entries.push(entry);
    }

    if entries.len() != manifest.entry_count {
        bail!(
            "entry count mismatch: manifest says {} but found {} entries",
            manifest.entry_count,
            entries.len()
        );
    }

    // Verify hash chain continuity and content integrity.
    let mut prior_content_hash: Option<String> = None;
    let mut hashed_entries = Vec::new();
    for entry in &entries {
        if entry.content_hash.is_none() {
            continue;
        }
        let stored_hash = entry.content_hash.clone().unwrap();
        let recomputed = compute_content_hash(entry);
        if stored_hash != recomputed {
            bail!(
                "audit log entry {} has tampered content: stored content_hash '{}' != recomputed '{}'",
                entry.id,
                stored_hash,
                recomputed
            );
        }

        if let Some(ref prior) = prior_content_hash {
            let prev = entry.previous_hash.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "audit log entry {} has content_hash but missing previous_hash",
                    entry.id
                )
            })?;
            if prev != prior {
                bail!(
                    "audit log entry {} has broken chain: previous_hash '{}' != prior content_hash '{}'",
                    entry.id,
                    prev,
                    prior
                );
            }
        } else if entry.previous_hash.is_some() {
            bail!(
                "audit log entry {} is the first hashed entry but has previous_hash",
                entry.id
            );
        }

        prior_content_hash = Some(stored_hash);
        hashed_entries.push(entry);
    }

    // Verify first and last hash bookends.
    let first_hash = hashed_entries
        .first()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    let last_hash = hashed_entries
        .last()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    if first_hash != manifest.first_hash {
        bail!(
            "first hash mismatch: manifest '{}' != computed '{}'",
            manifest.first_hash,
            first_hash
        );
    }
    if last_hash != manifest.last_hash {
        bail!(
            "last hash mismatch: manifest '{}' != computed '{}'",
            manifest.last_hash,
            last_hash
        );
    }

    // Verify Merkle root.
    let hashes: Vec<String> = hashed_entries
        .iter()
        .map(|e| e.content_hash.clone().unwrap())
        .collect();
    let merkle_root = compute_merkle_root(&hashes);
    if merkle_root != manifest.merkle_root {
        bail!(
            "merkle root mismatch: manifest '{}' != computed '{}'",
            manifest.merkle_root,
            merkle_root
        );
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{AuditAction, AuditResourceType};

    fn dummy_entry(
        id: i64,
        actor_id: &str,
        action: AuditAction,
        resource_id: &str,
    ) -> AuditLogEntry {
        AuditLogEntry {
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

    fn compute_hashes(entries: &mut [AuditLogEntry]) {
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
        // Tamper the exported JSONL.
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
        // Tamper the manifest Merkle root.
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
        // Truncate the JSONL to create a count mismatch.
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
