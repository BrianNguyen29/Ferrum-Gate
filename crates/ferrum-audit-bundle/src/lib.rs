//! Portable, tamper-evident audit bundle format.
//!
//! This crate exposes the canonical bundle builder and verifier used by both
//! `ferrumctl audit export/verify` and the gateway's optional WORM sink. The
//! format is intentionally simple and self-contained:
//!
//! - `audit.jsonl` — one JSON Lines record per audit log entry, in chronological
//!   order, preserving the original `content_hash` and `previous_hash` fields.
//! - `manifest.json` — metadata including bundle version, export timestamp, first
//!   and last content hash, entry count, and a Merkle root over the chain of
//!   content hashes.
//!
//! The bundle is **not** encrypted by default; operators may encrypt at rest via
//! filesystem or sink-level encryption.
//!
//! Verification re-computes the SHA-256 content hash for each entry, checks the
//! hash chain continuity, and verifies the Merkle root. It does not make any
//! compliance, WORM-certified, or tamper-proof claims.

use chrono::Utc;
use ferrum_proto::AuditLogEntry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const AUDIT_JSONL: &str = "audit.jsonl";
pub const MANIFEST_JSON: &str = "manifest.json";

/// Errors that can occur while building, exporting, or verifying a bundle.
#[derive(Debug, thiserror::Error)]
pub enum AuditBundleError {
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid bundle: {0}")]
    Invalid(String),
}

/// Manifest for a portable audit bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditBundleManifest {
    pub version: u32,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub first_hash: String,
    pub last_hash: String,
    pub merkle_root: String,
    pub entry_count: usize,
    /// Hash of the last entry in the previous bundle, if this bundle is a
    /// windowed batch that continues a chain. `None` means this bundle is a
    /// standalone or full export: its first hashed entry must have a NULL
    /// `previous_hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_boundary_hash: Option<String>,
}

/// A built bundle: the NDJSON body and its manifest.
#[derive(Debug, Clone)]
pub struct AuditBundle {
    pub body: String,
    pub manifest: AuditBundleManifest,
}

/// Compute the deterministic SHA-256 content hash for an audit log entry.
///
/// Mirrors the server-side logic in `crates/ferrum-store/src/sqlite/audit_log.rs`
/// and `crates/ferrum-store/src/postgres/audit_log.rs`. The hash covers the
/// canonical fields and excludes `id`, `content_hash`, and `previous_hash` to
/// avoid circularity.
pub fn compute_content_hash(entry: &AuditLogEntry) -> String {
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
/// Mirrors the server-side logic in `crates/ferrum-store/src/merkle.rs`. An empty
/// hash list returns an empty root.
pub fn compute_merkle_root(hashes: &[String]) -> String {
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

/// Build a bundle from an NDJSON body of audit log entries.
///
/// The body is expected to contain one JSON-serialized `AuditLogEntry` per line.
/// This is the path used by `ferrumctl`, which receives the raw NDJSON export
/// from the server.
pub fn build_bundle_from_ndjson(body: &str) -> Result<AuditBundle, AuditBundleError> {
    let mut entries = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: AuditLogEntry = serde_json::from_str(line).map_err(|e| {
            AuditBundleError::Invalid(format!("failed to parse audit log entry: {e}"))
        })?;
        entries.push(entry);
    }
    build_bundle(&entries, body)
}

/// Build a bundle from a slice of audit log entries and the original NDJSON body.
///
/// The caller is responsible for supplying the body that matches the entries; the
/// manifest is computed from the entries themselves. This function produces a
/// full-bundle manifest: the first hashed entry must have a NULL `previous_hash`.
/// For windowed batches that continue from a previous bundle, use
/// `build_bundle_with_boundary`.
pub fn build_bundle(
    entries: &[AuditLogEntry],
    body: &str,
) -> Result<AuditBundle, AuditBundleError> {
    build_bundle_with_boundary(entries, body, None)
}

/// Build a bundle from a slice of audit log entries, the original NDJSON body,
/// and an optional boundary hash.
///
/// When `previous_boundary_hash` is `Some`, the manifest records it and the
/// verifier will accept the first hashed entry having a `previous_hash` equal to
/// that boundary, allowing windowed batches to be verified independently while
/// still checking chain continuity. When `None`, the bundle is treated as a
/// full export and the first hashed entry must have a NULL `previous_hash`.
pub fn build_bundle_with_boundary(
    entries: &[AuditLogEntry],
    body: &str,
    previous_boundary_hash: Option<String>,
) -> Result<AuditBundle, AuditBundleError> {
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
        exported_at: Utc::now(),
        first_hash,
        last_hash,
        merkle_root,
        entry_count,
        previous_boundary_hash,
    };

    Ok(AuditBundle {
        body: body.to_string(),
        manifest,
    })
}

/// Export a built bundle to a directory.
///
/// `dir` must exist or will be created. Writes `audit.jsonl` and `manifest.json`.
pub fn export_bundle(
    dir: &Path,
    bundle: &AuditBundle,
) -> Result<AuditBundleManifest, AuditBundleError> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }

    let jsonl_path = dir.join(AUDIT_JSONL);
    let manifest_path = dir.join(MANIFEST_JSON);

    fs::write(&jsonl_path, &bundle.body)?;

    let manifest_json = serde_json::to_string_pretty(&bundle.manifest)?;
    fs::write(&manifest_path, manifest_json)?;

    Ok(bundle.manifest.clone())
}

/// Verify a bundle directory.
///
/// Returns the manifest if valid, or an error describing the failure. Verification
/// checks entry count, hash chain continuity, content integrity, and the Merkle
/// root. It does not make WORM or compliance claims.
pub fn verify_bundle(dir: &Path) -> Result<AuditBundleManifest, AuditBundleError> {
    let manifest_path = dir.join(MANIFEST_JSON);
    let jsonl_path = dir.join(AUDIT_JSONL);

    if !manifest_path.exists() {
        return Err(AuditBundleError::Invalid(format!(
            "bundle manifest not found: {}",
            manifest_path.display()
        )));
    }
    if !jsonl_path.exists() {
        return Err(AuditBundleError::Invalid(format!(
            "bundle audit log not found: {}",
            jsonl_path.display()
        )));
    }

    let manifest: AuditBundleManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| AuditBundleError::Invalid(format!("failed to read manifest: {e}")))?,
    )
    .map_err(|e| AuditBundleError::Invalid(format!("failed to parse manifest: {e}")))?;

    if manifest.version != 1 {
        return Err(AuditBundleError::Invalid(format!(
            "unsupported bundle version: {}",
            manifest.version
        )));
    }

    let body = fs::read_to_string(&jsonl_path)?;

    let mut entries = Vec::new();
    let mut seen_ids = HashSet::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: AuditLogEntry = serde_json::from_str(line).map_err(|e| {
            AuditBundleError::Invalid(format!("failed to parse audit log entry in bundle: {e}"))
        })?;
        if !seen_ids.insert(entry.id) {
            return Err(AuditBundleError::Invalid(format!(
                "duplicate audit log entry id: {}",
                entry.id
            )));
        }
        entries.push(entry);
    }

    if entries.len() != manifest.entry_count {
        return Err(AuditBundleError::Invalid(format!(
            "entry count mismatch: manifest says {} but found {} entries",
            manifest.entry_count,
            entries.len()
        )));
    }

    let mut prior_content_hash: Option<String> = None;
    let mut hashed_entries = Vec::new();
    for entry in &entries {
        if entry.content_hash.is_none() {
            continue;
        }
        let stored_hash = entry.content_hash.clone().unwrap();
        let recomputed = compute_content_hash(entry);
        if stored_hash != recomputed {
            return Err(AuditBundleError::Invalid(format!(
                "audit log entry {} has tampered content: stored content_hash '{}' != recomputed '{}'",
                entry.id, stored_hash, recomputed
            )));
        }

        if let Some(ref prior) = prior_content_hash {
            let prev = entry.previous_hash.as_deref().ok_or_else(|| {
                AuditBundleError::Invalid(format!(
                    "audit log entry {} has content_hash but missing previous_hash",
                    entry.id
                ))
            })?;
            if prev != prior {
                return Err(AuditBundleError::Invalid(format!(
                    "audit log entry {} has broken chain: previous_hash '{}' != prior content_hash '{}'",
                    entry.id, prev, prior
                )));
            }
        } else if let Some(ref boundary) = manifest.previous_boundary_hash {
            // Windowed bundle: the first hashed entry may continue from a prior
            // bundle; its previous_hash must match the declared boundary.
            let prev = entry.previous_hash.as_deref().ok_or_else(|| {
                AuditBundleError::Invalid(format!(
                    "audit log entry {} is the first hashed entry of a windowed bundle but has no previous_hash; expected boundary '{}'",
                    entry.id, boundary
                ))
            })?;
            if prev != boundary {
                return Err(AuditBundleError::Invalid(format!(
                    "audit log entry {} has broken boundary: previous_hash '{}' != manifest boundary '{}'",
                    entry.id, prev, boundary
                )));
            }
        } else if entry.previous_hash.is_some() {
            return Err(AuditBundleError::Invalid(format!(
                "audit log entry {} is the first hashed entry but has previous_hash",
                entry.id
            )));
        }

        prior_content_hash = Some(stored_hash);
        hashed_entries.push(entry);
    }

    let first_hash = hashed_entries
        .first()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    let last_hash = hashed_entries
        .last()
        .map(|e| e.content_hash.clone().unwrap())
        .unwrap_or_default();
    if first_hash != manifest.first_hash {
        return Err(AuditBundleError::Invalid(format!(
            "first hash mismatch: manifest '{}' != computed '{}'",
            manifest.first_hash, first_hash
        )));
    }
    if last_hash != manifest.last_hash {
        return Err(AuditBundleError::Invalid(format!(
            "last hash mismatch: manifest '{}' != computed '{}'",
            manifest.last_hash, last_hash
        )));
    }

    let hashes: Vec<String> = hashed_entries
        .iter()
        .map(|e| e.content_hash.clone().unwrap())
        .collect();
    let merkle_root = compute_merkle_root(&hashes);
    if merkle_root != manifest.merkle_root {
        return Err(AuditBundleError::Invalid(format!(
            "merkle root mismatch: manifest '{}' != computed '{}'",
            manifest.merkle_root, merkle_root
        )));
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
        let bundle = build_bundle_from_ndjson(&body).unwrap();
        assert_eq!(bundle.manifest.version, 1);
        assert_eq!(bundle.manifest.entry_count, 2);

        export_bundle(tmp.path(), &bundle).unwrap();
        let verified = verify_bundle(tmp.path()).unwrap();
        assert_eq!(verified.merkle_root, bundle.manifest.merkle_root);
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
        export_bundle(tmp.path(), &build_bundle_from_ndjson(&body).unwrap()).unwrap();

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
        export_bundle(tmp.path(), &build_bundle_from_ndjson(&body).unwrap()).unwrap();

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
        export_bundle(tmp.path(), &build_bundle_from_ndjson(&body).unwrap()).unwrap();

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
        export_bundle(tmp.path(), &build_bundle_from_ndjson(&body).unwrap()).unwrap();

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
        export_bundle(tmp.path(), &build_bundle_from_ndjson(&body).unwrap()).unwrap();

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

    #[test]
    fn test_verify_windowed_bundle() {
        let mut all_entries = vec![
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
            dummy_entry(2, "bob", AuditAction::TokenRevoke, "t2"),
            dummy_entry(3, "carol", AuditAction::TokenCreate, "t3"),
            dummy_entry(4, "dave", AuditAction::TokenRevoke, "t4"),
        ];
        compute_hashes(&mut all_entries);

        // First batch: full bundle (no boundary).
        let first_batch = &all_entries[0..2];
        let first_body = first_batch
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let first_tmp = tempfile::tempdir().unwrap();
        export_bundle(
            first_tmp.path(),
            &build_bundle(first_batch, &first_body).unwrap(),
        )
        .unwrap();
        let first_manifest = verify_bundle(first_tmp.path()).unwrap();
        assert!(first_manifest.previous_boundary_hash.is_none());

        // Second batch: windowed bundle continuing from the first batch's last hash.
        let boundary = first_manifest.last_hash;
        let second_batch = &all_entries[2..4];
        let second_body = second_batch
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let second_tmp = tempfile::tempdir().unwrap();
        let second_bundle =
            build_bundle_with_boundary(second_batch, &second_body, Some(boundary.clone())).unwrap();
        assert_eq!(
            second_bundle.manifest.previous_boundary_hash,
            Some(boundary)
        );
        export_bundle(second_tmp.path(), &second_bundle).unwrap();
        let second_manifest = verify_bundle(second_tmp.path()).unwrap();
        assert_eq!(
            second_manifest.previous_boundary_hash,
            Some(all_entries[1].content_hash.clone().unwrap())
        );
    }

    #[test]
    fn test_verify_windowed_bundle_boundary_mismatch() {
        let mut all_entries = vec![
            dummy_entry(1, "alice", AuditAction::TokenCreate, "t1"),
            dummy_entry(2, "bob", AuditAction::TokenRevoke, "t2"),
            dummy_entry(3, "carol", AuditAction::TokenCreate, "t3"),
        ];
        compute_hashes(&mut all_entries);

        let second_batch = &all_entries[2..3];
        let second_body = second_batch
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let tmp = tempfile::tempdir().unwrap();
        let bundle =
            build_bundle_with_boundary(second_batch, &second_body, Some("badboundary".into()))
                .unwrap();
        export_bundle(tmp.path(), &bundle).unwrap();

        let err = verify_bundle(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("broken boundary"),
            "expected broken boundary error, got: {}",
            msg
        );
    }
}
