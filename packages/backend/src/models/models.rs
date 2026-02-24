use serde::{Deserialize, Serialize};
use std::fmt;

// ===== CHAIN TYPES =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainId {
    Evm,
    Starknet,
}

// ===== INTENT STATUS =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStatus {
    Pending,
    Committed,
    Batched,
    Registered,
    RootSynced,
    NearSubmitted,
    TokensDelivered,
    Filled,
    Settled,
    MarkedSettled,
    SettlementFailed,
    UserClaimed,
    Expired,
    Refunded,
    Failed,
}

// ===== INTENT MODEL =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowIntent {
    pub id: String,
    pub commitment: String,
    pub nullifier_hash: String,
    pub view_key: String,
    pub near_intents_id: String,
    pub source_chain: ChainId,
    pub dest_chain: ChainId,
    pub encrypted_recipient: String,
    pub token: String,
    pub amount: String,
    pub status: IntentStatus,
    pub created_at: u64,
    pub updated_at: u64,
    // NEAR 1Click — keyed by deposit_address (GET /v0/status?depositAddress=...)
    pub deposit_address: Option<String>,
    pub near_correlation_id: Option<String>,
    pub near_status: Option<String>,
    // Transaction hashes at each stage
    pub dest_tx_hash: Option<String>,
    pub settle_tx_hash: Option<String>,
    pub source_settle_tx_hash: Option<String>,
    // ECIES-encrypted privacy params (decrypted at settlement time)
    pub encrypted_secret: Option<String>,
    pub encrypted_nullifier: Option<String>,
}

// ===== REMOTE ROOT SNAPSHOT =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRootSnapshot {
    pub chain_id: ChainId,
    pub root: String,
    pub leaf_count: u64,
    pub synced_at: u64,
    pub verified: bool,
}

// ===== PROOF DATA =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub proof: Vec<String>,
    pub leaf_index: usize,
    pub root: String,
    pub leaf: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementProof {
    pub merkle_proof: MerkleProof,
    pub nullifier_hash: String,
    pub commitment: String,
}

// ===== TRAIT: INTENT STORE =====

pub trait IntentStore: Send + Sync {
    // Intent CRUD
    fn save_intent(&self, intent: &ShadowIntent) -> anyhow::Result<()>;
    fn get_intent(&self, id: &str) -> anyhow::Result<Option<ShadowIntent>>;
    fn get_intents_by_status(&self, status: IntentStatus) -> anyhow::Result<Vec<ShadowIntent>>;

    // Intent field updates
    fn update_intent_status(&self, id: &str, status: IntentStatus) -> anyhow::Result<()>;
    fn update_intent_near_status(&self, id: &str, near_status: &str) -> anyhow::Result<()>;
    fn update_intent_deposit_address(&self, id: &str, deposit_address: &str) -> anyhow::Result<()>;
    fn update_intent_dest_tx(&self, id: &str, tx_hash: &str) -> anyhow::Result<()>;
    fn update_intent_settle_tx(&self, id: &str, tx_hash: &str) -> anyhow::Result<()>;
    fn update_intent_source_settle_tx(&self, id: &str, tx_hash: &str) -> anyhow::Result<()>;

    // Merkle tree persistence
    fn get_latest_root(&self, tree_name: &str) -> anyhow::Result<Option<String>>;
    fn save_root(&self, tree_name: &str, root: &str, leaf_count: u64) -> anyhow::Result<()>;
    fn get_leaves(&self, tree_name: &str) -> anyhow::Result<Vec<String>>;
    fn add_leaf(&self, tree_name: &str, leaf: &str) -> anyhow::Result<()>;

    // Transaction logging
    fn log_transaction(
        &self,
        intent_id: &str,
        chain: ChainId,
        tx_type: &str,
        tx_hash: &str,
        status: &str,
    ) -> anyhow::Result<()>;
}

// ==================== IMPLEMENTATIONS ====================

impl IntentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::Batched => "batched",
            Self::Registered => "registered",
            Self::RootSynced => "root_synced",
            Self::NearSubmitted => "near_submitted",
            Self::TokensDelivered => "tokens_delivered",
            Self::Filled => "filled",
            Self::Settled => "settled",
            Self::MarkedSettled => "marked_settled",
            Self::SettlementFailed => "settlement_failed",
            Self::UserClaimed => "user_claimed",
            Self::Expired => "expired",
            Self::Refunded => "refunded",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "committed" => Some(Self::Committed),
            "batched" => Some(Self::Batched),
            "registered" => Some(Self::Registered),
            "root_synced" => Some(Self::RootSynced),
            "near_submitted" => Some(Self::NearSubmitted),
            "tokens_delivered" => Some(Self::TokensDelivered),
            "filled" => Some(Self::Filled),
            "settled" => Some(Self::Settled),
            "marked_settled" => Some(Self::MarkedSettled),
            "settlement_failed" => Some(Self::SettlementFailed),
            "user_claimed" => Some(Self::UserClaimed),
            "expired" => Some(Self::Expired),
            "refunded" => Some(Self::Refunded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for IntentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl ChainId {
    /// Cross-chain root mapping identifier (used on-chain as remote chain key).
    /// NOT the actual network chain ID — use config for that.
    pub fn as_u64(&self) -> u64 {
        match self {
            Self::Evm => 1,
            Self::Starknet => 2,
        }
    }

    pub fn as_chain_id_str(&self) -> &'static str {
        match self {
            Self::Evm => "evm",
            Self::Starknet => "starknet",
        }
    }

    /// Infallible: maps unknown names to Evm. Use `try_from_name` for user-supplied input.
    pub fn from_name(name: &str) -> Self {
        Self::try_from_name(name).unwrap_or(Self::Evm)
    }

    /// Returns `None` for unknown chain names. Use this for validating user-supplied input.
    pub fn try_from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "evm" | "ethereum" | "eth" | "sepolia" | "1" | "11155111" => Some(Self::Evm),
            "starknet" | "starknet_sepolia" | "starknet-sepolia" | "sn_main" | "sn_sepolia"
            | "2" | "393" => Some(Self::Starknet),
            _ => None,
        }
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_chain_id_str())
    }
}
