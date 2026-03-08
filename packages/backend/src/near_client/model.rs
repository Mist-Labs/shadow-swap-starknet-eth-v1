use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ===== REQUEST TYPES =====

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    pub dry: bool,
    pub swap_type: SwapType,
    pub slippage_tolerance: u32,
    pub origin_asset: String,
    pub destination_asset: String,
    pub amount: String,
    pub refund_to: String,
    pub recipient: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_type: Option<DepositType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_type: Option<RefundType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_type: Option<RecipientType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_waiting_time_ms: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwapType {
    ExactInput,
    ExactOutput,
    FlexInput,
    AnyInput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DepositType {
    OriginChain,
    Intents,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundType {
    OriginChain,
    Intents,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecipientType {
    DestinationChain,
    Intents,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositSubmitRequest {
    pub tx_hash: String,
    pub deposit_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near_sender_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

// ===== RESPONSE TYPES =====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    pub correlation_id: String,
    pub timestamp: Option<String>,
    pub quote: QuoteDetails,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteDetails {
    pub deposit_address: Option<String>,
    pub deposit_memo: Option<String>,
    pub amount_in: String,
    pub amount_in_formatted: Option<String>,
    pub amount_in_usd: Option<String>,
    pub min_amount_in: Option<String>,
    pub amount_out: String,
    pub amount_out_formatted: Option<String>,
    pub min_amount_out: Option<String>,
    pub deadline: Option<String>,
    pub time_when_inactive: Option<String>,
    pub time_estimate: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub correlation_id: Option<String>,
    pub status: String,
    pub updated_at: Option<String>,
    pub swap_details: Option<SwapDetails>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapDetails {
    #[serde(default)]
    pub intent_hashes: Vec<String>,
    #[serde(default)]
    pub near_tx_hashes: Vec<String>,
    pub amount_in: Option<String>,
    pub amount_out: Option<String>,
    #[serde(default)]
    pub origin_chain_tx_hashes: Vec<TxHashEntry>,
    #[serde(default)]
    pub destination_chain_tx_hashes: Vec<TxHashEntry>,
    pub refunded_amount: Option<String>,
    pub refund_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxHashEntry {
    pub hash: String,
    pub explorer_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositSubmitResponse {
    pub correlation_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub asset_id: String,
    pub decimals: u8,
    pub blockchain: String,
    pub symbol: String,
    pub price: Option<String>,
    pub contract_address: Option<String>,
}

// ===== NEAR SWAP STATUS =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NearSwapStatus {
    PendingDeposit,
    Processing,
    Success,
    IncompleteDeposit,
    Refunded,
    Failed,
}

impl NearSwapStatus {
    pub fn from_api_string(s: &str) -> Option<Self> {
        match s {
            "PENDING_DEPOSIT" => Some(Self::PendingDeposit),
            "PROCESSING" => Some(Self::Processing),
            "SUCCESS" => Some(Self::Success),
            "INCOMPLETE_DEPOSIT" => Some(Self::IncompleteDeposit),
            "REFUNDED" => Some(Self::Refunded),
            "FAILED" => Some(Self::Failed),
            "KNOWN_DEPOSIT_TX" => Some(Self::Processing),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Refunded | Self::Failed)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingDeposit | Self::Processing)
    }
}

// ===== POLLING RESULT =====

#[derive(Debug, Clone)]
pub struct NearSwapResult {
    pub status: NearSwapStatus,
    pub deposit_address: String,
    pub destination_tx_hashes: Vec<String>,
    pub amount_out: Option<String>,
    pub refund_reason: Option<String>,
}

// ===== CLIENT STRUCT =====

pub struct NearClient {
    pub(crate) client: Client,
    pub(crate) base_url: String,
    pub(crate) api_key: Option<String>,
    pub(crate) poll_interval: Duration,
    pub(crate) max_polls: u32,
}

impl NearClient {
    pub(crate) fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = &self.api_key {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", key)) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }
}
