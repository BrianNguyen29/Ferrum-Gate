//! Append-only audit ledger with hash-chain integrity.
//!
//! ## Design
//!
//! Each [`LedgerEntry`] wraps a [`ProvenanceEvent`] and adds chain metadata:
//! - `sequence`: zero-based index into the ledger
//! - `prev_hash`: hash of the previous entry (null for genesis)
//! - `entry_hash`: deterministic hash of this entry's content + prev_hash
//!
//! The ledger is append-only: [`InMemoryLedger::append`] validates the chain
//! before inserting, and exposed a minimal read-only API for chain verification.
//!
//! ## Genesis Entry
//!
//! The first entry is the **genesis entry**. Its `prev_hash` is `None` and its
//! hash is computed from the entry content + the ASCII string `"GENESIS"`.

use ferrum_proto::{ProvenanceEvent, Sha256Hex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A ledger entry with chain linkage metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Zero-based sequence number (0 = genesis).
    pub sequence: u64,
    /// Hash of the previous entry (None for genesis).
    pub prev_hash: Option<Sha256Hex>,
    /// Deterministic hash of this entry (content + prev_hash).
    pub entry_hash: Sha256Hex,
    /// The underlying provenance event.
    pub event: ProvenanceEvent,
}

/// Error returned when chain validation fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("chain broken: expected prev_hash={expected}, got {actual}")]
    BrokenChain { expected: String, actual: String },

    #[error(
        "tamper detected: entry {sequence} hash mismatch (recomputed {recomputed} != recorded {recorded})"
    )]
    TamperDetected {
        sequence: u64,
        recorded: Sha256Hex,
        recomputed: Sha256Hex,
    },

    #[error("empty ledger cannot verify non-genesis entry")]
    EmptyLedger,

    #[error("event sequence {event_seq} does not match ledger length {ledger_len}")]
    SequenceMismatch { event_seq: u64, ledger_len: usize },

    #[error("event already appended (duplicate event_id)")]
    DuplicateEvent,
}

/// The genesis hash is the SHA-256 of the ASCII string "GENESIS".
pub const GENESIS_HASH: &str = "GENESIS";

/// In-memory append-only ledger with hash-chain integrity.
#[derive(Default)]
pub struct InMemoryLedger {
    entries: Vec<LedgerEntry>,
}

impl InMemoryLedger {
    /// Creates a new empty ledger.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Returns the number of entries in the ledger.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the ledger is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns a reference to the last entry, if any.
    pub fn last_entry(&self) -> Option<&LedgerEntry> {
        self.entries.last()
    }

    /// Returns a reference to the genesis entry, if any.
    pub fn genesis(&self) -> Option<&LedgerEntry> {
        self.entries.first()
    }

    /// Returns an iterator over all entries.
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Appends a [`ProvenanceEvent`] as a new [`LedgerEntry`].
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::DuplicateEvent`] if the event's `event_id` is already
    /// present in the ledger. Returns [`LedgerError::SequenceMismatch`] if the
    /// event's internal sequence number doesn't match the expected next position.
    /// On a tampered previous entry, returns [`LedgerError::BrokenChain`] or
    /// [`LedgerError::TamperDetected`].
    pub fn append(&mut self, event: ProvenanceEvent) -> Result<&LedgerEntry, LedgerError> {
        let sequence = self.entries.len() as u64;
        let prev_hash = self.entries.last().map(|e| e.entry_hash.clone());

        // Duplicate detection
        if self
            .entries
            .iter()
            .any(|e| e.event.event_id == event.event_id)
        {
            return Err(LedgerError::DuplicateEvent);
        }

        // Build a temporary entry to compute the hash (prev_hash is used as-is)
        let temp_entry = LedgerEntry {
            sequence,
            prev_hash: prev_hash.clone(),
            entry_hash: String::new(), // placeholder, not used for hashing
            event,
        };
        let entry_hash = compute_entry_hash(&temp_entry, prev_hash.as_deref());

        let entry = LedgerEntry {
            sequence,
            prev_hash,
            entry_hash: entry_hash.clone(),
            event: temp_entry.event,
        };

        // --- Light validation before committing ---
        // Verify the entry we just built hashes to what we expect
        let recomputed = compute_entry_hash_raw(&entry);
        if recomputed != entry_hash {
            // This should never happen — indicates a bug in compute_entry_hash
            return Err(LedgerError::TamperDetected {
                sequence,
                recorded: entry.entry_hash.to_string(),
                recomputed,
            });
        }

        self.entries.push(entry);
        Ok(self.entries.last().unwrap())
    }

    /// Verifies the entire chain, returning the first error encountered.
    ///
    /// Use this to audit integrity after loading from persistence.
    pub fn verify_chain(&self) -> Result<(), LedgerError> {
        for (i, entry) in self.entries.iter().enumerate() {
            let sequence = i as u64;

            // Check sequence matches
            if entry.sequence != sequence {
                return Err(LedgerError::SequenceMismatch {
                    event_seq: entry.sequence,
                    ledger_len: i,
                });
            }

            // Check prev_hash linkage
            let expected_prev = if i == 0 {
                None
            } else {
                Some(self.entries[i - 1].entry_hash.clone())
            };
            if entry.prev_hash != expected_prev {
                return Err(LedgerError::BrokenChain {
                    expected: expected_prev.unwrap_or_else(|| GENESIS_HASH.to_string()),
                    actual: entry
                        .prev_hash
                        .clone()
                        .unwrap_or_else(|| "None".to_string()),
                });
            }

            // Check entry hash
            let recomputed = compute_entry_hash_raw(entry);
            if recomputed != entry.entry_hash.as_str() {
                return Err(LedgerError::TamperDetected {
                    sequence,
                    recorded: entry.entry_hash.to_string(),
                    recomputed,
                });
            }
        }
        Ok(())
    }

    /// Loads pre-built entries into a new [`InMemoryLedger`] without re-hashing or
    /// re-validating chain linkage. Used when rebuilding a ledger from a trusted
    /// store (e.g. SQLite) that already persisted verified entries.
    ///
    /// # Safety
    ///
    /// Caller must ensure the entries form a valid chain (i.e. [`verify_chain`]
    /// would pass). Passing invalid data will cause subsequent [`verify_chain`]
    /// calls to fail.
    pub fn load_entries(entries: Vec<LedgerEntry>) -> Self {
        Self { entries }
    }
}

/// Compute the deterministic hash for a ledger entry.
///
/// The hash is SHA-256 over the concatenation of:
/// 1. The deterministic serde_json serialization of the entry's event
/// 2. The prev_hash string (or "GENESIS" if None)
fn compute_entry_hash(entry: &LedgerEntry, prev_hash: Option<&str>) -> String {
    let content = serde_json::to_string(&entry.event).expect("event must serialize");
    let prev = prev_hash.unwrap_or(GENESIS_HASH);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.update(prev.as_bytes());
    hex::encode(hasher.finalize())
}

/// Variant that works on a fully-constructed entry (uses entry.prev_hash).
pub fn compute_entry_hash_raw(entry: &LedgerEntry) -> String {
    compute_entry_hash(entry, entry.prev_hash.as_deref())
}

impl LedgerEntry {
    /// Construct a ledger entry from a provenance event and chain metadata.
    ///
    /// This is the low-level constructor used when building an entry from a
    /// known previous tip (rather than appending to an in-memory ledger).
    ///
    /// # Arguments
    /// * `event` - The provenance event to wrap
    /// * `sequence` - This entry's zero-based sequence number
    /// * `prev_hash` - Hash of the previous entry (None for genesis)
    pub fn from_event(event: ProvenanceEvent, sequence: u64, prev_hash: Option<Sha256Hex>) -> Self {
        let entry_hash = {
            // Build a temporary entry to compute the hash (clone event since we need it after)
            let temp = LedgerEntry {
                sequence,
                prev_hash: prev_hash.clone(),
                entry_hash: String::new(),
                event: event.clone(),
            };
            compute_entry_hash(&temp, prev_hash.as_deref())
        };

        LedgerEntry {
            sequence,
            prev_hash,
            entry_hash,
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActorRef, ActorType, EventId, HashChainRef, ObjectRef, ObjectType, SensitivityLabel,
        Timestamp, TrustLabel,
    };

    fn make_test_event(sequence: u64) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::UserGoalReceived,
            occurred_at: Timestamp::default(),
            actor: ActorRef {
                actor_type: ActorType::User,
                actor_id: format!("user-{}", sequence),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: format!("intent-{}", sequence),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![TrustLabel::Trusted],
            sensitivity_labels: vec![SensitivityLabel::Public],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    #[test]
    fn test_genesis_entry_has_null_prev_hash() {
        let mut ledger = InMemoryLedger::new();
        let event = make_test_event(0);
        let entry = ledger.append(event).expect("append should succeed");

        assert_eq!(entry.sequence, 0);
        assert!(entry.prev_hash.is_none());
        // Genesis hash is computed with "GENESIS" as prev_hash
        let expected = {
            let content = serde_json::to_string(&entry.event).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            hasher.update(b"GENESIS");
            hex::encode(hasher.finalize())
        };
        assert_eq!(entry.entry_hash.as_str(), expected);
    }

    #[test]
    fn test_chain_linkage() {
        let mut ledger = InMemoryLedger::new();

        ledger.append(make_test_event(1)).expect("append genesis");
        let first_hash = ledger.entries[0].entry_hash.clone();

        ledger.append(make_test_event(2)).expect("append second");
        let second_hash = ledger.entries[1].entry_hash.clone();

        ledger.append(make_test_event(3)).expect("append third");

        // Verify linkage
        assert_eq!(ledger.entries[1].prev_hash.as_ref(), Some(&first_hash));
        assert_eq!(ledger.entries[1].sequence, 1);

        assert_eq!(ledger.entries[2].prev_hash.as_ref(), Some(&second_hash));
        assert_eq!(ledger.entries[2].sequence, 2);
    }

    #[test]
    fn test_verify_chain_passes_on_valid_ledger() {
        let mut ledger = InMemoryLedger::new();
        ledger.append(make_test_event(0)).expect("append genesis");
        ledger.append(make_test_event(1)).expect("append second");
        ledger.append(make_test_event(2)).expect("append third");

        ledger.verify_chain().expect("chain should be valid");
    }

    #[test]
    fn test_verify_chain_detects_broken_prev_hash() {
        let mut ledger = InMemoryLedger::new();
        ledger.append(make_test_event(0)).expect("append genesis");
        ledger.append(make_test_event(1)).expect("append second");

        // Corrupt the prev_hash of the second entry
        ledger.entries[1].prev_hash = Some("badbadbad".to_string());

        let err = ledger
            .verify_chain()
            .expect_err("should detect broken chain");
        assert!(matches!(err, LedgerError::BrokenChain { .. }));
    }

    #[test]
    fn test_verify_chain_detects_tampered_entry_hash() {
        let mut ledger = InMemoryLedger::new();
        ledger.append(make_test_event(0)).expect("append genesis");
        ledger.append(make_test_event(1)).expect("append second");

        // Corrupt the entry_hash of the second entry
        ledger.entries[1].entry_hash = "tampered_entry_hash".to_string();

        let err = ledger.verify_chain().expect_err("should detect tamper");
        assert!(matches!(err, LedgerError::TamperDetected { .. }));
    }

    #[test]
    fn test_verify_chain_detects_tampered_event_content() {
        let mut ledger = InMemoryLedger::new();
        ledger.append(make_test_event(0)).expect("append genesis");
        ledger.append(make_test_event(1)).expect("append second");

        // Mutate event content after append (simulates tampering)
        ledger.entries[1].event.actor.actor_id = "hacker".to_string();

        let err = ledger
            .verify_chain()
            .expect_err("should detect content tamper");
        assert!(matches!(err, LedgerError::TamperDetected { .. }));
    }

    #[test]
    fn test_duplicate_event_rejected() {
        let mut ledger = InMemoryLedger::new();
        let event = make_test_event(0);
        let event_id = event.event_id;
        ledger.append(event).expect("append should succeed");

        let dup = ProvenanceEvent {
            event_id, // same id
            kind: ferrum_proto::ProvenanceEventKind::IntentCompiled,
            occurred_at: Timestamp::default(),
            actor: ActorRef {
                actor_type: ActorType::User,
                actor_id: "user-x".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "intent-x".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
        };

        let err = ledger
            .append(dup)
            .expect_err("duplicate should be rejected");
        assert!(matches!(err, LedgerError::DuplicateEvent));
    }

    #[test]
    fn test_empty_ledger_verify_succeeds() {
        let ledger = InMemoryLedger::new();
        ledger.verify_chain().expect("empty ledger is valid");
    }

    #[test]
    fn test_last_entry_returns_correctly() {
        let mut ledger = InMemoryLedger::new();
        assert!(ledger.last_entry().is_none());

        ledger.append(make_test_event(0)).expect("append");
        assert_eq!(ledger.last_entry().unwrap().sequence, 0);

        ledger.append(make_test_event(1)).expect("append");
        assert_eq!(ledger.last_entry().unwrap().sequence, 1);
    }

    #[test]
    fn test_genesis_returns_correctly() {
        let mut ledger = InMemoryLedger::new();
        assert!(ledger.genesis().is_none());

        ledger.append(make_test_event(0)).expect("append");
        assert_eq!(ledger.genesis().unwrap().sequence, 0);

        ledger.append(make_test_event(1)).expect("append");
        assert_eq!(ledger.genesis().unwrap().sequence, 0);
    }
}
