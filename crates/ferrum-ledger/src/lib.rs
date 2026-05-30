use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub index: u64,
    pub hash: String,
    pub prev_hash: String,
    pub event_data: Vec<u8>,
    pub event_data_hash: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct HashChainLedger {
    entries: Vec<LedgerEntry>,
}

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("chain integrity broken at index {index}: expected {expected}, found {found}")]
    ChainIntegrityBroken {
        index: u64,
        expected: String,
        found: String,
    },
    #[error("ledger is empty")]
    Empty,
}

const GENESIS_PREV_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn compute_entry_hash(prev_hash: &str, event_data: &[u8], index: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(event_data);
    hasher.update(index.to_le_bytes());
    hex::encode(hasher.finalize())
}

fn compute_data_hash(event_data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(event_data);
    hex::encode(hasher.finalize())
}

impl HashChainLedger {
    pub fn append(&mut self, event_data: &[u8]) -> Result<String, LedgerError> {
        let index = self.entries.len() as u64;
        let prev_hash = if index == 0 {
            GENESIS_PREV_HASH.to_string()
        } else {
            self.entries.last().map(|e| e.hash.clone()).unwrap()
        };
        let event_data_hash = compute_data_hash(event_data);
        let hash = compute_entry_hash(&prev_hash, event_data, index);
        let entry = LedgerEntry {
            index,
            hash: hash.clone(),
            prev_hash,
            event_data: event_data.to_vec(),
            event_data_hash,
            timestamp: Utc::now(),
        };
        self.entries.push(entry);
        Ok(hash)
    }

    pub fn verify_chain(&self) -> Result<bool, LedgerError> {
        if self.entries.is_empty() {
            return Ok(true);
        }
        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            let prev_hash = if i == 0 {
                GENESIS_PREV_HASH.to_string()
            } else {
                self.entries[i - 1].hash.clone()
            };
            // Recompute event_data_hash to verify it matches
            let expected_data_hash = compute_data_hash(&entry.event_data);
            if entry.event_data_hash != expected_data_hash {
                return Err(LedgerError::ChainIntegrityBroken {
                    index: entry.index,
                    expected: expected_data_hash,
                    found: entry.event_data_hash.clone(),
                });
            }
            // Recompute entry hash using stored event_data bytes
            let recomputed = compute_entry_hash(&prev_hash, &entry.event_data, entry.index);
            if entry.hash != recomputed {
                return Err(LedgerError::ChainIntegrityBroken {
                    index: entry.index,
                    expected: recomputed,
                    found: entry.hash.clone(),
                });
            }
        }
        Ok(true)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get_entry(&self, index: usize) -> Option<&LedgerEntry> {
        self.entries.get(index)
    }

    pub fn latest_hash(&self) -> Option<String> {
        self.entries.last().map(|e| e.hash.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_entry_has_zero_prev_hash() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"genesis event").unwrap();
        assert_eq!(ledger.entries[0].prev_hash, GENESIS_PREV_HASH);
    }

    #[test]
    fn test_append_returns_hash() {
        let mut ledger = HashChainLedger::default();
        let hash = ledger.append(b"first event").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_chain_grows() {
        let mut ledger = HashChainLedger::default();
        assert_eq!(ledger.len(), 0);
        ledger.append(b"event 1").unwrap();
        assert_eq!(ledger.len(), 1);
        ledger.append(b"event 2").unwrap();
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn test_verify_chain_on_valid_chain() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"event 1").unwrap();
        ledger.append(b"event 2").unwrap();
        assert!(ledger.verify_chain().is_ok());
    }

    #[test]
    fn test_verify_chain_detects_tampering() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"event 1").unwrap();
        ledger.append(b"event 2").unwrap();
        // Tamper with the second entry's hash
        ledger.entries[1].hash = "a".repeat(64);
        assert!(ledger.verify_chain().is_err());
    }

    #[test]
    fn test_latest_hash_matches_last_entry() {
        let mut ledger = HashChainLedger::default();
        let h1 = ledger.append(b"event 1").unwrap();
        assert_eq!(ledger.latest_hash(), Some(h1.clone()));
        let h2 = ledger.append(b"event 2").unwrap();
        assert_eq!(ledger.latest_hash(), Some(h2));
    }

    #[test]
    fn test_each_entry_references_previous() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"event 1").unwrap();
        ledger.append(b"event 2").unwrap();
        assert_eq!(ledger.entries[1].prev_hash, ledger.entries[0].hash);
    }

    #[test]
    fn test_empty_ledger_latest_hash_is_none() {
        let ledger = HashChainLedger::default();
        assert_eq!(ledger.latest_hash(), None);
    }

    #[test]
    fn test_empty_ledger_verify_returns_ok() {
        let ledger = HashChainLedger::default();
        assert!(ledger.verify_chain().unwrap());
    }

    #[test]
    fn test_entry_index_increments() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"e0").unwrap();
        assert_eq!(ledger.entries[0].index, 0);
        ledger.append(b"e1").unwrap();
        assert_eq!(ledger.entries[1].index, 1);
        ledger.append(b"e2").unwrap();
        assert_eq!(ledger.entries[2].index, 2);
    }

    #[test]
    fn test_event_data_hash_matches_sha256() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"hello world").unwrap();
        let entry = &ledger.entries[0];
        // Manually compute sha256 of "hello world"
        let mut hasher = Sha256::new();
        hasher.update(b"hello world");
        let expected = hex::encode(hasher.finalize());
        assert_eq!(entry.event_data_hash, expected);
    }

    #[test]
    fn test_verify_chain_detects_tampered_prev_hash() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"event 1").unwrap();
        ledger.append(b"event 2").unwrap();
        // Tamper with first entry's hash - second entry's prev_hash won't match
        ledger.entries[0].hash = "b".repeat(64);
        assert!(ledger.verify_chain().is_err());
    }

    #[test]
    fn test_verify_chain_detects_tampered_event_data_hash() {
        let mut ledger = HashChainLedger::default();
        ledger.append(b"event 1").unwrap();
        ledger.append(b"event 2").unwrap();
        // Tamper with first entry's event_data_hash - its hash will change
        ledger.entries[0].event_data_hash = "c".repeat(64);
        assert!(ledger.verify_chain().is_err());
    }
}
