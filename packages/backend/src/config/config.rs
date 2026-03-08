use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::env;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub evm: EvmConfig,
    pub starknet: StarkNetConfig,
    pub near: NearConfig,
    pub relay: RelayConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub hmac_secret: String,
    pub relayer_private_key: String,
    pub timestamp_tolerance_secs: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EvmConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub private_key: String,
    pub owner_private_key: Option<String>,
    pub intent_pool_address: String,
    pub settlement_address: String,
    pub chain_id: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StarkNetConfig {
    pub rpc_url: String,
    pub private_key: String,
    pub account_address: Option<String>,
    pub contract_address: Option<String>,
    /// StarkNet network chain ID for starknet-rs (e.g., "SN_MAIN" or "SN_SEPOLIA")
    pub chain_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NearConfig {
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RelayConfig {
    pub relay_poll_interval_secs: u64,
    pub settlement_poll_interval_secs: u64,
    pub root_sync_interval_secs: u64,
    pub mark_settled_max_retries: u32,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(AppConfig {
            server: ServerConfig {
                host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: env::var("PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .map_err(|e| anyhow!("Invalid PORT: {}", e))?,
                hmac_secret: env::var("HMAC_SECRET")
                    .map_err(|_| anyhow!("HMAC_SECRET must be set"))?,
                relayer_private_key: env::var("RELAYER_PRIVATE_KEY")
                    .map_err(|_| anyhow!("RELAYER_PRIVATE_KEY must be set"))?,
                timestamp_tolerance_secs: env::var("TIMESTAMP_TOLERANCE_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
            },
            evm: EvmConfig {
                rpc_url: env::var("EVM_RPC_URL").map_err(|_| anyhow!("EVM_RPC_URL must be set"))?,
                ws_url: env::var("EVM_WS_URL").ok(),
                private_key: env::var("EVM_PRIVATE_KEY")
                    .map_err(|_| anyhow!("EVM_PRIVATE_KEY must be set"))?,
                owner_private_key: env::var("EVM_OWNER_PRIVATE_KEY").ok(),
                intent_pool_address: env::var("EVM_INTENT_POOL_ADDRESS")
                    .map_err(|_| anyhow!("EVM_INTENT_POOL_ADDRESS must be set"))?,
                settlement_address: env::var("EVM_SETTLEMENT_ADDRESS")
                    .map_err(|_| anyhow!("EVM_SETTLEMENT_ADDRESS must be set"))?,
                chain_id: env::var("EVM_CHAIN_ID")
                    .map_err(|_| {
                        anyhow!(
                            "EVM_CHAIN_ID must be set (e.g., 1 for mainnet, 11155111 for sepolia)"
                        )
                    })?
                    .parse()
                    .map_err(|e| anyhow!("Invalid EVM_CHAIN_ID: {}", e))?,
            },
            starknet: StarkNetConfig {
                rpc_url: env::var("STARKNET_RPC_URL")
                    .map_err(|_| anyhow!("STARKNET_RPC_URL must be set"))?,
                private_key: env::var("STARKNET_PRIVATE_KEY")
                    .map_err(|_| anyhow!("STARKNET_PRIVATE_KEY must be set"))?,
                account_address: env::var("STARKNET_ACCOUNT_ADDRESS").ok(),
                contract_address: env::var("STARKNET_CONTRACT_ADDRESS").ok(),
                chain_id: env::var("STARKNET_CHAIN_ID").unwrap_or_else(|_| "SN_MAIN".to_string()),
            },
            near: NearConfig {
                api_key: env::var("NEAR_API_KEY").ok(),
            },
            relay: RelayConfig {
                relay_poll_interval_secs: env::var("RELAY_POLL_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                settlement_poll_interval_secs: env::var("SETTLEMENT_POLL_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                root_sync_interval_secs: env::var("ROOT_SYNC_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
                mark_settled_max_retries: env::var("MARK_SETTLED_MAX_RETRIES")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5),
            },
        })
    }
}
