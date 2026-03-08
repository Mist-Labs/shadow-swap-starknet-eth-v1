use anyhow::{anyhow, Result};
use ethers::types::H256;
use starknet::core::types::Felt;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::{
    evm::relayer::EvmRelayer,
    merkle_manager::tree_manager::MerkleTreeManager,
    models::models::{ChainId, IntentStatus, IntentStore, ShadowIntent},
    near_client::model::{NearClient, NearSwapStatus},
    starknet::relayer::StarkNetRelayer,
};

pub struct RelayCoordinator {
    store: Arc<dyn IntentStore>,
    evm_relayer: Arc<EvmRelayer>,
    starknet_relayer: Arc<StarkNetRelayer>,
    near_client: Arc<NearClient>,
    merkle_manager: Arc<MerkleTreeManager>,
    poll_interval_secs: u64,
    batch_notify: Arc<Notify>,
}

impl RelayCoordinator {
    pub fn new(
        store: Arc<dyn IntentStore>,
        evm_relayer: Arc<EvmRelayer>,
        starknet_relayer: Arc<StarkNetRelayer>,
        near_client: Arc<NearClient>,
        merkle_manager: Arc<MerkleTreeManager>,
        poll_interval_secs: u64,
        batch_notify: Arc<Notify>,
    ) -> Self {
        Self {
            store,
            evm_relayer,
            starknet_relayer,
            near_client,
            merkle_manager,
            poll_interval_secs,
            batch_notify,
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!(
            "ShadowSwap relay coordinator started ({}s interval)",
            self.poll_interval_secs
        );

        self.merkle_manager.load_from_store().await?;

        loop {
            if let Err(e) = self.process_all_stages().await {
                error!("Processing cycle error: {}", e);
            }
            sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    async fn process_all_stages(&self) -> Result<()> {
        self.process_pending_intents().await?;
        self.try_process_evm_batch().await;
        self.try_process_starknet_batch().await;
        self.process_batched_intents().await?;
        self.process_near_submitted().await?;
        Ok(())
    }

    async fn try_process_evm_batch(&self) {
        match self.evm_relayer.get_pending_batch_info().await {
            Ok((count, _, time_remaining)) if count > 0 && time_remaining == 0 => {
                match self.evm_relayer.process_batch_if_timeout().await {
                    Ok(tx) => info!("✅ EVM batch processed: {:?}", tx),
                    Err(e) => error!("❌ EVM batch processing failed: {}", e),
                }
            }
            Ok((count, _, time_remaining)) if count > 0 => {
                info!(
                    "⏳ EVM batch has {} commitments, {}s remaining",
                    count, time_remaining
                );
            }
            Err(e) => error!("Failed to get EVM batch info: {}", e),
            _ => {}
        }
    }

    async fn try_process_starknet_batch(&self) {
        match self.starknet_relayer.get_pending_batch_info().await {
            Ok((count, _, _)) if count > 0 => {
                match self.starknet_relayer.process_batch_if_timeout().await {
                    Ok(tx) => info!("✅ StarkNet batch processed: 0x{:x}", tx),
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }

    // ==============================================================
    //  STAGE 1: Pending -> Batched
    // ==============================================================

    async fn process_pending_intents(&self) -> Result<()> {
        let intents = self.store.get_intents_by_status(IntentStatus::Pending)?;
        for intent in intents {
            if let Err(e) = self.add_to_batch(&intent).await {
                error!("Failed to batch intent {}: {}", &intent.id[..16], e);
            }
        }
        Ok(())
    }

    async fn add_to_batch(&self, intent: &ShadowIntent) -> Result<()> {
        info!(
            "Adding intent {} to batch on {}",
            &intent.id[..16],
            intent.source_chain
        );

        let already_on_chain = match intent.source_chain {
            ChainId::Evm => {
                let tx = self
                    .evm_relayer
                    .add_to_pending_batch(
                        &intent.commitment,
                        &intent.near_intents_id,
                        &intent.view_key,
                    )
                    .await?;
                tx == H256::zero()
            }
            ChainId::Starknet => {
                let commitment = str_to_felt(&intent.commitment)?;
                let near_id = near_id_to_felt(&intent.near_intents_id)?;
                let view_key = hex_to_felt_reduced(&intent.view_key)?;

                self.starknet_relayer
                    .add_to_pending_batch(commitment, near_id, view_key)
                    .await?;
                false
            }
        };

        if !already_on_chain {
            match self
                .merkle_manager
                .add_commitment(intent.source_chain, &intent.commitment)
                .await
            {
                Ok(_) => {}
                Err(e) if e.to_string().contains("Commitment already exists in tree") => {
                    warn!(
                        "Commitment {} already in tree — skipping merkle add",
                        &intent.id[..16]
                    );
                }
                Err(e) => return Err(e),
            }
        } else {
            warn!(
                "Commitment {} already on-chain — skipping merkle add",
                &intent.id[..16]
            );
        }

        self.store
            .update_intent_status(&intent.id, IntentStatus::Batched)?;

        self.batch_notify.notify_one();

        info!("Intent {} batched", &intent.id[..16]);
        Ok(())
    }

    // ==============================================================
    //  STAGE 2: Batched -> NearSubmitted
    // ==============================================================

    async fn process_batched_intents(&self) -> Result<()> {
        let intents = self.store.get_intents_by_status(IntentStatus::Batched)?;
        for intent in intents {
            if intent.deposit_address.is_some() {
                self.store
                    .update_intent_status(&intent.id, IntentStatus::NearSubmitted)?;
                info!(
                    "Intent {} -> NearSubmitted (deposit address set)",
                    &intent.id[..16]
                );
            }
        }
        Ok(())
    }

    // ==============================================================
    //  STAGE 3: NearSubmitted -> TokensDelivered
    // ==============================================================

    async fn process_near_submitted(&self) -> Result<()> {
        let intents = self
            .store
            .get_intents_by_status(IntentStatus::NearSubmitted)?;
        for intent in intents {
            if let Err(e) = self.check_near_delivery(&intent).await {
                warn!("NEAR check failed for {}: {}", &intent.id[..16], e);
            }
        }
        Ok(())
    }

    async fn check_near_delivery(&self, intent: &ShadowIntent) -> Result<()> {
        let deposit_address = intent
            .deposit_address
            .as_ref()
            .ok_or_else(|| anyhow!("No deposit address for intent {}", &intent.id[..16]))?;

        let result = self.near_client.get_status(deposit_address).await?;

        self.store
            .update_intent_near_status(&intent.id, &format!("{:?}", result.status))?;

        match result.status {
            NearSwapStatus::Success => {
                if let Some(tx_hash) = result.destination_tx_hashes.first() {
                    info!(
                        "NEAR success for {} — dest_chain={} dest_tx_hash={}",
                        &intent.id[..16],
                        intent.dest_chain,
                        tx_hash
                    );
                    let verified = self.verify_delivery(intent, tx_hash).await?;

                    if verified {
                        self.store.update_intent_dest_tx(&intent.id, tx_hash)?;
                        self.store
                            .update_intent_status(&intent.id, IntentStatus::TokensDelivered)?;
                        info!(
                            "Tokens delivered for {}, dest tx: {}",
                            &intent.id[..16],
                            &tx_hash[..18.min(tx_hash.len())]
                        );
                    } else {
                        warn!("Transfer verification failed for {}", &intent.id[..16]);
                    }
                } else {
                    warn!(
                        "NEAR SUCCESS but no destination tx hashes for {}",
                        &intent.id[..16]
                    );
                }
            }
            NearSwapStatus::Failed => {
                self.store
                    .update_intent_status(&intent.id, IntentStatus::Failed)?;
                error!("NEAR swap failed for {}", &intent.id[..16]);
            }
            NearSwapStatus::Refunded => {
                self.store
                    .update_intent_status(&intent.id, IntentStatus::Refunded)?;
                warn!(
                    "NEAR swap refunded for {}, reason: {:?}",
                    &intent.id[..16],
                    result.refund_reason
                );
            }
            NearSwapStatus::IncompleteDeposit => {
                warn!(
                    "Incomplete deposit for {} — user may need to top up",
                    &intent.id[..16]
                );
            }
            NearSwapStatus::PendingDeposit | NearSwapStatus::Processing => {}
        }

        Ok(())
    }

    async fn verify_delivery(&self, intent: &ShadowIntent, tx_hash: &str) -> Result<bool> {
        match intent.dest_chain {
            ChainId::Evm => self.evm_relayer.verify_transaction_exists(tx_hash).await,
            ChainId::Starknet => {
                self.starknet_relayer
                    .verify_transaction_exists(tx_hash)
                    .await
            }
        }
    }
}

// ===== HELPERS =====

fn str_to_felt(hex: &str) -> Result<Felt> {
    Felt::from_hex(hex)
        .map_err(|e| anyhow!("Invalid felt hex '{}': {}", &hex[..16.min(hex.len())], e))
}

fn hex_to_felt_reduced(hex: &str) -> Result<Felt> {
    let stripped = hex.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(stripped)
        .map_err(|e| anyhow!("Invalid hex '{}': {}", &hex[..16.min(hex.len())], e))?;
    let mut padded = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    padded[start..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
    Ok(Felt::from_bytes_be(&padded))
}

fn near_id_to_felt(near_id: &str) -> Result<Felt> {
    let cleaned = near_id.replace('-', "");
    let hex_str = if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        cleaned.clone()
    } else {
        format!("0x{}", cleaned)
    };

    if let Ok(felt) = Felt::from_hex(&hex_str) {
        return Ok(felt);
    }

    let raw = hex_str.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(raw).map_err(|e| {
        anyhow!(
            "Invalid hex in NEAR ID '{}': {}",
            &near_id[..16.min(near_id.len())],
            e
        )
    })?;
    let truncated = if bytes.len() > 31 {
        &bytes[bytes.len() - 31..]
    } else {
        &bytes
    };
    Ok(Felt::from_bytes_be_slice(truncated))
}
