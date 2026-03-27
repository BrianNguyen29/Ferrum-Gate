//! Proof structure verification for Sync-3a diagnostic probe.
//!
//! This module performs STRUCTURE-ONLY verification of proofs. Full cryptographic
//! verification (matching range_hash against leader_tip) requires the apply-phase
//! anchor and is deferred to the write-path slice.
//!
//! ## Proof Structure Requirements
//!
//! A valid proof structure must satisfy:
//! 1. `entries` is non-empty
//! 2. `entries` are in strictly increasing sequence order
//! 3. `range_hash` is non-empty
//! 4. `continuity_proof.nodes` is non-empty
//! 5. `continuity_proof.leaf_count >= entries.len()`

use crate::error::ProbeError;
use crate::transport::Proof;

/// Verifies the structure of a proof without performing cryptographic validation.
///
/// This is a read-only diagnostic check: it only validates the shape of the proof,
/// not whether the hashes are correct. Full verification requires the apply-phase
/// anchor (leader_tip as trusted root).
///
/// Returns `Ok(())` if the structure is valid, or a `ProbeError` describing the
/// structural violation.
pub fn verify_proof_structure(proof: &Proof) -> Result<(), ProbeError> {
    // Check 1: entries must be non-empty
    if proof.entries.is_empty() {
        return Err(ProbeError::ProofStructureInvalid {
            reason: "entries is empty".to_string(),
        });
    }

    // Check 2: entries must be in strictly increasing sequence order
    let mut prev_seq: Option<u64> = None;
    for entry in &proof.entries {
        if let Some(prev) = prev_seq {
            if entry.sequence <= prev {
                return Err(ProbeError::ProofStructureInvalid {
                    reason: format!(
                        "entries not strictly increasing: seq {} <= previous {}",
                        entry.sequence, prev
                    ),
                });
            }
        }
        prev_seq = Some(entry.sequence);
    }

    // Check 3: range_hash must be non-empty
    if proof.range_hash.is_empty() {
        return Err(ProbeError::ProofStructureInvalid {
            reason: "range_hash is empty".to_string(),
        });
    }

    // Check 4: continuity_proof.nodes must be non-empty
    if proof.continuity_proof.nodes.is_empty() {
        return Err(ProbeError::ProofStructureInvalid {
            reason: "continuity_proof.nodes is empty".to_string(),
        });
    }

    // Check 5: continuity_proof.leaf_count must cover entries
    if proof.continuity_proof.leaf_count < proof.entries.len() as u64 {
        return Err(ProbeError::ProofStructureInvalid {
            reason: format!(
                "continuity_proof.leaf_count ({}) < entries.len() ({})",
                proof.continuity_proof.leaf_count,
                proof.entries.len()
            ),
        });
    }

    Ok(())
}

/// Verifies entry hash continuity: each entry's hash should match the
/// next entry's prev_hash (if we had it). Since we only have hashes,
/// we just verify the hash is non-empty.
///
/// Note: Full prev_hash verification requires the apply-phase anchor
/// and is deferred to the write-path slice.
pub fn verify_entry_hashes(proof: &Proof) -> Result<(), ProbeError> {
    for entry in &proof.entries {
        if entry.entry_hash.is_empty() {
            return Err(ProbeError::ProofStructureInvalid {
                reason: format!("entry at sequence {} has empty hash", entry.sequence),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{EntryHashInfo, HashPath};

    fn make_proof(
        entries: Vec<(u64, &str)>,
        range_hash: &str,
        nodes: Vec<&str>,
        leaf_count: u64,
    ) -> Proof {
        Proof {
            entries: entries
                .into_iter()
                .map(|(seq, hash)| EntryHashInfo {
                    sequence: seq,
                    entry_hash: hash.to_string(),
                })
                .collect(),
            range_hash: range_hash.to_string(),
            continuity_proof: HashPath {
                nodes: nodes.into_iter().map(|s| s.to_string()).collect(),
                leaf_count,
            },
        }
    }

    #[test]
    fn valid_proof_structure_passes() {
        let proof = make_proof(
            vec![(1, "hash1"), (2, "hash2"), (3, "hash3")],
            "range_hash_value",
            vec!["node1", "node2"],
            10,
        );

        assert!(verify_proof_structure(&proof).is_ok());
    }

    #[test]
    fn empty_entries_fails() {
        let proof = make_proof(vec![], "range_hash", vec!["node1"], 10);

        let result = verify_proof_structure(&proof);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ProbeError::ProofStructureInvalid { reason } => {
                assert!(reason.contains("entries is empty"));
            }
            _ => panic!("expected ProofStructureInvalid"),
        }
    }

    #[test]
    fn non_strictly_increasing_sequence_fails() {
        // Sequence 2 is not greater than sequence 1
        let proof = make_proof(
            vec![(1, "hash1"), (2, "hash2"), (2, "hash3")], // duplicate seq
            "range_hash",
            vec!["node1", "node2"],
            10,
        );

        let result = verify_proof_structure(&proof);
        assert!(result.is_err());
    }

    #[test]
    fn empty_range_hash_fails() {
        let proof = make_proof(
            vec![(1, "hash1")],
            "", // empty range hash
            vec!["node1"],
            10,
        );

        let result = verify_proof_structure(&proof);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ProbeError::ProofStructureInvalid { reason } => {
                assert!(reason.contains("range_hash is empty"));
            }
            _ => panic!("expected ProofStructureInvalid"),
        }
    }

    #[test]
    fn empty_continuity_proof_nodes_fails() {
        let proof = make_proof(vec![(1, "hash1")], "range_hash", vec![], 10);

        let result = verify_proof_structure(&proof);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ProbeError::ProofStructureInvalid { reason } => {
                assert!(reason.contains("continuity_proof.nodes is empty"));
            }
            _ => panic!("expected ProofStructureInvalid"),
        }
    }

    #[test]
    fn insufficient_leaf_count_fails() {
        // leaf_count=2 but we have 3 entries
        let proof = make_proof(
            vec![(1, "hash1"), (2, "hash2"), (3, "hash3")],
            "range_hash",
            vec!["node1", "node2"],
            2,
        );

        let result = verify_proof_structure(&proof);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            ProbeError::ProofStructureInvalid { reason } => {
                assert!(reason.contains("leaf_count"));
            }
            _ => panic!("expected ProofStructureInvalid"),
        }
    }

    #[test]
    fn exactly_sufficient_leaf_count_passes() {
        // leaf_count=3 and we have 3 entries - this is exactly sufficient
        let proof = make_proof(
            vec![(1, "hash1"), (2, "hash2"), (3, "hash3")],
            "range_hash",
            vec!["node1", "node2"],
            3,
        );

        assert!(verify_proof_structure(&proof).is_ok());
    }

    #[test]
    fn verify_entry_hashes_rejects_empty_hash() {
        let proof = make_proof(vec![(1, ""), (2, "hash2")], "range_hash", vec!["node1"], 10);

        let result = verify_entry_hashes(&proof);
        assert!(result.is_err());
    }

    #[test]
    fn verify_entry_hashes_accepts_valid_hashes() {
        let proof = make_proof(
            vec![(1, "hash1"), (2, "hash2")],
            "range_hash",
            vec!["node1"],
            10,
        );

        assert!(verify_entry_hashes(&proof).is_ok());
    }
}
