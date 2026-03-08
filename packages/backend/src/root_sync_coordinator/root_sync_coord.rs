use anyhow::{anyhow, Result};
use starknet::core::types::Felt;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};
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
    batch_notify: Arc<Notify>,
    last_synced_evm_root: RwLock<String>,
    last_synced_starknet_root: RwLock<String>,
}

impl RootSyncCoordinator {
    pub fn new(
        store: Arc<dyn IntentStore>,
        evm_relayer: Arc<EvmRelayer>,
        starknet_relayer: Arc<StarkNetRelayer>,
        merkle_manager: Arc<MerkleTreeManager>,
        batch_notify: Arc<Notify>,
    ) -> Self {
        Self {
            store,
            evm_relayer,
            starknet_relayer,
            merkle_manager,
            batch_notify,
            last_synced_evm_root: RwLock::new(String::new()),
            last_synced_starknet_root: RwLock::new(String::new()),
        }
    }

    pub async fn sync_all_roots(&self) -> Result<()> {
        if let Err(e) = self.sync_evm_root_to_starknet().await {
            error!("❌ EVM → StarkNet root sync: {}", e);
        }
        if let Err(e) = self.sync_starknet_root_to_evm().await {
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

        let root_felt = truncate_root_to_felt(&local_root)?;
        let root_felt_hex = format!("0x{:064x}", root_felt);

        if *self.last_synced_evm_root.read().await == root_felt_hex {
            return Ok(());
        }

        let remote = self
            .starknet_relayer
            .get_latest_remote_root(chain_id_felt)
            .await;

        let needs_sync = match remote {
            Ok(snapshot) => {
                let remote_norm = format!(
                    "0x{:0>64}",
                    snapshot.root.to_lowercase().trim_start_matches("0x")
                );
                remote_norm != root_felt_hex.to_lowercase()
            }
            Err(_) => true,
        };

        if needs_sync {
            info!("🌉 [EVM → StarkNet] Syncing root: {}", &root_felt_hex[..16]);

            let leaf_count_pair = u256_from_u64(leaf_count);

            let tx = self
                .starknet_relayer
                .sync_merkle_root(chain_id_felt, root_felt, leaf_count_pair)
                .await?;

            info!("✅ Root synced to StarkNet, tx: 0x{:x}", tx);

            *self.last_synced_evm_root.write().await = root_felt_hex;

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
        let leaf_count = self.merkle_manager.get_leaf_count(ChainId::Starknet).await;

        if leaf_count == 0 {
            return Ok(());
        }

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

        if *self.last_synced_starknet_root.read().await == local_root {
            return Ok(());
        }

        let remote = self
            .evm_relayer
            .get_latest_remote_root(ChainId::Starknet.as_chain_id_str())
            .await;

        let needs_sync = match remote {
            Ok(snapshot) => {
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
            Err(_) => true,
        };

        if needs_sync {
            info!("🌉 [StarkNet → EVM] Syncing root: {}", &local_root[..16]);

            let tx = self
                .evm_relayer
                .sync_merkle_root(ChainId::Starknet.as_chain_id_str(), &local_root, leaf_count)
                .await?;

            info!("✅ Root synced to EVM, tx: {:?}", tx);

            *self.last_synced_starknet_root.write().await = local_root;

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
        info!("🔄 RootSyncCoordinator started (event-driven + 5min fallback)");

        let fallback_interval = Duration::from_secs(300);

        loop {
            // Wake on new commitment OR after 5 min — whichever comes first
            tokio::select! {
                _ = self.batch_notify.notified() => {
                    info!("🔔 Batch notification — syncing roots");
                }
                _ = sleep(fallback_interval) => {
                    info!("⏰ Fallback timer — syncing roots");
                }
            }

            for attempt in 1..=3 {
                match self.sync_all_roots().await {
                    Ok(()) => break,
                    Err(e) => {
                        error!("❌ Root sync failed (attempt {}/3): {}", attempt, e);
                        if attempt < 3 {
                            sleep(Duration::from_secs(10 * attempt)).await;
                        }
                    }
                }
            }
        }
    }
}

fn u256_from_u64(value: u64) -> (Felt, Felt) {
    (Felt::from(value), Felt::ZERO)
}

fn truncate_root_to_felt(root_hex: &str) -> Result<Felt> {
    use num_bigint::BigUint;
    use num_traits::Num;

    let root_clean = root_hex.strip_prefix("0x").unwrap_or(root_hex);
    let root_bigint =
        BigUint::from_str_radix(root_clean, 16).map_err(|e| anyhow!("Invalid root hex: {}", e))?;

    let felt_max = BigUint::from_str_radix(
        "0800000000000011000000000000000000000000000000000000000000000000",
        16,
    )
    .unwrap();

    let truncated = root_bigint % felt_max;
    let truncated_hex = format!("0x{:064x}", truncated);

    Felt::from_hex(&truncated_hex)
        .map_err(|e| anyhow!("Failed to create Felt from truncated root: {}", e))
}
