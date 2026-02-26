use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::{
    merkle_manager::proof_generator::{HashType, MerkleProofGenerator},
    models::models::{ChainId, IntentStore},
};

pub struct MerkleTreeManager {
    store: Arc<dyn IntentStore>,
    evm_generator: MerkleProofGenerator,
    starknet_generator: MerkleProofGenerator,
    evm_leaves: Arc<RwLock<Vec<String>>>,
    starknet_leaves: Arc<RwLock<Vec<String>>>,
}

impl MerkleTreeManager {
    pub fn new(store: Arc<dyn IntentStore>) -> Self {
        Self {
            store,
            evm_generator: MerkleProofGenerator::new(HashType::Keccak256),
            starknet_generator: MerkleProofGenerator::new(HashType::Poseidon),
            evm_leaves: Arc::new(RwLock::new(Vec::new())),
            starknet_leaves: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn load_from_store(&self) -> Result<()> {
        let evm_leaves = self.store.get_leaves("evm_commitments")?;
        let starknet_leaves = self.store.get_leaves("starknet_commitments")?;

        info!(
            "📂 Loaded {} EVM leaves, {} StarkNet leaves",
            evm_leaves.len(),
            starknet_leaves.len()
        );

        *self.evm_leaves.write().await = evm_leaves;
        *self.starknet_leaves.write().await = starknet_leaves;

        Ok(())
    }

    pub async fn add_commitment(&self, chain: ChainId, commitment: &str) -> Result<String> {
        info!(
            "🌱 [ADD_COMMITMENT] chain={} commitment={}",
            chain,
            &commitment[..16]
        );
        let (leaves, generator, tree_name) = match chain {
            ChainId::Evm => (&self.evm_leaves, &self.evm_generator, "evm_commitments"),
            ChainId::Starknet => (
                &self.starknet_leaves,
                &self.starknet_generator,
                "starknet_commitments",
            ),
        };

        let mut leaves_guard = leaves.write().await;

        if leaves_guard
            .iter()
            .any(|l| l.to_lowercase() == commitment.to_lowercase())
        {
            return Err(anyhow!("Commitment already exists in tree"));
        }

        leaves_guard.push(commitment.to_string());
        self.store.add_leaf(tree_name, commitment)?;

        let root = generator.compute_root(&leaves_guard)?;
        self.store
            .save_root(tree_name, &root, leaves_guard.len() as u64)?;

        info!(
            "🌳 [{}] Added commitment, tree size: {}, root: {}",
            chain,
            leaves_guard.len(),
            &root[..16]
        );

        Ok(root)
    }

    pub async fn get_root(&self, chain: ChainId) -> Result<String> {
        let (leaves, generator) = match chain {
            ChainId::Evm => (&self.evm_leaves, &self.evm_generator),
            ChainId::Starknet => (&self.starknet_leaves, &self.starknet_generator),
        };

        let leaves_guard = leaves.read().await;
        if leaves_guard.is_empty() {
            return Ok(
                "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            );
        }

        generator.compute_root(&leaves_guard)
    }

    pub async fn get_leaf_count(&self, chain: ChainId) -> u64 {
        match chain {
            ChainId::Evm => self.evm_leaves.read().await.len() as u64,
            ChainId::Starknet => self.starknet_leaves.read().await.len() as u64,
        }
    }

    /// Snapshot the in-memory leaves for a given chain.
    /// Prefer this over DB queries for proof generation — it is always the
    /// live set that the Merkle root on-chain was computed from.
    pub async fn get_leaves_snapshot(&self, chain: ChainId) -> Vec<String> {
        match chain {
            ChainId::Evm => self.evm_leaves.read().await.clone(),
            ChainId::Starknet => self.starknet_leaves.read().await.clone(),
        }
    }
}
