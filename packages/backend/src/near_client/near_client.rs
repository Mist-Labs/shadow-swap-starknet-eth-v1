use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::near_client::model::{
    DepositSubmitRequest, DepositSubmitResponse, DepositType, NearClient, NearSwapResult,
    NearSwapStatus, QuoteRequest, QuoteResponse, RefundType, RecipientType, StatusResponse,
    SwapType, TokenInfo,
};

const BASE_URL: &str = "https://1click.chaindefuser.com";
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_MAX_POLLS: u32 = 120;

impl NearClient {
    pub fn new(api_key: Option<&str>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let base_url = std::env::var("NEAR_BASE_URL")
            .unwrap_or_else(|_| BASE_URL.to_string());

        Ok(Self {
            client,
            base_url,
            api_key: api_key.map(|k| k.to_string()),
            poll_interval: DEFAULT_POLL_INTERVAL,
            max_polls: DEFAULT_MAX_POLLS,
        })
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    pub fn with_poll_config(mut self, interval: Duration, max_polls: u32) -> Self {
        self.poll_interval = interval;
        self.max_polls = max_polls;
        self
    }

    // ===== GET /v0/tokens =====

    pub async fn get_supported_tokens(&self) -> Result<Vec<TokenInfo>> {
        let url = format!("{}/v0/tokens", self.base_url);

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .context("Failed to fetch supported tokens")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Get tokens failed ({}): {}", status, text));
        }

        resp.json().await.context("Failed to parse tokens response")
    }

    // ===== POST /v0/quote =====

    pub async fn request_quote(&self, req: &QuoteRequest) -> Result<QuoteResponse> {
        let url = format!("{}/v0/quote", self.base_url);

        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(req)
            .send()
            .await
            .context("Failed to request quote")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Quote request failed ({}): {}", status, text));
        }

        resp.json().await.context("Failed to parse quote response")
    }

    /// Request a live quote (dry=false) → returns deposit address.
    /// User must send tokens to this deposit address to initiate the swap.
    pub async fn request_live_quote(
        &self,
        origin_asset: &str,
        destination_asset: &str,
        amount: &str,
        recipient: &str,
        refund_to: &str,
        slippage_bps: u32,
    ) -> Result<QuoteResponse> {
        let req = QuoteRequest {
            dry: false,
            swap_type: SwapType::ExactInput,
            slippage_tolerance: slippage_bps,
            origin_asset: origin_asset.to_string(),
            destination_asset: destination_asset.to_string(),
            amount: amount.to_string(),
            refund_to: refund_to.to_string(),
            recipient: recipient.to_string(),
            deposit_type: Some(DepositType::OriginChain),
            refund_type: Some(RefundType::OriginChain),
            recipient_type: Some(RecipientType::DestinationChain),
            deadline: None,
            referral: Some("shadowswap".to_string()),
            quote_waiting_time_ms: Some(3000),
        };

        let quote = self.request_quote(&req).await?;

        if quote.quote.deposit_address.is_none() {
            return Err(anyhow!(
                "Quote response missing deposit address (was dry=true used?)"
            ));
        }

        Ok(quote)
    }

    // ===== POST /v0/deposit/submit =====

    pub async fn submit_deposit_tx(
        &self,
        tx_hash: &str,
        deposit_address: &str,
    ) -> Result<DepositSubmitResponse> {
        let url = format!("{}/v0/deposit/submit", self.base_url);

        let body = DepositSubmitRequest {
            tx_hash: tx_hash.to_string(),
            deposit_address: deposit_address.to_string(),
            near_sender_account: None,
            memo: None,
        };

        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await
            .context("Failed to submit deposit tx")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Deposit submit failed ({}): {}", status, text));
        }

        resp.json()
            .await
            .context("Failed to parse deposit submit response")
    }

    // ===== GET /v0/status?depositAddress=... =====

    pub async fn get_status(&self, deposit_address: &str) -> Result<NearSwapResult> {
        // Fix #7: URL-encode the deposit address to prevent query-string injection.
        let url = format!(
            "{}/v0/status?depositAddress={}",
            self.base_url,
            urlencoding::encode(deposit_address)
        );

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .context("Failed to get swap status")?;

        // 404 = deposit address not found yet (not deposited)
        if resp.status().as_u16() == 404 {
            return Ok(NearSwapResult {
                status: NearSwapStatus::PendingDeposit,
                deposit_address: deposit_address.to_string(),
                destination_tx_hashes: vec![],
                amount_out: None,
                refund_reason: None,
            });
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Status request failed ({}): {}", status, text));
        }

        let status_resp: StatusResponse = resp
            .json()
            .await
            .context("Failed to parse status response")?;

        let near_status = NearSwapStatus::from_api_string(&status_resp.status)
            .ok_or_else(|| anyhow!("Unknown NEAR status: {}", status_resp.status))?;

        let (dest_tx_hashes, amount_out, refund_reason) = match &status_resp.swap_details {
            Some(details) => (
                details
                    .destination_chain_tx_hashes
                    .iter()
                    .map(|e| e.hash.clone())
                    .collect(),
                details.amount_out.clone(),
                details.refund_reason.clone(),
            ),
            None => (vec![], None, None),
        };

        Ok(NearSwapResult {
            status: near_status,
            deposit_address: deposit_address.to_string(),
            destination_tx_hashes: dest_tx_hashes,
            amount_out,
            refund_reason,
        })
    }

    // ===== POLL UNTIL TERMINAL =====

    pub async fn poll_until_complete(&self, deposit_address: &str) -> Result<NearSwapResult> {
        info!(
            "⏳ Polling NEAR swap status for deposit: {}",
            &deposit_address[..18.min(deposit_address.len())]
        );

        for attempt in 1..=self.max_polls {
            let result = self.get_status(deposit_address).await?;

            match result.status {
                NearSwapStatus::Success => {
                    info!(
                        "✅ NEAR swap complete after {} polls, dest txs: {:?}",
                        attempt, result.destination_tx_hashes
                    );
                    return Ok(result);
                }
                NearSwapStatus::Refunded => {
                    warn!(
                        "♻️ NEAR swap refunded after {} polls, reason: {:?}",
                        attempt, result.refund_reason
                    );
                    return Ok(result);
                }
                NearSwapStatus::Failed => {
                    return Err(anyhow!(
                        "NEAR swap failed for deposit {}",
                        deposit_address
                    ));
                }
                NearSwapStatus::IncompleteDeposit => {
                    warn!("⚠️ Incomplete deposit — waiting for resolution");
                    sleep(self.poll_interval).await;
                }
                NearSwapStatus::PendingDeposit | NearSwapStatus::Processing => {
                    if attempt % 12 == 0 {
                        info!(
                            "⏳ Still waiting... attempt {}/{} (status: {:?})",
                            attempt, self.max_polls, result.status
                        );
                    }
                    sleep(self.poll_interval).await;
                }
            }
        }

        Err(anyhow!(
            "NEAR swap polling timed out after {} attempts for {}",
            self.max_polls,
            deposit_address
        ))
    }

    // ===== HEALTH CHECK =====

    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/v0/tokens", self.base_url);

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => Ok(()),
            Ok(r) => Err(anyhow!("NEAR API unhealthy: {}", r.status())),
            Err(e) => Err(anyhow!("NEAR API unreachable: {}", e)),
        }
    }
}