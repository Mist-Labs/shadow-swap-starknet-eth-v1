use anyhow::{anyhow, Context, Result};
use sha3::{Digest, Keccak256};
use starknet_core::types::Felt;
use starknet_crypto::{poseidon_hash, poseidon_hash_many};
use tracing::{debug, info};

const ZERO_LEAF: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

/// Fixed tree height matching both EVM and StarkNet on-chain contracts.
const TREE_HEIGHT: usize = 20;

// ===== HASH TYPE =====

#[derive(Debug, Clone, Copy)]
pub enum HashType {
    Keccak256, // For EVM
    Poseidon,  // For StarkNet
}

// ===== MERKLE PROOF GENERATOR =====

pub struct MerkleProofGenerator {
    hash_type: HashType,
}

impl MerkleProofGenerator {
    pub fn new(hash_type: HashType) -> Self {
        Self { hash_type }
    }

    /// Hash pair using Keccak256 (EVM) - SORTED ORDER (OZ-compatible)
    fn hash_pair_keccak(&self, a: &str, b: &str) -> Result<String> {
        let a_bytes =
            hex::decode(a.trim_start_matches("0x")).context("Failed to decode left hash")?;
        let b_bytes =
            hex::decode(b.trim_start_matches("0x")).context("Failed to decode right hash")?;

        if a_bytes.len() != 32 || b_bytes.len() != 32 {
            return Err(anyhow!(
                "Invalid hash length: left={}, right={}",
                a_bytes.len(),
                b_bytes.len()
            ));
        }

        // SORTED ORDER - matches OZ MerkleProof.sol and your contract _hashPair
        let (first, second) = if a_bytes < b_bytes {
            (&a_bytes, &b_bytes)
        } else {
            (&b_bytes, &a_bytes)
        };

        let combined = [first.as_slice(), second.as_slice()].concat();
        let mut hasher = Keccak256::new();
        hasher.update(&combined);
        let hash = hasher.finalize();

        Ok(format!("0x{}", hex::encode(hash)))
    }

    /// Hash pair using Poseidon (StarkNet) - SORTED ORDER
    fn hash_pair_poseidon(&self, a: &str, b: &str) -> Result<String> {
        let a_felt = felt_from_hex(a)?;
        let b_felt = felt_from_hex(b)?;

        // SORTED ORDER - matches your Cairo _hash_pair
        let hash = if a_felt < b_felt {
            poseidon_hash_many(&[a_felt, b_felt])
        } else {
            poseidon_hash_many(&[b_felt, a_felt])
        };

        Ok(format!("0x{:064x}", hash))
    }

    /// Hash pair (delegates to appropriate hash function)
    fn hash_pair(&self, a: &str, b: &str) -> Result<String> {
        match self.hash_type {
            HashType::Keccak256 => self.hash_pair_keccak(a, b),
            HashType::Poseidon => self.hash_pair_poseidon(a, b),
        }
    }

    /// Pre-compute zero hashes for each tree level.
    /// zeros[0] = 0x0, zeros[i] = hash(zeros[i-1], zeros[i-1])
    fn compute_zeros(&self) -> Result<Vec<String>> {
        let mut zeros = Vec::with_capacity(TREE_HEIGHT);
        zeros.push(ZERO_LEAF.to_string());
        for i in 1..TREE_HEIGHT {
            let prev = &zeros[i - 1];
            zeros.push(self.hash_pair(prev, prev)?);
        }
        Ok(zeros)
    }

    /// Generate Merkle proof using a fixed-height tree (matching on-chain contracts).
    ///
    /// The on-chain contracts use an incremental Merkle tree of height 20.
    /// Empty subtrees use pre-computed zero hashes: zeros[level] = hash(zeros[level-1], zeros[level-1]).
    pub fn generate_proof(
        &self,
        leaves: &[String],
        leaf: &str,
    ) -> Result<(Vec<String>, usize, String)> {
        info!(
            "📋 Generating proof with {} leaves, leaf={}",
            leaves.len(),
            &leaf[..leaf.len().min(10)]
        );

        if leaves.is_empty() {
            return Err(anyhow!("Cannot generate proof for empty tree"));
        }

        let leaf_index = leaves
            .iter()
            .position(|l| l.to_lowercase() == leaf.to_lowercase())
            .ok_or_else(|| anyhow!("Leaf {} not found", &leaf[..leaf.len().min(10)]))?;

        info!(
            "🔍 Found leaf at index {} (tree has {} leaves)",
            leaf_index,
            leaves.len()
        );

        let zeros = self.compute_zeros()?;

        info!("🌳 Fixed-height tree: height={}", TREE_HEIGHT);

        let mut nodes: Vec<std::collections::HashMap<usize, String>> =
            vec![std::collections::HashMap::new(); TREE_HEIGHT + 1];
        let mut filled_subtrees: Vec<String> = vec![zeros[0].clone(); TREE_HEIGHT];

        for (i, leaf_val) in leaves.iter().enumerate() {
            let mut current_hash = leaf_val.clone();
            let mut index = i;

            // Record the leaf itself
            nodes[0].insert(i, leaf_val.clone());

            for height in 0..TREE_HEIGHT {
                let (left, right) = if index & 1 == 0 {
                    filled_subtrees[height] = current_hash.clone();
                    (current_hash.clone(), zeros[height].clone())
                } else {
                    (filled_subtrees[height].clone(), current_hash.clone())
                };

                current_hash = self.hash_pair(&left, &right)?;
                index >>= 1;

                // Record this node
                nodes[height + 1].insert(index, current_hash.clone());
            }
        }

        // Collect proof siblings — walk up from leaf_index
        let mut proof = Vec::with_capacity(TREE_HEIGHT);
        let mut current_index = leaf_index;

        for height in 0..TREE_HEIGHT {
            let sibling_index = current_index ^ 1;
            let sibling = nodes[height]
                .get(&sibling_index)
                .cloned()
                .unwrap_or_else(|| zeros[height].clone());
            proof.push(sibling);
            current_index >>= 1;
        }

        let root = nodes[TREE_HEIGHT]
            .get(&0)
            .cloned()
            .unwrap_or_else(|| zeros[TREE_HEIGHT - 1].clone());

        info!(
            "✅ Proof generated: {} siblings, root={}",
            proof.len(),
            &root[..root.len().min(10)]
        );

        Ok((proof, leaf_index, root))
    }

    /// Compute Merkle root using fixed-height tree
    pub fn compute_root(&self, leaves: &[String]) -> Result<String> {
        if leaves.is_empty() {
            return Ok(ZERO_LEAF.to_string());
        }

        let zeros = self.compute_zeros()?;
        let mut filled_subtrees: Vec<String> = vec![ZERO_LEAF.to_string(); TREE_HEIGHT];
        let mut current_root = ZERO_LEAF.to_string();

        for (leaf_index, leaf) in leaves.iter().enumerate() {
            let mut current_hash = leaf.clone();
            let mut index = leaf_index;

            for height in 0..TREE_HEIGHT {
                let (left, right) = if index & 1 == 0 {
                    filled_subtrees[height] = current_hash.clone();
                    (current_hash.clone(), zeros[height].clone())
                } else {
                    (filled_subtrees[height].clone(), current_hash.clone())
                };

                current_hash = self.hash_pair(&left, &right)?;
                index >>= 1;
            }

            current_root = current_hash;
        }

        Ok(current_root)
    }

    /// Verify proof against a root
    pub fn verify_proof(
        &self,
        proof: &[String],
        root: &str,
        leaf: &str,
        index: usize,
    ) -> Result<bool> {
        let mut computed_hash = leaf.to_string();
        let mut current_index = index;

        debug!("🔍 Verifying proof:");
        debug!("  Leaf: {}", &leaf[..leaf.len().min(10)]);
        debug!("  Index: {}", index);
        debug!("  Expected root: {}", &root[..root.len().min(10)]);

        for (level, proof_element) in proof.iter().enumerate() {
            let is_right = (current_index & 1) == 1;

            computed_hash = if is_right {
                self.hash_pair(proof_element, &computed_hash)?
            } else {
                self.hash_pair(&computed_hash, proof_element)?
            };

            current_index >>= 1;
        }

        debug!(
            "  Computed root: {}",
            &computed_hash[..computed_hash.len().min(10)]
        );

        let is_valid = computed_hash.to_lowercase() == root.to_lowercase();

        if is_valid {
            info!("✅ Proof verification successful");
        } else {
            info!("❌ Proof verification failed");
        }

        Ok(is_valid)
    }
}

fn felt_from_hex(hex: &str) -> Result<Felt> {
    let hex = hex.trim_start_matches("0x");
    Felt::from_hex(hex).map_err(|e| anyhow!("Invalid Felt hex: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccak_hash_pair_sorted() {
        let gen = MerkleProofGenerator::new(HashType::Keccak256);
        let a = "0x2222222222222222222222222222222222222222222222222222222222222222";
        let b = "0x1111111111111111111111111111111111111111111111111111111111111111";

        let result1 = gen.hash_pair_keccak(a, b).unwrap();
        let result2 = gen.hash_pair_keccak(b, a).unwrap();

        assert_eq!(result1, result2, "Hash should be commutative");
    }

    #[test]
    fn test_first_pair_hash() {
        let gen = MerkleProofGenerator::new(HashType::Keccak256);

        let leaf0 = "0xf258a2bec7d9b680608549deb361b71f518915f3131833992018ba0aaa40bfa0";
        let leaf1 = "0x2db53c927146d07ba1994402150cc71c2387faf63ed1afabc85a019e47661fe8";

        let hash = gen.hash_pair_keccak(leaf0, leaf1).unwrap();
        println!("hash_pair({}, {}) = {}", &leaf0[..16], &leaf1[..16], hash);
    }

    #[test]
    fn test_zero_hashes() {
        let gen = MerkleProofGenerator::new(HashType::Keccak256);
        let zeros = gen.compute_zeros().unwrap();
        println!("zeros[0] = {}", zeros[0]);
        println!("zeros[1] = {}", zeros[1]);
        println!("zeros[2] = {}", zeros[2]);
        println!("zeros[3] = {}", zeros[3]);
        println!("zeros[4] = {}", zeros[4]);
    }

    #[test]
    fn test_poseidon_zeros_are_consistent() {
        let gen = MerkleProofGenerator::new(HashType::Poseidon);
        let zeros = gen.compute_zeros().unwrap();
        assert_eq!(zeros.len(), TREE_HEIGHT);
        assert_eq!(zeros[0], ZERO_LEAF);
        // zeros[1] = hash(0, 0) should be deterministic
        let z1 = gen.hash_pair(ZERO_LEAF, ZERO_LEAF).unwrap();
        assert_eq!(zeros[1], z1);
    }

    #[test]
    fn test_proof_height_matches_tree() {
        let gen = MerkleProofGenerator::new(HashType::Poseidon);
        let leaves =
            vec!["0x0011111111111111111111111111111111111111111111111111111111111111".to_string()];
        let (proof, idx, _root) = gen.generate_proof(&leaves, &leaves[0]).unwrap();
        assert_eq!(
            proof.len(),
            TREE_HEIGHT,
            "Proof should have TREE_HEIGHT siblings"
        );
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_poseidon_hash_many_vs_hash() {
        let a = Felt::ZERO;
        let b = Felt::ZERO;

        let h_many = poseidon_hash_many(&[a, b]);
        let h_two = poseidon_hash(a, b);

        println!("poseidon_hash_many([0,0]) = 0x{:064x}", h_many);
        println!("poseidon_hash(0,0)        = 0x{:064x}", h_two);
        println!("Match: {}", h_many == h_two);

        // Compute root both ways for the actual test leaves
        let leaf0 = "0x0022abec414ba6484b1404720d66108ac2cd0e87f5cdf1f943b06f5bce64970d";
        let leaf1 = "0x0036f6f25e630d091dc8fbe482b762fb292c2287208847fd46e37ec97776c04f";
        let leaves = vec![leaf0.to_string(), leaf1.to_string()];

        // With poseidon_hash_many
        let gen_many = MerkleProofGenerator::new(HashType::Poseidon);
        let root_many = gen_many.compute_root(&leaves).unwrap();
        println!("\nRoot with poseidon_hash_many: {}", root_many);

        // Now compute root using poseidon_hash (2-element) instead
        fn hash_pair_poseidon2(a: &str, b: &str) -> String {
            let a_felt = felt_from_hex(a).unwrap();
            let b_felt = felt_from_hex(b).unwrap();
            let h = if a_felt < b_felt {
                poseidon_hash(a_felt, b_felt)
            } else {
                poseidon_hash(b_felt, a_felt)
            };
            format!("0x{:064x}", h)
        }

        // Build tree with poseidon_hash (height 20)
        let zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
        let mut zeros_ph = vec![zero.to_string()];
        for i in 1..20 {
            zeros_ph.push(hash_pair_poseidon2(&zeros_ph[i - 1], &zeros_ph[i - 1]));
        }
        println!("\nzeros[1] with poseidon_hash: {}", zeros_ph[1]);

        let mut layer = vec![leaf0.to_string(), leaf1.to_string()];
        for level in 0..20 {
            let mut next = Vec::new();
            let pairs = layer.len() / 2;
            for i in 0..pairs {
                next.push(hash_pair_poseidon2(&layer[2 * i], &layer[2 * i + 1]));
            }
            if layer.len() % 2 == 1 {
                next.push(hash_pair_poseidon2(layer.last().unwrap(), &zeros_ph[level]));
            }
            layer = next;
        }
        println!("Root with poseidon_hash: {}", layer[0]);

        // Expected on-chain root
        println!("Expected on-chain root: 0x014b4e96141911123ea7f695863bd495109ce419c9a05971174c90c0f6d9608f");
    }
}
