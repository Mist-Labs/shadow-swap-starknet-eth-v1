use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use starknet_crypto::Felt;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use zeroize::Zeroizing;

use crate::{
    avnu::client::AvnuClient,
    database::db::Database,
    encryption::decrypt_ecies::{decrypt_with_ecies, decrypt_with_ecies_utf8},
    evm::relayer::EvmRelayer,
    merkle_manager::{
        proof_generator::{HashType, MerkleProofGenerator},
        tree_manager::MerkleTreeManager,
    },
    models::models::{ChainId, IntentStatus, IntentStore, ShadowIntent},
    near_client::model::{NearClient, NearSwapStatus},
    starknet::relayer::{u256_to_felt_pair, StarkNetRelayer},
};

const RESCUE_TIMEOUT_SECS: u64 = 3600;

/// NEAR always delivers STRK on the StarkNet side of the bridge.
/// Any other token in the delivery tx is unexpected and must be rejected.
const STRK_ADDRESS: &str = "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";

pub struct SettlementCoordinator {
    db: Arc<Database>,
    evm_relayer: Arc<EvmRelayer>,
    starknet_relayer: Arc<StarkNetRelayer>,
    near_client: Arc<NearClient>,
    merkle_manager: Arc<MerkleTreeManager>,
    avnu_client: AvnuClient,
    keccak_proof_gen: Arc<MerkleProofGenerator>,
    poseidon_proof_gen: Arc<MerkleProofGenerator>,
    relayer_private_key: String,
    poll_interval_secs: u64,
    mark_settled_max_retries: u32,
}

impl SettlementCoordinator {
    pub fn new(
        db: Arc<Database>,
        evm_relayer: Arc<EvmRelayer>,
        starknet_relayer: Arc<StarkNetRelayer>,
        near_client: Arc<NearClient>,
        merkle_manager: Arc<MerkleTreeManager>,
        relayer_private_key: String,
        poll_interval_secs: u64,
        mark_settled_max_retries: u32,
    ) -> Self {
        Self {
            db,
            evm_relayer,
            starknet_relayer,
            near_client,
            merkle_manager,
            avnu_client: AvnuClient::new(),
            keccak_proof_gen: Arc::new(MerkleProofGenerator::new(HashType::Keccak256)),
            poseidon_proof_gen: Arc::new(MerkleProofGenerator::new(HashType::Poseidon)),
            relayer_private_key,
            poll_interval_secs,
            mark_settled_max_retries,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!(
            "Settlement coordinator started ({}s interval)",
            self.poll_interval_secs
        );
        loop {
            if let Err(e) = self.process_pending_settlements().await {
                error!("Settlement processing error: {}", e);
            }
            sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    async fn process_pending_settlements(&self) -> Result<()> {
        let pending = self
            .db
            .get_intents_by_status(IntentStatus::TokensDelivered)?;
        if pending.is_empty() {
            return Ok(());
        }
        info!("Processing {} pending settlements", pending.len());
        for intent in &pending {
            if let Err(e) = self.process_single_settlement(intent).await {
                error!("Failed to settle intent {}: {}", &intent.id[..10], e);
            }
        }
        Ok(())
    }

    async fn process_single_settlement(&self, intent: &ShadowIntent) -> Result<()> {
        info!("Processing settlement for intent {}", &intent.id[..10]);

        // Step 1: Verify NEAR bridge completed
        let deposit_address = intent
            .deposit_address
            .as_ref()
            .ok_or_else(|| anyhow!("No deposit address for intent {}", &intent.id[..10]))?;

        let near_result = self
            .near_client
            .get_status(deposit_address)
            .await
            .context("Failed to get NEAR status")?;

        if near_result.status != NearSwapStatus::Success {
            warn!(
                "NEAR bridge not completed for {} (status: {:?})",
                &intent.id[..10],
                near_result.status
            );
            return Ok(());
        }

        // Step 2: Get destination tx, verify token delivery, and cache delivered values.
        let dest_tx_hash = near_result
            .destination_tx_hashes
            .first()
            .ok_or_else(|| anyhow!("No destination tx hash"))?;

        let (delivered_token, delivered_amount) = self
            .verify_token_delivery(intent, dest_tx_hash)
            .await
            .context("Token delivery verification failed")?
            .ok_or_else(|| anyhow!("Token delivery not verified for tx {}", dest_tx_hash))?;

        // Step 3: Nullifier pre-check
        let nullifier_used = self.check_nullifier_used(intent).await?;
        if nullifier_used {
            warn!(
                "Nullifier already used for intent {} — skipping settlement",
                &intent.id[..10]
            );
            self.db
                .update_intent_status(&intent.id, IntentStatus::Failed)?;
            return Err(anyhow!("Nullifier already used — possible replay"));
        }

        // Step 4: Owner rescue fork — after RESCUE_TIMEOUT_SECS stuck at TokensDelivered
        let stuck_secs = (Utc::now().timestamp() as u64).saturating_sub(intent.updated_at);
        if stuck_secs > RESCUE_TIMEOUT_SECS {
            warn!(
                "Intent {} stuck {}s — attempting owner rescue",
                &intent.id[..10],
                stuck_secs
            );
            return self
                .rescue(intent, dest_tx_hash, &delivered_token, &delivered_amount)
                .await;
        }

        // Step 5: Off-chain Merkle proof verification
        let proof_valid = self
            .verify_proof_offchain(intent)
            .await
            .context("Off-chain proof verification failed")?;

        if !proof_valid {
            return Err(anyhow!("Invalid Merkle proof - rejecting settlement"));
        }
        info!("Off-chain proof verification passed");

        // Step 6: Decrypt ECIES params
        let encrypted_nullifier = intent.encrypted_nullifier.as_ref().ok_or_else(|| {
            anyhow!(
                "Missing encrypted_nullifier for intent {}",
                &intent.id[..10]
            )
        })?;

        let decrypted_nullifier = Zeroizing::new(
            decrypt_with_ecies(encrypted_nullifier, &self.relayer_private_key)
                .context("Failed to decrypt nullifier")?,
        );
        let decrypted_recipient = Zeroizing::new(
            decrypt_with_ecies_utf8(&intent.encrypted_recipient, &self.relayer_private_key)
                .context("Failed to decrypt recipient")?,
        );

        info!(
            "Privacy params decrypted for intent {} (recipient: {}...)",
            &intent.id[..10],
            &decrypted_recipient[..10.min(decrypted_recipient.len())]
        );

        // Step 7: Settle on destination chain
        self.settle_on_destination(
            intent,
            decrypted_nullifier.as_str(),
            decrypted_recipient.as_str(),
            dest_tx_hash,
            &delivered_token,
            &delivered_amount,
        )
        .await
        .map_err(|e| {
            error!(
                "settle_on_destination full error for {}: {:#}",
                &intent.id[..10],
                e
            );
            e
        })
        .context("Destination settlement failed")?;

        self.db
            .update_intent_status(&intent.id, IntentStatus::Settled)?;

        // Step 8: Mark settled on source chain with retry
        self.mark_settled_with_retry(intent).await;

        info!("Settlement complete for {}", &intent.id[..10]);
        Ok(())
    }

    // ==============================================================
    //  Rescue: bypass Merkle proof, direct transfer only (no swap).
    // ==============================================================

    async fn rescue(
        &self,
        intent: &ShadowIntent,
        dest_tx_hash: &str,
        delivered_token: &str,
        delivered_amount: &str,
    ) -> Result<()> {
        let recipient = Zeroizing::new(
            decrypt_with_ecies_utf8(&intent.encrypted_recipient, &self.relayer_private_key)
                .context("Failed to decrypt recipient for rescue")?,
        );

        let rescue_tx_hash = match intent.dest_chain {
            ChainId::Evm => {
                info!(
                    "EVM rescue: token={} amount={} recipient={}...",
                    delivered_token,
                    delivered_amount,
                    &recipient[..10.min(recipient.len())]
                );

                let tx = self
                    .evm_relayer
                    .rescue_tokens(delivered_token, recipient.as_str(), delivered_amount)
                    .await
                    .map_err(|e| {
                        error!(
                            "EVM rescue failed for {}: token={} amount={} — {:#}",
                            &intent.id[..10],
                            delivered_token,
                            delivered_amount,
                            e
                        );
                        e
                    })
                    .context("EVM rescue failed")?;

                format!("{:?}", tx)
            }

            ChainId::Starknet => {
                let recipient_felt = Felt::from_hex(recipient.as_str())
                    .map_err(|e| anyhow!("Invalid recipient hex: {}", e))?;
                let token_felt = Felt::from_hex(delivered_token)
                    .map_err(|e| anyhow!("Invalid delivered token hex: {}", e))?;
                let amount = u256_to_felt_pair(delivered_amount)?;

                info!(
                    "StarkNet rescue: token={} amount={} recipient={}...",
                    delivered_token,
                    delivered_amount,
                    &recipient[..10.min(recipient.len())]
                );

                let tx = self
                    .starknet_relayer
                    .rescue_tokens(token_felt, recipient_felt, amount)
                    .await
                    .map_err(|e| {
                        error!(
                            "StarkNet rescue failed for {}: token={} amount={} — {:#}",
                            &intent.id[..10],
                            delivered_token,
                            delivered_amount,
                            e
                        );
                        e
                    })
                    .context("StarkNet rescue failed")?;

                format!("0x{:064x}", tx)
            }
        };

        if let Err(e) = self.db.update_intent_settle_tx(&intent.id, &rescue_tx_hash) {
            warn!("Failed to record rescue tx hash: {}", e);
        }
        self.db
            .update_intent_status(&intent.id, IntentStatus::Settled)?;
        self.mark_settled_with_retry(intent).await;

        info!(
            "Intent {} rescued on {} (tx={})",
            &intent.id[..10],
            if intent.dest_chain == ChainId::Evm { "EVM" } else { "StarkNet" },
            &dest_tx_hash[..18.min(dest_tx_hash.len())]
        );
        Ok(())
    }

    // ==============================================================
    //  Direct StarkNet settlement — no AVNU swap.
    // ==============================================================

    async fn settle_direct(
        &self,
        intent: &ShadowIntent,
        decrypted_nullifier: &str,
        decrypted_recipient: &str,
        delivered_token: &str,
        delivered_amount: &str,
    ) -> Result<()> {
        let intent_id =
            Felt::from_hex(&intent.id).map_err(|e| anyhow!("Invalid intent id hex: {}", e))?;
        let nullifier = Felt::from_hex(decrypted_nullifier)
            .map_err(|e| anyhow!("Invalid decrypted nullifier hex: {}", e))?;
        let recipient = Felt::from_hex(decrypted_recipient)
            .map_err(|e| anyhow!("Invalid recipient hex: {}", e))?;
        let token = Felt::from_hex(delivered_token)
            .map_err(|e| anyhow!("Invalid delivered token hex: {}", e))?;
        let amount = u256_to_felt_pair(delivered_amount)?;

        self.starknet_relayer
            .settle_and_release(
                intent_id,
                nullifier,
                recipient,
                token,
                amount,
                Felt::ZERO,
                (Felt::ZERO, Felt::ZERO),
                (Felt::ZERO, Felt::ZERO),
                &[],
            )
            .await?;

        Ok(())
    }

    // ==============================================================
    //  Nullifier check
    // ==============================================================

    async fn check_nullifier_used(&self, intent: &ShadowIntent) -> Result<bool> {
        match intent.dest_chain {
            ChainId::Evm => self
                .evm_relayer
                .is_nullifier_used(&intent.nullifier_hash)
                .await
                .context("Nullifier check failed on EVM"),
            ChainId::Starknet => {
                let encrypted_nullifier = intent
                    .encrypted_nullifier
                    .as_ref()
                    .ok_or_else(|| anyhow!("Missing encrypted_nullifier"))?;
                let decrypted = decrypt_with_ecies(encrypted_nullifier, &self.relayer_private_key)
                    .context("Failed to decrypt nullifier for nullifier check")?;
                let nullifier_felt = Felt::from_hex(&decrypted)
                    .map_err(|e| anyhow!("Invalid nullifier hex: {}", e))?;
                self.starknet_relayer
                    .is_nullifier_used(nullifier_felt)
                    .await
                    .context("Nullifier check failed on StarkNet")
            }
        }
    }

    // ==============================================================
    //  Off-chain Merkle proof verification
    // ==============================================================

    async fn verify_proof_offchain(&self, intent: &ShadowIntent) -> Result<bool> {
        info!("Verifying Merkle proof off-chain");

        let commitment = &intent.commitment;

        let (leaves, root) = match intent.source_chain {
            ChainId::Evm => {
                let leaves = self.merkle_manager.get_leaves_snapshot(ChainId::Evm).await;
                let root = self.evm_relayer.get_merkle_root().await?;
                (leaves, root)
            }
            ChainId::Starknet => {
                let leaves = self
                    .merkle_manager
                    .get_leaves_snapshot(ChainId::Starknet)
                    .await;
                let root = self.starknet_relayer.get_merkle_root().await?;
                (leaves, format!("0x{:064x}", root))
            }
        };

        let (proof, leaf_index, computed_root) = match intent.source_chain {
            ChainId::Evm => self.keccak_proof_gen.generate_proof(&leaves, commitment)?,
            ChainId::Starknet => self
                .poseidon_proof_gen
                .generate_proof(&leaves, commitment)?,
        };

        info!(
            "Proof generated: {} siblings, index {}",
            proof.len(),
            leaf_index
        );
        info!("Expected root:  {}", &root[..root.len().min(18)]);
        info!("Computed root:  {}", &computed_root[..computed_root.len().min(18)]);

        if computed_root.to_lowercase() != root.to_lowercase() {
            error!(
                "Root mismatch! Expected: {}, Computed: {}",
                root, computed_root
            );
            return Ok(false);
        }

        let verified = match intent.source_chain {
            ChainId::Evm => self
                .keccak_proof_gen
                .verify_proof(&proof, &root, commitment, leaf_index)?,
            ChainId::Starknet => self
                .poseidon_proof_gen
                .verify_proof(&proof, &root, commitment, leaf_index)?,
        };

        if !verified {
            error!("Proof verification failed!");
            return Ok(false);
        }

        info!("Proof verification successful");
        Ok(true)
    }

    // ==============================================================
    //  Token delivery verification
    // ==============================================================

    async fn verify_token_delivery(
        &self,
        intent: &ShadowIntent,
        tx_hash: &str,
    ) -> Result<Option<(String, String)>> {
        info!(
            "Verifying token delivery: {}",
            &tx_hash[..18.min(tx_hash.len())]
        );

        match intent.dest_chain {
            ChainId::Evm => {
                match self
                    .evm_relayer
                    .get_delivered_token_and_amount(tx_hash)
                    .await
                {
                    Ok((token, amount)) => Ok(Some((token, amount))),
                    Err(e) => {
                        warn!(
                            "EVM delivery verification failed for {}: {}",
                            &tx_hash[..18.min(tx_hash.len())],
                            e
                        );
                        Ok(None)
                    }
                }
            }
            ChainId::Starknet => self.verify_starknet_transfer(tx_hash).await,
        }
    }

    async fn verify_starknet_transfer(&self, tx_hash: &str) -> Result<Option<(String, String)>> {
        use starknet::core::types::TransactionReceipt;
        use starknet::core::utils::get_selector_from_name;

        let tx_hash_felt =
            Felt::from_hex(tx_hash).map_err(|e| anyhow!("Invalid StarkNet tx hash: {}", e))?;

        let receipt = match self
            .starknet_relayer
            .wait_for_transaction(tx_hash_felt)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("StarkNet transfer verification failed: {}", e);
                return Ok(None);
            }
        };

        let events = match &receipt {
            TransactionReceipt::Invoke(r) => &r.events,
            _ => {
                warn!(
                    "StarkNet delivery TX is not an Invoke transaction: {}",
                    &tx_hash[..18.min(tx_hash.len())]
                );
                return Ok(None);
            }
        };

        let settlement_addr = Felt::from_hex(
            self.starknet_relayer
                .contract_address_hex()
                .trim_start_matches("0x"),
        )
        .map_err(|e| anyhow!("Invalid settlement contract address: {}", e))?;

        let transfer_selector =
            get_selector_from_name("Transfer").map_err(|e| anyhow!("Selector error: {}", e))?;

        let transfer_event = events.iter().find(|event| {
            event.keys.len() >= 1
                && event.keys[0] == transfer_selector
                && event.data.len() >= 4
                && event.data[1] == settlement_addr
        });

        match transfer_event {
            None => {
                warn!(
                    "No Transfer to settlement contract found in StarkNet TX: {}",
                    &tx_hash[..18.min(tx_hash.len())]
                );
                Ok(None)
            }
            Some(event) => {
                let token_hex = format!("0x{:064x}", event.from_address);
                let low = event.data[2].to_biguint();
                let high = event.data[3].to_biguint();
                let amount = (high << 128u32) + low;

                info!(
                    "StarkNet Transfer verified: token={} amount={} tx={}",
                    &token_hex,
                    &amount,
                    &tx_hash[..18.min(tx_hash.len())]
                );

                Ok(Some((token_hex, amount.to_string())))
            }
        }
    }

    // ==============================================================
    //  Settlement on destination chain
    // ==============================================================

    async fn settle_on_destination(
        &self,
        intent: &ShadowIntent,
        decrypted_nullifier: &str,
        decrypted_recipient: &str,
        dest_tx_hash: &str,
        delivered_token: &str,
        delivered_amount: &str,
    ) -> Result<()> {
        info!("Settling on destination chain");

        match intent.dest_chain {
            // ── EVM ───────────────────────────────────────────────────────────
            ChainId::Evm => {
                info!(
                    "settle_and_release EVM: token={} amount={}",
                    delivered_token, delivered_amount
                );

                self.evm_relayer
                    .settle_and_release(
                        &intent.id,
                        decrypted_nullifier,
                        decrypted_recipient,
                        delivered_token,
                        delivered_amount,
                    )
                    .await?;
            }

            // ── StarkNet ──────────────────────────────────────────────────────
            ChainId::Starknet => {
                // NEAR always delivers STRK on the StarkNet path.
                if delivered_token.to_lowercase() != STRK_ADDRESS {
                    return Err(anyhow!(
                        "NEAR delivered unexpected token on StarkNet path: {} (expected STRK {})",
                        delivered_token,
                        STRK_ADDRESS
                    ));
                }

                let dest_token_str: Option<&str> = match intent.dest_token.as_deref() {
                    None | Some("") => None,
                    Some(addr) => {
                        Felt::from_hex(addr).with_context(|| {
                            format!(
                                "intent.dest_token is not a valid StarkNet address: {}",
                                addr
                            )
                        })?;
                        Some(addr)
                    }
                };

                let needs_swap = dest_token_str
                    .map(|dt| dt.to_lowercase() != delivered_token.to_lowercase())
                    .unwrap_or(false);

                // (dest_token_felt, expected_dest_amount, min_dest_amount, routes_calldata)
                let (dest_token_felt, expected_dest_amount, min_dest_amount, routes_calldata): (
                    Felt,
                    (Felt, Felt),
                    (Felt, Felt),
                    Vec<Felt>,
                ) = if needs_swap {
                    let dest_token = dest_token_str.unwrap();
                    // taker = settlement contract (on-chain caller of multi_route_swap)
                    let taker = self.starknet_relayer.contract_address_hex();

                    let quote = match self
                        .avnu_client
                        .fetch_quote(delivered_token, dest_token, delivered_amount, &taker)
                        .await
                    {
                        Ok(q) => q,
                        Err(e) if e.to_string().starts_with("AVNU_DUST:") => {
                            warn!(
                                "Intent {}: dust amount, skipping swap — delivering {} as-is",
                                &intent.id[..10],
                                delivered_token
                            );
                            return self
                                .settle_direct(
                                    intent,
                                    decrypted_nullifier,
                                    decrypted_recipient,
                                    delivered_token,
                                    delivered_amount,
                                )
                                .await;
                        }
                        Err(e) => {
                            return Err(e).with_context(|| {
                                format!(
                                    "AVNU quote failed for intent {}: {} -> {}",
                                    &intent.id[..10],
                                    delivered_token,
                                    dest_token
                                )
                            });
                        }
                    };

                    let min_felt = quote.min_dest_amount_felts()?;
                    let expected_felt = quote.buy_amount_felts()?;
                    let dest_felt = Felt::from_hex(dest_token)
                        .map_err(|e| anyhow!("Invalid dest_token hex: {}", e))?;

                    info!(
                        "AVNU quote intent={}: buy={} min={} route_felts={}",
                        &intent.id[..10],
                        quote.buy_amount,
                        quote.min_dest_amount,
                        quote.routes_calldata.len()
                    );

                    (dest_felt, expected_felt, min_felt, quote.routes_calldata)
                } else {
                    (
                        Felt::ZERO,
                        (Felt::ZERO, Felt::ZERO),
                        (Felt::ZERO, Felt::ZERO),
                        vec![],
                    )
                };

                let intent_id = Felt::from_hex(&intent.id)
                    .map_err(|e| anyhow!("Invalid intent id hex: {}", e))?;
                let nullifier = Felt::from_hex(decrypted_nullifier)
                    .map_err(|e| anyhow!("Invalid decrypted nullifier hex: {}", e))?;
                let recipient = Felt::from_hex(decrypted_recipient)
                    .map_err(|e| anyhow!("Invalid recipient hex: {}", e))?;
                let token = Felt::from_hex(delivered_token)
                    .map_err(|e| anyhow!("Invalid delivered token hex: {}", e))?;
                let amount = u256_to_felt_pair(delivered_amount)?;

                info!(
                    "settle_and_release StarkNet intent={}: token={} amount={} swap={}",
                    &intent.id[..10],
                    delivered_token,
                    delivered_amount,
                    needs_swap
                );

                self.starknet_relayer
                    .settle_and_release(
                        intent_id,
                        nullifier,
                        recipient,
                        token,
                        amount,
                        dest_token_felt,
                        expected_dest_amount,
                        min_dest_amount,
                        &routes_calldata,
                    )
                    .await?;

                let _ = dest_tx_hash;
            }
        }

        Ok(())
    }

    // ==============================================================
    //  Mark settled on source chain with retry
    // ==============================================================

    async fn mark_settled_with_retry(&self, intent: &ShadowIntent) {
        for attempt in 1..=self.mark_settled_max_retries {
            match self.mark_settled_on_source(intent).await {
                Ok(()) => {
                    if let Err(e) = self
                        .db
                        .update_intent_status(&intent.id, IntentStatus::MarkedSettled)
                    {
                        error!("Failed to update status to MarkedSettled: {}", e);
                    }
                    return;
                }
                Err(e) => {
                    warn!(
                        "mark_settled attempt {}/{} failed for {}: {}",
                        attempt,
                        self.mark_settled_max_retries,
                        &intent.id[..10],
                        e
                    );
                    if attempt < self.mark_settled_max_retries {
                        sleep(Duration::from_secs(5 * attempt as u64)).await;
                    }
                }
            }
        }

        error!(
            "mark_settled failed after {} retries for {} — manual intervention required",
            self.mark_settled_max_retries,
            &intent.id[..10]
        );
        if let Err(e) = self
            .db
            .update_intent_status(&intent.id, IntentStatus::SettlementFailed)
        {
            error!("Failed to update status to SettlementFailed: {}", e);
        }
    }

    async fn mark_settled_on_source(&self, intent: &ShadowIntent) -> Result<()> {
        info!("Marking settled on source chain");

        match intent.source_chain {
            ChainId::Evm => {
                self.evm_relayer
                    .mark_settled(&intent.commitment, &intent.nullifier_hash)
                    .await?;
            }
            ChainId::Starknet => {
                let commitment_felt = Felt::from_hex(&intent.commitment)
                    .map_err(|e| anyhow!("Invalid commitment hex: {}", e))?;
                let nullifier_felt = Felt::from_hex(&intent.nullifier_hash)
                    .map_err(|e| anyhow!("Invalid nullifier hex: {}", e))?;

                self.starknet_relayer
                    .mark_settled(commitment_felt, nullifier_felt)
                    .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_offchain_proof_verification_rejects_invalid() {}

    #[tokio::test]
    async fn test_nullifier_hash_computation() {}
}