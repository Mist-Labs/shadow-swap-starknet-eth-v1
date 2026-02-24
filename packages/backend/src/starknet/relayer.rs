use anyhow::{anyhow, Result};
use num_traits::ToPrimitive;
use starknet::{
    accounts::{Account, ExecutionEncoding, SingleOwnerAccount},
    core::{
        chain_id,
        types::{BlockId, BlockTag, Call, Felt, FunctionCall, TransactionReceipt},
        utils::get_selector_from_name,
    },
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider, Url},
    signers::{LocalWallet, SigningKey},
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::models::models::{ChainId, RemoteRootSnapshot};

/// Resolve a chain ID string (e.g., "SN_MAIN", "SN_SEPOLIA") to the starknet-rs Felt constant.
fn resolve_starknet_chain_id(chain_id_str: &str) -> Result<Felt> {
    match chain_id_str {
        "SN_MAIN" => Ok(chain_id::MAINNET),
        "SN_SEPOLIA" => Ok(chain_id::SEPOLIA),
        other => Felt::from_hex(other)
            .map_err(|e| anyhow!("Invalid STARKNET_CHAIN_ID '{}': {}", other, e)),
    }
}

pub struct StarkNetRelayer {
    provider: Arc<JsonRpcClient<HttpTransport>>,
    account: Option<SingleOwnerAccount<Arc<JsonRpcClient<HttpTransport>>, LocalWallet>>,
    contract_address: Felt,
    chain_id: ChainId,
}

impl StarkNetRelayer {
    pub async fn new(
        rpc_url: &str,
        contract_address: &str,
        private_key: Option<&str>,
        account_address: Option<&str>,
        starknet_chain_id: &str,
    ) -> Result<Self> {
        let provider = Arc::new(JsonRpcClient::new(HttpTransport::new(Url::parse(rpc_url)?)));

        let contract_address = Felt::from_hex(contract_address)
            .map_err(|e| anyhow!("Invalid contract address: {}", e))?;

        let sn_chain_id = resolve_starknet_chain_id(starknet_chain_id)?;

        let account = if let (Some(pk), Some(addr)) = (private_key, account_address) {
            let signer = LocalWallet::from(SigningKey::from_secret_scalar(
                Felt::from_hex(pk).map_err(|e| anyhow!("Invalid private key: {}", e))?,
            ));

            let account_addr =
                Felt::from_hex(addr).map_err(|e| anyhow!("Invalid account address: {}", e))?;

            Some(SingleOwnerAccount::new(
                provider.clone(),
                signer,
                account_addr,
                sn_chain_id,
                ExecutionEncoding::New,
            ))
        } else {
            None
        };

        Ok(Self {
            provider,
            account,
            contract_address,
            chain_id: ChainId::Starknet,
        })
    }

    // ==============================================================
    //                     SOURCE SIDE FUNCTIONS
    // ==============================================================

    pub async fn add_to_pending_batch(
        &self,
        commitment: Felt,
        near_intents_id: Felt,
        view_key: Felt,
    ) -> Result<Felt> {
        let account = self.require_account()?;

        info!("📝 [StarkNet] Adding commitment to pending batch");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("add_to_pending_batch")?,
            calldata: vec![commitment, near_intents_id, view_key],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!("✅ Transaction sent: 0x{:x}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn process_batch_if_timeout(&self) -> Result<Felt> {
        let account = self.require_account()?;

        info!("⏰ [StarkNet] Processing batch (timeout)");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("process_batch_if_timeout")?,
            calldata: vec![],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!("✅ Batch processed, tx: 0x{:x}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn mark_settled(&self, commitment: Felt, nullifier_hash: Felt) -> Result<Felt> {
        let account = self.require_account()?;

        info!("✅ [StarkNet] Marking intent as settled");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("mark_settled")?,
            calldata: vec![commitment, nullifier_hash],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!("✅ Transaction sent: 0x{:x}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    // ==============================================================
    //                   CROSS-CHAIN SYNC FUNCTIONS
    // ==============================================================

    pub async fn sync_merkle_root(
        &self,
        chain_id: Felt,
        root: Felt,
        leaf_count: (Felt, Felt),
    ) -> Result<Felt> {
        let account = self.require_account()?;

        info!("🌉 [StarkNet] Syncing remote Merkle root");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("sync_merkle_root")?,
            calldata: vec![chain_id, root, leaf_count.0, leaf_count.1],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!("✅ Root synced, tx: 0x{:x}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    pub async fn verify_remote_root(
        &self,
        chain_id: Felt,
        snapshot_index: (Felt, Felt),
    ) -> Result<Felt> {
        let account = self.require_account()?;

        info!("✅ [StarkNet] Verifying remote root");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("verify_remote_root")?,
            calldata: vec![chain_id, snapshot_index.0, snapshot_index.1],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!("✅ Root verified, tx: 0x{:x}", tx.transaction_hash);
        Ok(tx.transaction_hash)
    }

    // ==============================================================
    //                   DESTINATION SIDE FUNCTIONS
    // ==============================================================

    pub async fn settle_and_release(
        &self,
        intent_id: Felt,
        nullifier_hash: Felt,
        recipient: Felt,
        token: Felt,
        amount: (Felt, Felt),
    ) -> Result<Felt> {
        let account = self.require_account()?;

        info!("💸 [StarkNet] Settling and releasing tokens");

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("settle_and_release")?,
            calldata: vec![
                intent_id,
                nullifier_hash,
                recipient,
                token,
                amount.0,
                amount.1,
            ],
        };

        let tx = account.execute_v3(vec![call]).send().await?;

        info!(
            "✅ Settlement transaction sent: 0x{:x}",
            tx.transaction_hash
        );
        Ok(tx.transaction_hash)
    }

    // ==============================================================
    //                       VIEW FUNCTIONS
    // ==============================================================

    pub async fn get_merkle_root(&self) -> Result<Felt> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_merkle_root")?,
                    calldata: vec![],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.is_empty() {
            return Ok(Felt::ZERO);
        }

        Ok(result[0])
    }

    pub async fn is_nullifier_used(&self, nullifier_hash: Felt) -> Result<bool> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("is_nullifier_used")?,
                    calldata: vec![nullifier_hash],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        Ok(!result.is_empty() && result[0] != Felt::ZERO)
    }

    pub async fn commitment_exists(&self, commitment: Felt) -> Result<bool> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("commitment_exists")?,
                    calldata: vec![commitment],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        Ok(!result.is_empty() && result[0] != Felt::ZERO)
    }

    pub async fn get_intent(&self, commitment: Felt) -> Result<(Felt, u64, bool)> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_intent")?,
                    calldata: vec![commitment],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 3 {
            return Err(anyhow!("Invalid intent response: expected 3 fields"));
        }

        let commitment = result[0];
        let submitted_at = felt_to_u64(result[1])?;
        let settled = result[2] != Felt::ZERO;

        Ok((commitment, submitted_at, settled))
    }

    pub async fn get_pending_batch_info(&self) -> Result<(u64, u64, u64)> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_pending_batch_info")?,
                    calldata: vec![],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 3 {
            return Err(anyhow!("Invalid batch info response"));
        }

        Ok((
            felt_to_u64(result[0])?,
            felt_to_u64(result[1])?,
            felt_to_u64(result[2])?,
        ))
    }

    pub async fn is_relayer_authorized(&self, relayer: Felt) -> Result<bool> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("is_relayer_authorized")?,
                    calldata: vec![relayer],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        Ok(!result.is_empty() && result[0] != Felt::ZERO)
    }

    pub async fn is_root_verifier(&self, verifier: Felt) -> Result<bool> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("is_root_verifier")?,
                    calldata: vec![verifier],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        Ok(!result.is_empty() && result[0] != Felt::ZERO)
    }

    pub async fn is_token_whitelisted(&self, token: Felt) -> Result<bool> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("is_token_whitelisted")?,
                    calldata: vec![token],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        Ok(!result.is_empty() && result[0] != Felt::ZERO)
    }

    pub async fn get_latest_remote_root(&self, chain_id: Felt) -> Result<RemoteRootSnapshot> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_latest_remote_root")?,
                    calldata: vec![chain_id],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 5 {
            return Err(anyhow!(
                "Invalid remote root response: expected 5 fields, got {}",
                result.len()
            ));
        }

        parse_remote_root_snapshot(&result)
    }

    pub async fn get_latest_verified_remote_root(
        &self,
        chain_id: Felt,
    ) -> Result<(RemoteRootSnapshot, u64)> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name(
                        "get_latest_verified_remote_root",
                    )?,
                    calldata: vec![chain_id],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 7 {
            return Err(anyhow!(
                "Invalid verified root response: expected 7 fields, got {}",
                result.len()
            ));
        }

        let snapshot = parse_remote_root_snapshot(&result[..5])?;
        let index = felt_to_u64(result[5])?;

        Ok((snapshot, index))
    }

    pub async fn get_remote_root_snapshot(
        &self,
        chain_id: Felt,
        snapshot_index: (Felt, Felt),
    ) -> Result<RemoteRootSnapshot> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_remote_root_snapshot")?,
                    calldata: vec![chain_id, snapshot_index.0, snapshot_index.1],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 5 {
            return Err(anyhow!("Invalid snapshot response"));
        }

        parse_remote_root_snapshot(&result)
    }

    pub async fn get_remote_root_count(&self, chain_id: Felt) -> Result<u64> {
        let result = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contract_address,
                    entry_point_selector: get_selector_from_name("get_remote_root_count")?,
                    calldata: vec![chain_id],
                },
                BlockId::Tag(BlockTag::PreConfirmed),
            )
            .await?;

        if result.len() < 2 {
            return Err(anyhow!("Invalid root count response"));
        }

        felt_to_u64(result[0])
    }

    // ==============================================================
    //                      ADMIN FUNCTIONS
    // ==============================================================

    pub async fn update_batch_config(&self, new_batch_size: u64, new_timeout: u64) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("update_batch_config")?,
            calldata: vec![Felt::from(new_batch_size), Felt::from(new_timeout)],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn set_relayer_status(&self, relayer: Felt, authorized: bool) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("set_relayer_status")?,
            calldata: vec![relayer, bool_to_felt(authorized)],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn set_root_verifier_status(&self, verifier: Felt, authorized: bool) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("set_root_verifier_status")?,
            calldata: vec![verifier, bool_to_felt(authorized)],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn set_token_whitelist(&self, token: Felt, whitelisted: bool) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("set_token_whitelist")?,
            calldata: vec![token, bool_to_felt(whitelisted)],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn rescue_tokens(
        &self,
        token: Felt,
        to: Felt,
        amount: (Felt, Felt),
    ) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("rescue_tokens")?,
            calldata: vec![token, to, amount.0, amount.1],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn pause(&self) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("pause")?,
            calldata: vec![],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    pub async fn unpause(&self) -> Result<Felt> {
        let account = self.require_account()?;

        let call = Call {
            to: self.contract_address,
            selector: get_selector_from_name("unpause")?,
            calldata: vec![],
        };

        let tx = account.execute_v3(vec![call]).send().await?;
        Ok(tx.transaction_hash)
    }

    // ==============================================================
    //                      HELPER FUNCTIONS
    // ==============================================================

    fn require_account(
        &self,
    ) -> Result<&SingleOwnerAccount<Arc<JsonRpcClient<HttpTransport>>, LocalWallet>> {
        self.account
            .as_ref()
            .ok_or_else(|| anyhow!("No account configured"))
    }

    pub async fn wait_for_transaction(&self, tx_hash: Felt) -> Result<TransactionReceipt> {
        info!("⏳ Waiting for transaction confirmation...");

        let max_attempts = 60;

        for attempt in 1..=max_attempts {
            match self.provider.get_transaction_receipt(tx_hash).await {
                Ok(receipt_with_block) => {
                    let receipt = receipt_with_block.receipt;

                    let is_accepted = match &receipt {
                        TransactionReceipt::Invoke(r) => matches!(
                            r.execution_result,
                            starknet::core::types::ExecutionResult::Succeeded
                        ),
                        TransactionReceipt::Declare(r) => matches!(
                            r.execution_result,
                            starknet::core::types::ExecutionResult::Succeeded
                        ),
                        TransactionReceipt::DeployAccount(r) => matches!(
                            r.execution_result,
                            starknet::core::types::ExecutionResult::Succeeded
                        ),
                        TransactionReceipt::Deploy(r) => matches!(
                            r.execution_result,
                            starknet::core::types::ExecutionResult::Succeeded
                        ),
                        TransactionReceipt::L1Handler(r) => matches!(
                            r.execution_result,
                            starknet::core::types::ExecutionResult::Succeeded
                        ),
                    };

                    if is_accepted {
                        info!("✅ Transaction confirmed: 0x{:x}", tx_hash);
                        return Ok(receipt);
                    } else {
                        error!("❌ Transaction reverted: 0x{:x}", tx_hash);
                        return Err(anyhow!("Transaction reverted"));
                    }
                }
                Err(_) if attempt < max_attempts => {
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
                Err(e) => {
                    warn!(
                        "⚠️ Transaction not found after {} attempts: 0x{:x}",
                        attempt, tx_hash
                    );
                    return Err(anyhow!("Transaction lookup failed: {}", e));
                }
            }
        }

        Err(anyhow!(
            "Transaction not confirmed after {} attempts",
            max_attempts
        ))
    }

    pub async fn health_check(&self) -> Result<()> {
        self.provider
            .block_hash_and_number()
            .await
            .map_err(|e| anyhow!("StarkNet health check failed: {}", e))?;
        Ok(())
    }

    pub fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    pub fn contract_address_hex(&self) -> String {
        format!("0x{:x}", self.contract_address)
    }
}

// ==============================================================
//                      CONVERSION HELPERS
// ==============================================================

fn parse_remote_root_snapshot(result: &[Felt]) -> Result<RemoteRootSnapshot> {
    let root = felt_to_hex(result[0]);
    let leaf_count = felt_to_u128(result[1])? as u64;
    let synced_at = felt_to_u64(result[3])?;
    let verified = result[4] != Felt::ZERO;

    Ok(RemoteRootSnapshot {
        chain_id: ChainId::Evm,
        root,
        leaf_count,
        synced_at,
        verified,
    })
}

fn felt_to_u128(felt: Felt) -> Result<u128> {
    let bytes = felt.to_bytes_be();
    let slice: [u8; 16] = bytes[16..]
        .try_into()
        .map_err(|_| anyhow!("Failed to convert felt to u128"))?;
    Ok(u128::from_be_bytes(slice))
}

fn felt_to_u64(felt: Felt) -> Result<u64> {
    let bytes = felt.to_bytes_be();
    let slice: [u8; 8] = bytes[24..]
        .try_into()
        .map_err(|_| anyhow!("Failed to convert felt to u64"))?;
    Ok(u64::from_be_bytes(slice))
}

fn bool_to_felt(value: bool) -> Felt {
    if value {
        Felt::ONE
    } else {
        Felt::ZERO
    }
}

pub fn felt_to_hex(felt: Felt) -> String {
    format!("0x{:x}", felt)
}

pub fn u256_to_felt_pair(amount_str: &str) -> Result<(Felt, Felt)> {
    use num_bigint::BigUint;
    use num_traits::Num;

    let amount: BigUint = if amount_str.starts_with("0x") {
        BigUint::from_str_radix(&amount_str[2..], 16)?
    } else {
        amount_str.parse::<BigUint>()?
    };

    let mask_128: BigUint = BigUint::from(u128::MAX);
    let low_u128: u128 = (&amount & &mask_128).to_u128().unwrap_or(0);
    let high_u128: u128 = (&amount >> 128u32).to_u128().unwrap_or(0);

    let low = Felt::from(low_u128);
    let high = Felt::from(high_u128);

    Ok((low, high))
}