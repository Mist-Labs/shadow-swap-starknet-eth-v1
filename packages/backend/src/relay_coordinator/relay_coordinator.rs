use anyhow::{anyhow, Result};
use starknet::core::types::Felt;
use std::sync::Arc;
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
}

impl RelayCoordinator {
    pub fn new(
        store: Arc<dyn IntentStore>,
        evm_relayer: Arc<EvmRelayer>,
        starknet_relayer: Arc<StarkNetRelayer>,
        near_client: Arc<NearClient>,
        merkle_manager: Arc<MerkleTreeManager>,
        poll_interval_secs: u64,
    ) -> Self {
        Self {
            store,
            evm_relayer,
            starknet_relayer,
            near_client,
            merkle_manager,
            poll_interval_secs,
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!("ShadowSwap relay coordinator started ({}s interval)", self.poll_interval_secs);

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
        self.try_process_starknet_batch().await;
        self.process_batched_intents().await?;
        self.process_near_submitted().await?;
        Ok(())
    }

    async fn try_process_starknet_batch(&self) {
        match self.starknet_relayer.get_pending_batch_info().await {
            Ok((count, _, _)) if count > 0 => {
                match self.starknet_relayer.process_batch_if_timeout().await {
                    Ok(tx) => info!("StarkNet batch processed: 0x{:x}", tx),
                    Err(_) => {} // timeout not reached yet, ignore
                }
            }
            _ => {}
        }
    }

    // ==============================================================
    //  STAGE 1: Pending -> Batched
    //  Add commitment to source chain pending batch + Merkle tree
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

        match intent.source_chain {
            ChainId::Evm => {
                self.evm_relayer
                    .add_to_pending_batch(
                        &intent.commitment,
                        &intent.near_intents_id,
                        &intent.view_key,
                    )
                    .await?;
            }
            ChainId::Starknet => {
                let commitment = str_to_felt(&intent.commitment)?;
                let near_id = near_id_to_felt(&intent.near_intents_id)?;
                let view_key = hex_to_felt_reduced(&intent.view_key)?;

                self.starknet_relayer
                    .add_to_pending_batch(commitment, near_id, view_key)
                    .await?;
            }
        }

        self.merkle_manager
            .add_commitment(intent.source_chain, &intent.commitment)
            .await?;

        // Commitments table written exclusively by the indexer webhook (CommitmentAdded event).
        // This ensures DB always mirrors confirmed on-chain state — never optimistic writes.

        self.store
            .update_intent_status(&intent.id, IntentStatus::Batched)?;

        info!("Intent {} batched", &intent.id[..16]);
        Ok(())
    }

    // ==============================================================
    //  STAGE 2: Batched -> NearSubmitted
    //  Auto-transition when deposit_address is set (user completed
    //  NEAR 1Click quote and provided deposit address)
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
    //  Poll NEAR 1Click by deposit_address, verify Transfer event
    //
    //  Key: status is queried via GET /v0/status?depositAddress=...
    //  NOT by intent ID. The deposit_address comes from the quote
    //  response and is stored on the ShadowIntent.
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
                    let verified = match intent.dest_chain {
                        ChainId::Evm => {
                            // For cross-chain swaps, token changes (e.g., STRK->USDT)
                            // so we can't verify source token. Just verify TX exists.
                            self.evm_relayer.verify_transaction_exists(tx_hash).await?
                        }
                        ChainId::Starknet => {
                            // NEAR reports SUCCESS + dest tx = tokens delivered
                            true
                        }
                    };

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
}

// ===== HELPERS =====

fn str_to_felt(hex: &str) -> Result<Felt> {
    Felt::from_hex(hex)
        .map_err(|e| anyhow!("Invalid felt hex '{}': {}", &hex[..16.min(hex.len())], e))
}

/// Convert a hex string to Felt, reducing mod P automatically.
/// Use this for values that may be arbitrary 256-bit numbers (e.g. view_key
/// generated client-side as random bytes) that might exceed the Felt252 prime.
fn hex_to_felt_reduced(hex: &str) -> Result<Felt> {
    let stripped = hex.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(stripped)
        .map_err(|e| anyhow!("Invalid hex '{}': {}", &hex[..16.min(hex.len())], e))?;
    let mut padded = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    padded[start..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
    Ok(Felt::from_bytes_be(&padded))
}

/// Convert a NEAR correlation ID (UUID or hex) to a Felt.
/// UUIDs like "f3b696dd-5bd5-4e2b-9052-5168bd7d764d" are stripped of dashes
/// to produce a 32-char hex string (128 bits), which fits in a Felt252.
/// Values that overflow felt252 (>251 bits) are truncated to the lower 31 bytes.
fn near_id_to_felt(near_id: &str) -> Result<Felt> {
    let cleaned = near_id.replace('-', "");
    let hex_str = if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        cleaned.clone()
    } else {
        format!("0x{}", cleaned)
    };

    // Fast path: directly parse (UUIDs and short values always succeed here)
    if let Ok(felt) = Felt::from_hex(&hex_str) {
        return Ok(felt);
    }

    // Overflow path: value is >= field prime. Take the lower 31 bytes (248 bits),
    // which is always within range. near_intents_id is informational, not cryptographic.
    let raw = hex_str.trim_start_matches("0x").trim_start_matches("0X");
    let bytes = hex::decode(raw)
        .map_err(|e| anyhow!("Invalid hex in NEAR ID '{}': {}", &near_id[..16.min(near_id.len())], e))?;
    let truncated = if bytes.len() > 31 { &bytes[bytes.len() - 31..] } else { &bytes };
    Ok(Felt::from_bytes_be_slice(truncated))
}
