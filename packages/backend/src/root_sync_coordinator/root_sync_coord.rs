use anyhow::{anyhow, Result};
use starknet::core::types::Felt;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::{
    evm::relayer::EvmRelayer,
    merkle_manager::tree_manager::MerkleTreeManager,
    models::models::{ChainId, IntentStore},
    starknet::relayer::StarkNetRelayer,
};

const ZERO_ROOT: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

pub struct RootSyncCoordinator {
    store: Arc<dyn IntentStore>,
    evm_relayer: Arc<EvmRelayer>,
    starknet_relayer: Arc<StarkNetRelayer>,
    merkle_manager: Arc<MerkleTreeManager>,
    sync_interval_secs: u64,
}

impl RootSyncCoordinator {
    pub fn new(
        store: Arc<dyn IntentStore>,
        evm_relayer: Arc<EvmRelayer>,
        starknet_relayer: Arc<StarkNetRelayer>,
        merkle_manager: Arc<MerkleTreeManager>,
        sync_interval_secs: u64,
    ) -> Self {
        Self {
            store,
            evm_relayer,
            starknet_relayer,
            merkle_manager,
            sync_interval_secs,
        }
    }

    pub async fn sync_all_roots(&self) -> Result<()> {
        let results = tokio::join!(
            self.sync_evm_root_to_starknet(),
            self.sync_starknet_root_to_evm()
        );

        if let Err(e) = results.0 {
            error!("❌ EVM → StarkNet root sync: {}", e);
        }
        if let Err(e) = results.1 {
            error!("❌ StarkNet → EVM root sync: {}", e);
        }

        Ok(())
    }

    async fn sync_evm_root_to_starknet(&self) -> Result<()> {
        let local_root = self.merkle_manager.get_root(ChainId::Evm).await?;
        let leaf_count = self.merkle_manager.get_leaf_count(ChainId::Evm).await;

        if leaf_count == 0 || local_root == ZERO_ROOT {
            return Ok(());
        }

        let chain_id_felt = Felt::from(ChainId::Evm.as_u64());

        // Truncate EVM root (256-bit) to fit StarkNet Felt (252-bit) once,
        // so the comparison uses the same value that was (or will be) stored on-chain.
        let root_felt = truncate_root_to_felt(&local_root)?;
        let root_felt_hex = format!("0x{:064x}", root_felt);

        let remote = self
            .starknet_relayer
            .get_latest_remote_root(chain_id_felt)
            .await;

        let needs_sync = match remote {
            Ok(snapshot) => {
                // Normalize both to 64-char zero-padded hex before comparing.
                // felt_to_hex uses {:x} (no padding), root_felt_hex uses {:064x}.
                let remote_norm = format!(
                    "0x{:0>64}",
                    snapshot.root.to_lowercase().trim_start_matches("0x")
                );
                remote_norm != root_felt_hex.to_lowercase()
            }
            Err(_) => true, // No root synced yet — proceed with first sync
        };

        if needs_sync {
            info!("🌉 [EVM → StarkNet] Syncing root: {}", &root_felt_hex[..16]);

            let leaf_count_pair = u256_from_u64(leaf_count);

            let tx = self
                .starknet_relayer
                .sync_merkle_root(chain_id_felt, root_felt, leaf_count_pair)
                .await?;

            info!("✅ Root synced to StarkNet, tx: 0x{:x}", tx);

            self.store.log_transaction(
                "root_sync",
                ChainId::Starknet,
                "sync_evm_root",
                &format!("0x{:x}", tx),
                "confirmed",
            )?;
        }

        Ok(())
    }

    async fn sync_starknet_root_to_evm(&self) -> Result<()> {
        let leaf_count = self
            .merkle_manager
            .get_leaf_count(ChainId::Starknet)
            .await;

        if leaf_count == 0 {
            return Ok(());
        }

        // get_root calls compute_root which may fail if the StarkNet tree contains
        // leaves that are >= the Felt prime (e.g. old bad data from a key mismatch).
        // Warn and skip this cycle rather than propagating as an error.
        let local_root = match self.merkle_manager.get_root(ChainId::Starknet).await {
            Ok(root) => root,
            Err(e) => {
                warn!("⚠️  StarkNet Merkle root computation failed (invalid leaf in tree?): {}. Skipping sync.", e);
                return Ok(());
            }
        };

        if local_root == ZERO_ROOT {
            return Ok(());
        }

        let remote = self
            .evm_relayer
            .get_latest_remote_root(ChainId::Starknet.as_chain_id_str())
            .await;

        let needs_sync = match remote {
            Ok(snapshot) => {
                // Normalize both to 64-char zero-padded hex before comparing.
                let remote_norm = format!(
                    "0x{:0>64}",
                    snapshot.root.to_lowercase().trim_start_matches("0x")
                );
                let local_norm = format!(
                    "0x{:0>64}",
                    local_root.to_lowercase().trim_start_matches("0x")
                );
                remote_norm != local_norm
            }
            Err(_) => true, // No root synced yet — proceed with first sync
        };

        if needs_sync {
            info!(
                "🌉 [StarkNet → EVM] Syncing root: {}",
                &local_root[..16]
            );

            let tx = self
                .evm_relayer
                .sync_merkle_root(
                    ChainId::Starknet.as_chain_id_str(),
                    &local_root,
                    leaf_count,
                )
                .await?;

            info!("✅ Root synced to EVM, tx: {:?}", tx);

            self.store.log_transaction(
                "root_sync",
                ChainId::Evm,
                "sync_starknet_root",
                &format!("{:?}", tx),
                "confirmed",
            )?;
        }

        Ok(())
    }

    pub async fn run(self: Arc<Self>) {
        info!(
            "🔄 RootSyncCoordinator started ({}s interval)",
            self.sync_interval_secs
        );

        loop {
            if let Err(e) = self.sync_all_roots().await {
                error!("❌ Root sync cycle failed: {}", e);
            }
            sleep(Duration::from_secs(self.sync_interval_secs)).await;
        }
    }

}

fn u256_from_u64(value: u64) -> (Felt, Felt) {
    (Felt::from(value), Felt::ZERO)
}

/// Truncate 256-bit EVM root to 252-bit StarkNet Felt
/// StarkNet Felt max: 2^252 - 1 (0x0800000000000011000000000000000000000000000000000000000000000000)
fn truncate_root_to_felt(root_hex: &str) -> Result<Felt> {
    use num_bigint::BigUint;
    use num_traits::Num;

    let root_clean = root_hex.strip_prefix("0x").unwrap_or(root_hex);
    let root_bigint = BigUint::from_str_radix(root_clean, 16)
        .map_err(|e| anyhow!("Invalid root hex: {}", e))?;

    // Felt max value (2^252 - 1)
    let felt_max = BigUint::from_str_radix(
        "0800000000000011000000000000000000000000000000000000000000000000",
        16
    ).unwrap();

    // Truncate by taking modulo
    let truncated = root_bigint % felt_max;
    let truncated_hex = format!("0x{:064x}", truncated);

    Felt::from_hex(&truncated_hex)
        .map_err(|e| anyhow!("Failed to create Felt from truncated root: {}", e))
}