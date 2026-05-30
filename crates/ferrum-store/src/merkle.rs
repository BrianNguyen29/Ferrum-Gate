use sha2::{Digest, Sha256};

/// Compute a deterministic Merkle root over a list of hex-encoded content hashes.
///
/// Algorithm:
/// - Leaves: SHA256(0x00 || decoded content_hash_bytes)
/// - Internal nodes: SHA256(0x01 || left || right)
/// - For odd number of nodes at any level, duplicate the last node.
///
/// Returns the hex-encoded root hash, or an empty string if `hashes` is empty.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        assert_eq!(compute_merkle_root(&[]), "");
    }

    #[test]
    fn test_one_leaf() {
        let hash = "a".repeat(64);
        let root = compute_merkle_root(std::slice::from_ref(&hash));
        let mut hasher = sha2::Sha256::new();
        hasher.update([0x00]);
        hasher.update(hex::decode(&hash).unwrap());
        let expected = hex::encode(hasher.finalize());
        assert_eq!(root, expected);
    }

    #[test]
    fn test_two_leaves() {
        let h1 = "a".repeat(64);
        let h2 = "b".repeat(64);
        let root = compute_merkle_root(&[h1.clone(), h2.clone()]);
        let leaf1 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x00]);
            hasher.update(hex::decode(&h1).unwrap());
            hasher.finalize().to_vec()
        };
        let leaf2 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x00]);
            hasher.update(hex::decode(&h2).unwrap());
            hasher.finalize().to_vec()
        };
        let mut hasher = sha2::Sha256::new();
        hasher.update([0x01]);
        hasher.update(&leaf1);
        hasher.update(&leaf2);
        let expected = hex::encode(hasher.finalize());
        assert_eq!(root, expected);
    }

    #[test]
    fn test_three_leaves() {
        let h1 = "a".repeat(64);
        let h2 = "b".repeat(64);
        let h3 = "c".repeat(64);
        let root = compute_merkle_root(&[h1.clone(), h2.clone(), h3.clone()]);
        let leaf1 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x00]);
            hasher.update(hex::decode(&h1).unwrap());
            hasher.finalize().to_vec()
        };
        let leaf2 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x00]);
            hasher.update(hex::decode(&h2).unwrap());
            hasher.finalize().to_vec()
        };
        let leaf3 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x00]);
            hasher.update(hex::decode(&h3).unwrap());
            hasher.finalize().to_vec()
        };
        let node1 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x01]);
            hasher.update(&leaf1);
            hasher.update(&leaf2);
            hasher.finalize().to_vec()
        };
        let node2 = {
            let mut hasher = sha2::Sha256::new();
            hasher.update([0x01]);
            hasher.update(&leaf3);
            hasher.update(&leaf3); // duplicate last
            hasher.finalize().to_vec()
        };
        let mut hasher = sha2::Sha256::new();
        hasher.update([0x01]);
        hasher.update(&node1);
        hasher.update(&node2);
        let expected = hex::encode(hasher.finalize());
        assert_eq!(root, expected);
    }
}
