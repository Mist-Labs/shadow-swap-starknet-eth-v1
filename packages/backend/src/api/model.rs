use serde::{Deserialize, Serialize};

// ============================================================================
// BRIDGE REQUEST/RESPONSE
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct InitiateBridgeRequest {
    pub intent_id: String,
    pub commitment: String,
    pub nullifier_hash: String,
    pub view_key: String,
    pub near_intents_id: String,
    pub source_chain: String,
    pub dest_chain: String,
    pub encrypted_recipient: String,
    pub token: String,
    pub amount: String,
    pub deposit_address: Option<String>,
    pub encrypted_secret: String,
    pub encrypted_nullifier: String,
    pub dest_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmDepositRequest {
    pub intent_id: String,
    pub deposit_address: String,
}

#[derive(Debug, Serialize)]
pub struct InitiateBridgeResponse {
    pub success: bool,
    pub intent_id: String,
    pub commitment: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IntentStatusResponse {
    pub intent_id: String,
    pub status: String,
    pub source_chain: String,
    pub dest_chain: String,
    pub token: String,
    pub amount: String,
    pub commitment: String,
    pub deposit_address: Option<String>,
    pub near_status: Option<String>,
    pub dest_tx_hash: Option<String>,
    pub settle_tx_hash: Option<String>,
    pub source_settle_tx_hash: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize)]
pub struct IntentListResponse {
    pub status: String,
    pub count: usize,
    pub data: Vec<IntentStatusResponse>,
}

// ============================================================================
// INDEXER EVENT
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct IndexerEventRequest {
    pub event_type: String,
    pub chain: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub log_index: u32,
    pub event_data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct IndexerEventResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// METRICS
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub status: String,
    pub data: MetricsData,
}

#[derive(Debug, Serialize)]
pub struct MetricsData {
    pub total_intents: u64,
    pub pending_intents: u64,
    pub settled_intents: u64,
    pub failed_intents: u64,
    pub refunded_intents: u64,
}
