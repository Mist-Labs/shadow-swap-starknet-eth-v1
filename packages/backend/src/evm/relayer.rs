use anyhow::{anyhow, Context, Result};
use ethers::{
    contract::abigen,
    core::types::{Address, H256, U256},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
};
use std::sync::Arc;
use tracing::{info, warn};

use crate::models::models::{ChainId, RemoteRootSnapshot};

abigen!(
    ShadowSettlementContract,
    r#"[
        function addToPendingBatch(bytes32 commitment, bytes32 nearIntentsId, bytes32 viewKey) external
        function processBatchIfTimeout() external
        function markSettled(bytes32 commitment, bytes32 nullifierHash) external
        function syncMerkleRoot(string calldata chainId, bytes32 root, uint256 leafCount) external
        function verifyRemoteRoot(string calldata chainId, uint256 snapshotIndex) external
        function settleAndRelease(bytes32 intentId, bytes32 nullifierHash, address recipient, address token, uint256 amount) external
        function rescueTokens(address token, address to, uint256 amount) external
        function getMerkleRoot() external view returns (bytes32)
        function isNullifierUsed(bytes32 nullifierHash) external view returns (bool)
        function commitmentExists(bytes32 commitment) external view returns (bool)
        function getLatestRemoteRoot(string calldata chainId) external view returns (bytes32 root, uint256 leafCount, uint64 syncedAt, bool verified)
        function getRemoteRootCount(string calldata chainId) external view returns (uint256)
        function getPendingBatchInfo() external view returns (uint256 count, uint64 firstSubmissionTime, uint256 timeRemaining)
        function setTokenWhitelist(address token, bool whitelisted) external
        function pause() external
        function unpause() external
        event IntentSettled(bytes32 indexed intentId, bytes32 indexed nullifierHash, address token, uint256 amount, uint64 timestamp)
        event Transfer(address indexed from, address indexed to, uint256 value)
    ]"#
);

type SignedContract = ShadowSettlementContract<SignerMiddleware<Provider<Http>, LocalWallet>>;

pub struct EvmRelayer {
    provider: Arc<Provider<Http>>,
    contract: Option<SignedContract>,
    owner_contract: Option<SignedContract>,
    contract_address: Address,
    chain_id: ChainId,
}

impl EvmRelayer {
    pub async fn new(
        rpc_url: &str,
        contract_address: &str,
        private_key: Option<&str>,
        owner_private_key: Option<&str>,
    ) -> Result<Self> {
        let provider =
            Provider::<Http>::try_from(rpc_url)?.interval(std::time::Duration::from_millis(2000));
        let provider = Arc::new(provider);

        let contract_address: Address = contract_address.parse()?;
        let chain_id = provider.get_chainid().await?.as_u64();

        let make_contract = |pk: &str| -> Result<SignedContract> {
            let wallet: LocalWallet = pk.parse::<LocalWallet>()?.with_chain_id(chain_id);
            let client = SignerMiddleware::new((*provider).clone(), wallet);
            Ok(ShadowSettlementContract::new(
                contract_address,
                Arc::new(client),
            ))
        };

        let contract = private_key.map(make_contract).transpose()?;
        let owner_contract = owner_private_key.map(make_contract).transpose()?;

        Ok(Self {
            provider,
            contract,
            owner_contract,
            contract_address,
            chain_id: ChainId::Evm,
        })
    }

    // ===== SOURCE SIDE =====

    pub async fn add_to_pending_batch(
        &self,
        commitment: &str,
        near_intents_id: &str,
        view_key: &str,
    ) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("📝 [EVM] Adding commitment to pending batch");

        let commitment_bytes: [u8; 32] = hex_to_bytes32(commitment)?;
        let near_id_bytes: [u8; 32] = hex_to_bytes32_padded(&near_intents_id.replace('-', ""))?;
        let view_key_bytes: [u8; 32] = hex_to_bytes32(view_key)?;

        let tx = contract
            .add_to_pending_batch(commitment_bytes, near_id_bytes, view_key_bytes)
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!("✅ [EVM] Commitment added, tx: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn process_batch_if_timeout(&self) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("⏰ [EVM] Processing batch (timeout reached)");

        let tx = contract
            .process_batch_if_timeout()
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!("✅ [EVM] Batch processed, tx: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn get_pending_batch_info(&self) -> Result<(u64, u64, u64)> {
        let contract =
            ShadowSettlementContract::new(self.contract_address, self.provider.clone());

        let (count, first_submission_time, time_remaining) =
            contract.get_pending_batch_info().call().await?;

        Ok((
            count.as_u64(),
            first_submission_time,
            time_remaining.as_u64(),
        ))
    }

    pub async fn mark_settled(&self, commitment: &str, nullifier_hash: &str) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("✅ [EVM] Marking intent as settled");

        let commitment_bytes: [u8; 32] = hex_to_bytes32(commitment)?;
        let nullifier_bytes: [u8; 32] = hex_to_bytes32(nullifier_hash)?;

        let tx = contract
            .mark_settled(commitment_bytes, nullifier_bytes)
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!("✅ Transaction confirmed: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    // ===== CROSS-CHAIN SYNC =====

    pub async fn sync_merkle_root(
        &self,
        chain_id: &str,
        root: &str,
        leaf_count: u64,
    ) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("🌉 [EVM] Syncing remote Merkle root");

        let root_bytes: [u8; 32] = hex_to_bytes32(root)?;

        let tx = contract
            .sync_merkle_root(chain_id.to_string(), root_bytes, U256::from(leaf_count))
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!("✅ Root synced, tx: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn verify_remote_root(&self, chain_id: &str, snapshot_index: u64) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("✅ [EVM] Verifying remote root");

        let tx = contract
            .verify_remote_root(chain_id.to_string(), U256::from(snapshot_index))
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!("✅ Root verified, tx: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    // ===== DESTINATION SIDE =====

    pub async fn settle_and_release(
        &self,
        intent_id: &str,
        nullifier_hash: &str,
        recipient: &str,
        token: &str,
        amount: &str,
    ) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        info!("💸 [EVM] Settling and releasing tokens");

        let intent_id_bytes: [u8; 32] = hex_to_bytes32(intent_id)?;
        let nullifier_bytes: [u8; 32] = hex_to_bytes32(nullifier_hash)?;
        let recipient_addr: Address = recipient.parse()?;
        let token_addr: Address = token.parse()?;
        let amount_u256: U256 = U256::from_dec_str(amount)?;

        let tx = contract
            .settle_and_release(
                intent_id_bytes,
                nullifier_bytes,
                recipient_addr,
                token_addr,
                amount_u256,
            )
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        info!(
            "✅ Settlement transaction confirmed: {:?}",
            tx.transaction_hash
        );
        Ok(tx.transaction_hash)
    }

    pub async fn rescue_tokens(&self, token: &str, recipient: &str, amount: &str) -> Result<H256> {
        let contract = self
            .owner_contract
            .as_ref()
            .ok_or_else(|| anyhow!("EVM_OWNER_PRIVATE_KEY not set — cannot rescue"))?;

        let token_addr: Address = token
            .parse()
            .with_context(|| format!("rescue_tokens: invalid token address: {}", token))?;
        let recipient_addr: Address = recipient
            .parse()
            .with_context(|| format!("rescue_tokens: invalid recipient address: {}", recipient))?;
        let amount_u256: U256 = U256::from_dec_str(amount)
            .map_err(|e| anyhow!("rescue_tokens: invalid amount '{}': {}", amount, e))?;

        let tx = contract
            .rescue_tokens(token_addr, recipient_addr, amount_u256)
            .send()
            .await
            .with_context(|| {
                format!(
                    "rescue_tokens: send failed (token={} recipient={} amount={})",
                    token, recipient, amount
                )
            })?
            .await
            .with_context(|| "rescue_tokens: waiting for receipt failed")?
            .ok_or_else(|| anyhow!("rescue_tokens: transaction dropped from mempool"))?;

        info!("🚨 Owner rescue tx confirmed: {:?}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    // ===== VIEW FUNCTIONS =====

    pub async fn get_merkle_root(&self) -> Result<String> {
        let contract = ShadowSettlementContract::new(self.contract_address, self.provider.clone());
        let root = contract.get_merkle_root().call().await?;
        Ok(format!("0x{}", hex::encode(root)))
    }

    pub async fn is_nullifier_used(&self, nullifier_hash: &str) -> Result<bool> {
        let contract = ShadowSettlementContract::new(self.contract_address, self.provider.clone());
        let nullifier_bytes: [u8; 32] = hex_to_bytes32(nullifier_hash)?;
        let used = contract.is_nullifier_used(nullifier_bytes).call().await?;
        Ok(used)
    }

    pub async fn commitment_exists(&self, commitment: &str) -> Result<bool> {
        let contract = ShadowSettlementContract::new(self.contract_address, self.provider.clone());
        let commitment_bytes: [u8; 32] = hex_to_bytes32(commitment)?;
        let exists = contract.commitment_exists(commitment_bytes).call().await?;
        Ok(exists)
    }

    pub async fn get_latest_remote_root(&self, chain_id: &str) -> Result<RemoteRootSnapshot> {
        let contract = ShadowSettlementContract::new(self.contract_address, self.provider.clone());

        let (root, leaf_count, synced_at, verified) = contract
            .get_latest_remote_root(chain_id.to_string())
            .call()
            .await?;

        Ok(RemoteRootSnapshot {
            chain_id: ChainId::Starknet,
            root: format!("0x{}", hex::encode(root)),
            leaf_count: leaf_count.as_u64(),
            synced_at,
            verified,
        })
    }

    pub async fn get_remote_root_count(&self, chain_id: &str) -> Result<u64> {
        let contract = ShadowSettlementContract::new(self.contract_address, self.provider.clone());
        let count = contract
            .get_remote_root_count(chain_id.to_string())
            .call()
            .await?;
        Ok(count.as_u64())
    }

    // ===== TOKEN TRANSFER VERIFICATION =====

    pub async fn verify_transaction_exists(&self, tx_hash: &str) -> Result<bool> {
        // NEAR sometimes strips leading zeros from tx hashes — normalize to full 32 bytes
        let tx_hash = normalize_tx_hash(tx_hash);

        info!(
            "🔍 [EVM] Verifying TX exists: {}",
            &tx_hash[..18.min(tx_hash.len())]
        );

        let hash: H256 = tx_hash.parse()?;
        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await?
            .ok_or_else(|| anyhow!("Transaction receipt not found"))?;

        let success = receipt.status == Some(1.into());
        if success {
            info!("✅ [EVM] TX exists and succeeded");
        } else {
            warn!("❌ [EVM] TX exists but failed");
        }
        Ok(success)
    }

    pub async fn verify_transfer_event(
        &self,
        tx_hash: &str,
        expected_token: &str,
        expected_amount: &str,
    ) -> Result<bool> {
        let tx_hash = normalize_tx_hash(tx_hash);

        info!(
            "🔍 [EVM] Verifying Transfer event in tx: {}",
            &tx_hash[..18.min(tx_hash.len())]
        );

        let hash: H256 = tx_hash.parse()?;
        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await?
            .ok_or_else(|| anyhow!("Transaction receipt not found"))?;

        let expected_token_addr: Address = expected_token.parse()?;
        let expected_amount_u256: U256 = U256::from_dec_str(expected_amount)?;
        let transfer_topic = ethers::core::utils::keccak256("Transfer(address,address,uint256)");

        for log in receipt.logs {
            if log.address != expected_token_addr {
                continue;
            }
            if log.topics.is_empty() || log.topics[0] != H256::from_slice(&transfer_topic) {
                continue;
            }
            if log.topics.len() >= 3 {
                let to_address = Address::from(log.topics[2]);
                if to_address != self.contract_address {
                    continue;
                }
                let amount = U256::from_big_endian(&log.data);
                if amount >= expected_amount_u256 {
                    info!("✅ Transfer verified: {} tokens to contract", amount);
                    return Ok(true);
                }
            }
        }

        warn!("❌ Transfer event not found or insufficient amount");
        Ok(false)
    }

    pub async fn get_delivered_token_and_amount(&self, tx_hash: &str) -> Result<(String, String)> {
        let tx_hash = normalize_tx_hash(tx_hash);

        info!(
            "🔍 [EVM] Reading delivered token/amount from tx: {}",
            &tx_hash[..18.min(tx_hash.len())]
        );

        let hash: H256 = tx_hash
            .parse()
            .with_context(|| format!("Invalid EVM tx hash: {}", tx_hash))?;

        let receipt = self
            .provider
            .get_transaction_receipt(hash)
            .await?
            .ok_or_else(|| anyhow!("Transaction receipt not found for {}", tx_hash))?;

        if receipt.status != Some(1.into()) {
            return Err(anyhow!("Transaction {} failed (status=0)", tx_hash));
        }

        let transfer_topic = ethers::core::utils::keccak256("Transfer(address,address,uint256)");

        for log in &receipt.logs {
            if log.topics.len() < 3 {
                continue;
            }
            if log.topics[0] != H256::from_slice(&transfer_topic) {
                continue;
            }
            let to_address = Address::from(log.topics[2]);
            if to_address != self.contract_address {
                continue;
            }
            let token_addr = log.address;
            let amount = U256::from_big_endian(&log.data);

            info!(
                "✅ [EVM] ERC20 delivered: token={:?} amount={} to settlement contract",
                token_addr, amount
            );
            return Ok((format!("{:?}", token_addr), amount.to_string()));
        }

        let tx = self
            .provider
            .get_transaction(hash)
            .await?
            .ok_or_else(|| anyhow!("Transaction not found: {}", tx_hash))?;

        if tx.to == Some(self.contract_address) && tx.value > U256::zero() {
            let eth_sentinel = "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE".to_string();
            info!(
                "✅ [EVM] Native ETH delivered: amount={} to settlement contract",
                tx.value
            );
            return Ok((eth_sentinel, tx.value.to_string()));
        }

        Err(anyhow!(
            "No Transfer event to settlement contract found in tx {}",
            tx_hash
        ))
    }

    // ===== ADMIN =====

    pub async fn set_token_whitelist(&self, token: &str, whitelisted: bool) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        let token_addr: Address = token.parse()?;
        let tx = contract
            .set_token_whitelist(token_addr, whitelisted)
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        Ok(tx.transaction_hash)
    }

    pub async fn pause(&self) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        let tx = contract
            .pause()
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        Ok(tx.transaction_hash)
    }

    pub async fn unpause(&self) -> Result<H256> {
        let contract = self
            .contract
            .as_ref()
            .ok_or_else(|| anyhow!("No wallet configured"))?;

        let tx = contract
            .unpause()
            .send()
            .await?
            .await?
            .ok_or_else(|| anyhow!("Transaction dropped"))?;

        Ok(tx.transaction_hash)
    }

    // ===== HEALTH CHECK =====

    pub async fn health_check(&self) -> Result<()> {
        let block = self.provider.get_block_number().await?;
        info!("EVM relayer healthy, current block: {}", block);
        Ok(())
    }

    pub fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    pub fn contract_address(&self) -> String {
        format!("{:?}", self.contract_address)
    }
}

// ===== HELPERS =====

/// NEAR sometimes returns tx hashes with leading zeros stripped.
/// Pad to full 66 chars (0x + 64 hex) before parsing as H256.
fn normalize_tx_hash(hash: &str) -> String {
    let stripped = hash.trim_start_matches("0x");
    if stripped.len() < 64 {
        format!("0x{:0>64}", stripped)
    } else {
        hash.to_string()
    }
}

fn hex_to_bytes32(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.trim_start_matches("0x");
    let bytes = hex::decode(hex).context("Invalid hex string")?;
    if bytes.len() != 32 {
        return Err(anyhow!("Expected 32 bytes, got {}", bytes.len()));
    }
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);
    Ok(array)
}

fn hex_to_bytes32_padded(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.trim_start_matches("0x");
    let bytes = hex::decode(hex).context("Invalid hex string")?;
    if bytes.len() > 32 {
        return Err(anyhow!("Expected <= 32 bytes, got {}", bytes.len()));
    }
    let mut array = [0u8; 32];
    array[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_tx_hash_short() {
        let short = "0x4a6192606b64cb9498a49a3ed293991951431930deb756790f6d68ac33eb0c4";
        let normalized = normalize_tx_hash(short);
        assert_eq!(normalized.len(), 66);
        assert_eq!(
            normalized,
            "0x04a6192606b64cb9498a49a3ed293991951431930deb756790f6d68ac33eb0c4"
        );
    }

    #[test]
    fn test_normalize_tx_hash_full() {
        let full = "0x04a6192606b64cb9498a49a3ed293991951431930deb756790f6d68ac33eb0c4";
        let normalized = normalize_tx_hash(full);
        assert_eq!(normalized, full);
    }

    #[test]
    fn test_hex_to_bytes32() {
        let hex = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let bytes = hex_to_bytes32(hex).unwrap();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes[0], 0x12);
        assert_eq!(bytes[31], 0xef);
    }

    #[test]
    fn test_hex_to_bytes32_without_prefix() {
        let hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let bytes = hex_to_bytes32(hex).unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_hex_to_bytes32_invalid_length() {
        let hex = "0x1234";
        let result = hex_to_bytes32(hex);
        assert!(result.is_err());
    }
}