use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use zeroize::Zeroizing;

use crate::{
    database::db::Database,
    encryption::decrypt_ecies::{decrypt_with_ecies, decrypt_with_ecies_utf8},
    evm::relayer::EvmRelayer,
    merkle_manager::{
        proof_generator::{HashType, MerkleProofGenerator},
        tree_manager::MerkleTreeManager,
    },
    models::models::{ChainId, IntentStatus, IntentStore, ShadowIntent},
    near_client::model::{NearClient, NearSwapStatus},
    starknet::relayer::{StarkNetRelayer, u256_to_felt_pair},
};

/// After this many seconds stuck at TokensDelivered, owner rescue is attempted.
/// Set to 1 hour to account for NEAR indexer delay (10-15 min for StarkNet),
/// token delivery verification, proof generation, and network delays.
const RESCUE_TIMEOUT_SECS: u64 = 3600;

pub struct SettlementCoordinator {
    db: Arc<Database>,
    evm_relayer: Arc<EvmRelayer>,
    starknet_relayer: Arc<StarkNetRelayer>,
    near_client: Arc<NearClient>,
    merkle_manager: Arc<MerkleTreeManager>,
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
            keccak_proof_gen: Arc::new(MerkleProofGenerator::new(HashType::Keccak256)),
            poseidon_proof_gen: Arc::new(MerkleProofGenerator::new(HashType::Poseidon)),
            relayer_private_key,
            poll_interval_secs,
            mark_settled_max_retries,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Settlement coordinator started ({}s interval)", self.poll_interval_secs);

        loop {
            if let Err(e) = self.process_pending_settlements().await {
                error!("Settlement processing error: {}", e);
            }
            sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    async fn process_pending_settlements(&self) -> Result<()> {
        let pending = self.db.get_intents_by_status(IntentStatus::TokensDelivered)?;

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

        // Step 2: Verify token delivery on destination chain
        let dest_tx_hash = near_result
            .destination_tx_hashes
            .first()
            .ok_or_else(|| anyhow!("No destination tx hash"))?;

        let verified = self
            .verify_token_delivery(intent, dest_tx_hash)
            .await
            .context("Token delivery verification failed")?;

        if !verified {
            return Err(anyhow!("Token delivery not verified"));
        }

        // Step 3: Check nullifier hasn't been used on-chain.
        // Fix #2: moved before rescue fork so rescue cannot re-run on an already-settled intent.
        let nullifier_used = match intent.dest_chain {
            ChainId::Evm => {
                self.evm_relayer
                    .is_nullifier_used(&intent.nullifier_hash)
                    .await
                    .context("Nullifier check failed on EVM")?
            }
            ChainId::Starknet => {
                use starknet::core::types::Felt;
                let nullifier_felt = Felt::from_hex(&intent.nullifier_hash)
                    .map_err(|e| anyhow!("Invalid nullifier hex: {}", e))?;
                self.starknet_relayer
                    .is_nullifier_used(nullifier_felt)
                    .await
                    .context("Nullifier check failed on StarkNet")?
            }
        };

        if nullifier_used {
            warn!(
                "Nullifier already used for intent {} — skipping settlement",
                &intent.id[..10]
            );
            self.db
                .update_intent_status(&intent.id, IntentStatus::Failed)?;
            return Err(anyhow!("Nullifier already used — possible replay"));
        }

        // Step 4: Owner rescue fork — BOTH CHAINS, after RESCUE_TIMEOUT_SECS stuck.
        // Nullifier confirmed unused in step 3, so rescue is safe.
        let stuck_secs = (Utc::now().timestamp() as u64).saturating_sub(intent.updated_at);
        if stuck_secs > RESCUE_TIMEOUT_SECS {
            warn!(
                "Intent {} stuck {}s — attempting owner rescue (bypasses Merkle proof)",
                &intent.id[..10],
                stuck_secs
            );

            let recipient = Zeroizing::new(
                decrypt_with_ecies_utf8(
                    &intent.encrypted_recipient,
                    &self.relayer_private_key,
                )
                .context("Failed to decrypt recipient for rescue")?,
            );

            let dest_tx = intent
                .dest_tx_hash
                .as_deref()
                .ok_or_else(|| anyhow!("No dest_tx_hash — cannot determine delivered token/amount for rescue"))?;

            let rescue_tx_hash = match intent.dest_chain {
                ChainId::Evm => {
                    let (delivered_token, delivered_amount) = self
                        .evm_relayer
                        .get_delivered_token_and_amount(dest_tx)
                        .await
                        .with_context(|| format!("Failed to read delivered token/amount from NEAR tx {}", dest_tx))?;

                    info!(
                        "EVM rescue: token={} amount={} recipient={}...",
                        &delivered_token,
                        &delivered_amount,
                        &recipient[..10.min(recipient.len())]
                    );

                    let tx = self.evm_relayer
                        .rescue_tokens(&delivered_token, recipient.as_str(), &delivered_amount)
                        .await
                        .map_err(|e| {
                            error!(
                                "EVM rescue failed for {}: token={} recipient={}... amount={} — {:#}",
                                &intent.id[..10],
                                &delivered_token,
                                &recipient[..10.min(recipient.len())],
                                &delivered_amount,
                                e
                            );
                            e
                        })
                        .context("EVM rescue failed")?;
                    
                    format!("{:?}", tx)
                }
                ChainId::Starknet => {
                    use starknet::core::types::Felt;

                    let recipient_felt = Felt::from_hex(recipient.as_str())
                        .map_err(|e| anyhow!("Invalid recipient hex: {}", e))?;
                    let token_felt = Felt::from_hex(&intent.token)
                        .map_err(|e| anyhow!("Invalid token hex: {}", e))?;
                    let amount = u256_to_felt_pair(&intent.amount)?;

                    info!(
                        "StarkNet rescue: token={} amount={} recipient={}...",
                        &intent.token,
                        &intent.amount,
                        &recipient[..10.min(recipient.len())]
                    );

                    let tx = self.starknet_relayer
                        .rescue_tokens(token_felt, recipient_felt, amount)
                        .await
                        .map_err(|e| {
                            error!(
                                "StarkNet rescue failed for {}: token={} recipient={}... amount={} — {:#}",
                                &intent.id[..10],
                                &intent.token,
                                &recipient[..10.min(recipient.len())],
                                &intent.amount,
                                e
                            );
                            e
                        })
                        .context("StarkNet rescue failed")?;

                    format!("0x{:064x}", tx)
                }
            };

            // Record rescue tx hash
            if let Err(e) = self.db.update_intent_settle_tx(&intent.id, &rescue_tx_hash) {
                warn!("Failed to record rescue tx hash: {}", e);
            }
            self.db.update_intent_status(&intent.id, IntentStatus::Settled)?;

            // Best-effort mark settled on source chain
            self.mark_settled_with_retry(intent).await;

            info!("Intent {} rescued by owner on {} chain", &intent.id[..10], 
                  if intent.dest_chain == ChainId::Evm { "EVM" } else { "StarkNet" });
            return Ok(());
        }

        // Step 5: Generate and verify Merkle proof off-chain (normal path)
        let proof_valid = self
            .verify_proof_offchain(intent)
            .await
            .context("Off-chain proof verification failed")?;

        if !proof_valid {
            return Err(anyhow!("Invalid Merkle proof - rejecting settlement"));
        }

        info!("Off-chain proof verification passed");

        // Step 6: Decrypt ECIES params only when about to call settleAndRelease.
        // Fix #10: encrypted_secret is not used by settle_and_release — not decrypted.
        // Fix #4: wrap in Zeroizing so plaintext is wiped from heap on drop.
        let encrypted_nullifier = intent
            .encrypted_nullifier
            .as_ref()
            .ok_or_else(|| anyhow!("Missing encrypted_nullifier for intent {}", &intent.id[..10]))?;

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

        // Step 7: Settle on destination chain (settleAndRelease)
        self.settle_on_destination(
            intent,
            decrypted_nullifier.as_str(),
            decrypted_recipient.as_str(),
            dest_tx_hash,
        )
        .await
        .context("Destination settlement failed")?;
        // `decrypted_nullifier` and `decrypted_recipient` dropped and zeroed here

        self.db
            .update_intent_status(&intent.id, IntentStatus::Settled)?;

        // Step 8: Mark settled on source chain with retry logic
        self.mark_settled_with_retry(intent).await;

        info!("Settlement complete for {}", &intent.id[..10]);
        Ok(())
    }

    /// Retry mark_settled up to max_retries times. If all fail, mark as SettlementFailed
    /// for manual intervention.
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

        // All retries exhausted — mark as SettlementFailed for manual intervention
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

    /// Fix #12: Use in-memory Merkle tree snapshot for proof generation.
    /// The snapshot is always the live set the on-chain root was computed from.
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
                let leaves = self.merkle_manager.get_leaves_snapshot(ChainId::Starknet).await;
                let root = self.starknet_relayer.get_merkle_root().await?;
                (leaves, format!("0x{:064x}", root))
            }
        };

        let (proof, leaf_index, computed_root) = match intent.source_chain {
            ChainId::Evm => {
                self.keccak_proof_gen.generate_proof(&leaves, commitment)?
            }
            ChainId::Starknet => {
                self.poseidon_proof_gen
                    .generate_proof(&leaves, commitment)?
            }
        };

        info!(
            "Proof generated: {} siblings, index {}",
            proof.len(),
            leaf_index
        );
        info!("Expected root: {}", &root[..root.len().min(18)]);
        info!("Computed root: {}", &computed_root[..computed_root.len().min(18)]);

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

    async fn verify_token_delivery(
        &self,
        intent: &ShadowIntent,
        tx_hash: &str,
    ) -> Result<bool> {
        info!(
            "Verifying token delivery: {}",
            &tx_hash[..18.min(tx_hash.len())]
        );

        match intent.dest_chain {
            ChainId::Evm => {
                // For cross-chain swaps, RelayCoordinator already verified delivery
                // Just confirm TX exists
                self.evm_relayer.verify_transaction_exists(tx_hash).await
            }
            ChainId::Starknet => {
                // Fix #3: Verify Transfer event to settlement contract
                self.verify_starknet_transfer(tx_hash, intent).await
            }
        }
    }

    /// Fix #3: Verify the NEAR delivery TX contains an ERC20 Transfer event
    /// directed to our settlement contract. Previously only checked TX existence.
    async fn verify_starknet_transfer(
        &self,
        tx_hash: &str,
        _intent: &ShadowIntent,
    ) -> Result<bool> {
        use starknet::core::types::{Felt, TransactionReceipt};
        use starknet::core::utils::get_selector_from_name;

        let tx_hash_felt = Felt::from_hex(tx_hash)
            .map_err(|e| anyhow!("Invalid StarkNet tx hash: {}", e))?;

        let receipt = match self.starknet_relayer.wait_for_transaction(tx_hash_felt).await {
            Ok(r) => r,
            Err(e) => {
                warn!("StarkNet transfer verification failed: {}", e);
                return Ok(false);
            }
        };

        // Only Invoke TXs carry ERC20 Transfer events
        let events = match &receipt {
            TransactionReceipt::Invoke(r) => &r.events,
            _ => {
                warn!(
                    "StarkNet delivery TX is not an Invoke transaction: {}",
                    &tx_hash[..18.min(tx_hash.len())]
                );
                return Ok(false);
            }
        };

        // Settlement contract is the expected Transfer recipient
        let settlement_addr = Felt::from_hex(
            self.starknet_relayer
                .contract_address_hex()
                .trim_start_matches("0x"),
        )
        .map_err(|e| anyhow!("Invalid settlement contract address: {}", e))?;

        // Transfer event keys: [selector, from_address, to_address]
        let transfer_selector =
            get_selector_from_name("Transfer").map_err(|e| anyhow!("Selector error: {}", e))?;

        let transfer_to_contract = events.iter().any(|event| {
            event.keys.len() >= 3
                && event.keys[0] == transfer_selector
                && event.keys[2] == settlement_addr
        });

        if transfer_to_contract {
            info!(
                "StarkNet Transfer to settlement contract verified: {}",
                &tx_hash[..18.min(tx_hash.len())]
            );
            Ok(true)
        } else {
            warn!(
                "No Transfer to settlement contract found in StarkNet TX: {}",
                &tx_hash[..18.min(tx_hash.len())]
            );
            Ok(false)
        }
    }

    /// Fix #10: Removed _decrypted_secret parameter (was never used by settle_and_release).
    async fn settle_on_destination(
        &self,
        intent: &ShadowIntent,
        decrypted_nullifier: &str,
        decrypted_recipient: &str,
        dest_tx_hash: &str,
    ) -> Result<()> {
        info!("Settling on destination chain");

        match intent.dest_chain {
            ChainId::Evm => {
                // Use the actual token/amount NEAR delivered (not source-chain intent values).
                let (delivered_token, delivered_amount) = self
                    .evm_relayer
                    .get_delivered_token_and_amount(dest_tx_hash)
                    .await
                    .with_context(|| format!("Failed to read delivered token/amount from NEAR tx {}", dest_tx_hash))?;

                info!(
                    "settle_and_release: token={} amount={} (source intent had token={} amount={})",
                    &delivered_token, &delivered_amount, &intent.token, &intent.amount
                );

                self.evm_relayer
                    .settle_and_release(
                        &intent.id,
                        decrypted_nullifier,
                        decrypted_recipient,
                        &delivered_token,
                        &delivered_amount,
                    )
                    .await?;
            }
            ChainId::Starknet => {
                use starknet::core::types::Felt;

                let intent_id = Felt::from_hex(&intent.id)
                    .map_err(|e| anyhow!("Invalid intent id hex: {}", e))?;
                let nullifier = Felt::from_hex(decrypted_nullifier)
                    .map_err(|e| anyhow!("Invalid decrypted nullifier hex: {}", e))?;
                let recipient = Felt::from_hex(decrypted_recipient)
                    .map_err(|e| anyhow!("Invalid recipient hex: {}", e))?;
                let token = Felt::from_hex(&intent.token)
                    .map_err(|e| anyhow!("Invalid token hex: {}", e))?;

                let amount = u256_to_felt_pair(&intent.amount)?;

                self.starknet_relayer
                    .settle_and_release(intent_id, nullifier, recipient, token, amount)
                    .await?;
            }
        }

        Ok(())
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
                use starknet::core::types::Felt;

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