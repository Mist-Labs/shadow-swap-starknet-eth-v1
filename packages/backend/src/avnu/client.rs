//! AVNU REST API client.
//!
//! Flow:
//!   1. POST /swap/v2/quotes  — get best quote (buyAmount, quoteId)
//!   2. POST /swap/v2/build   — get ready-made multi_route_swap calldata
//!   3. Extract routes felts from calldata and pass to settle_and_release
//!
//! We use /swap/v2/build because AVNU's route encoding (percent scale,
//! nested multi-hop routes, DEX-specific additional_swap_params) is complex
//! and changes without notice. Building server-side is the only reliable approach.

use anyhow::{anyhow, Context, Result};
use num_bigint::BigUint;
use num_traits::{Num, ToPrimitive, Zero};
use serde_json::Value;
use starknet::core::types::Felt;
use tracing::{info, warn};

const AVNU_BASE_URL: &str = "https://starknet.api.avnu.fi";
const QUOTES_ENDPOINT: &str = "/swap/v2/quotes";
const BUILD_ENDPOINT: &str = "/swap/v2/build";

// Default slippage passed to /swap/v2/build (0.5%)
const DEFAULT_SLIPPAGE: f64 = 0.005;

// ── Public types ──────────────────────────────────────────────────────────────

pub struct AvnuQuote {
    pub buy_amount: BigUint,
    pub min_dest_amount: BigUint,
    /// Raw routes calldata felts extracted from /swap/v2/build response.
    /// These are passed verbatim to settle_and_release as the routes array.
    /// Layout: [routes_len, ...serialized Route structs]
    pub routes_calldata: Vec<Felt>,
}

impl AvnuQuote {
    pub fn min_dest_amount_felts(&self) -> Result<(Felt, Felt)> {
        biguint_to_felt_pair(&self.min_dest_amount)
    }

    pub fn buy_amount_felts(&self) -> Result<(Felt, Felt)> {
        biguint_to_felt_pair(&self.buy_amount)
    }
}

// ── Client ────────────────────────────────────────────────────────────────────

pub struct AvnuClient {
    http: reqwest::Client,
    base_url: String,
}

impl AvnuClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("Failed to build reqwest client"),
            base_url: AVNU_BASE_URL.to_string(),
        }
    }

    pub async fn fetch_quote(
        &self,
        sell_token: &str,
        buy_token: &str,
        sell_amount_str: &str,
        taker_address: &str,
    ) -> Result<AvnuQuote> {
        // ── Step 1: Get quote ─────────────────────────────────────────────────
        let sell_amount_biguint: BigUint = sell_amount_str
            .parse()
            .map_err(|_| anyhow!("Invalid sell amount: {}", sell_amount_str))?;
        let sell_amount_hex = format!("0x{}", sell_amount_biguint.to_str_radix(16));

        info!(
            "Fetching AVNU quote: {} {} -> {}",
            &sell_amount_hex,
            &sell_token[..sell_token.len().min(18)],
            &buy_token[..buy_token.len().min(18)],
        );

        let quotes_url = format!("{}{}", self.base_url, QUOTES_ENDPOINT);
        let response = self
            .http
            .get(&quotes_url)
            .query(&[
                ("sellTokenAddress", sell_token),
                ("buyTokenAddress", buy_token),
                ("sellAmount", sell_amount_hex.as_str()),
                ("takerAddress", taker_address),
                ("size", "1"),
            ])
            .send()
            .await
            .context("AVNU quotes HTTP request failed")?;

        let status = response.status();
        let body = response.text().await.context("Failed to read AVNU quotes body")?;

        if !status.is_success() {
            return Err(anyhow!(
                "AVNU quotes API returned {}: {}",
                status,
                &body[..body.len().min(200)]
            ));
        }

        let quotes: Vec<Value> =
            serde_json::from_str(&body).context("Failed to parse AVNU quotes response")?;

        let best = quotes.into_iter().next().ok_or_else(|| {
            anyhow!("AVNU returned no quotes for {} -> {}", sell_token, buy_token)
        })?;

        let quote_id = best["quoteId"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing quoteId in AVNU response"))?
            .to_string();

        let buy_amount_raw = best["buyAmount"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing buyAmount in AVNU response"))?;
        let buy_amount = parse_amount_str(buy_amount_raw)
            .with_context(|| format!("Failed to parse buyAmount: {}", buy_amount_raw))?;

        if buy_amount.is_zero() {
            return Err(anyhow!("AVNU quote returned zero buyAmount"));
        }

        // Apply slippage to compute min_dest_amount for our own balance-delta check.
        // AVNU also enforces slippage server-side via the build endpoint.
        let slippage_bps = (DEFAULT_SLIPPAGE * 10_000.0) as u64;
        let slippage_num = BigUint::from(10_000u64 - slippage_bps);
        let min_dest_amount = (&buy_amount * slippage_num) / BigUint::from(10_000u64);

        if min_dest_amount.is_zero() {
            warn!("min_dest_amount rounds to zero (dust: buy_amount={})", buy_amount);
            return Err(anyhow!(
                "AVNU_DUST: min_dest_amount is zero — caller should fall back to direct transfer"
            ));
        }

        info!("AVNU quote: quoteId={} buy={} min={}", &quote_id[..8], buy_amount, min_dest_amount);

        // ── Step 2: Build calldata ────────────────────────────────────────────
        let build_url = format!("{}{}", self.base_url, BUILD_ENDPOINT);
        let build_body = serde_json::json!({
            "quoteId": quote_id,
            "takerAddress": taker_address,
            "slippage": DEFAULT_SLIPPAGE,
            "includeApprove": false,
        });

        let build_response = self
            .http
            .post(&build_url)
            .header("Content-Type", "application/json")
            .body(build_body.to_string())
            .send()
            .await
            .context("AVNU build HTTP request failed")?;

        let build_status = build_response.status();
        let build_body_str = build_response
            .text()
            .await
            .context("Failed to read AVNU build body")?;

        if !build_status.is_success() {
            return Err(anyhow!(
                "AVNU build API returned {}: {}",
                build_status,
                &build_body_str[..build_body_str.len().min(300)]
            ));
        }

        let build_result: Value = serde_json::from_str(&build_body_str)
            .context("Failed to parse AVNU build response")?;

        // ── Step 3: Extract routes calldata ───────────────────────────────────
        // /swap/v2/build returns:
        // {
        //   calls: [{
        //     contractAddress: "0x4270219...",
        //     entrypoint: "multi_route_swap",
        //     calldata: [
        //       sell_token, sell_amount_low, sell_amount_high,
        //       buy_token, buy_amount_low, buy_amount_high,
        //       buy_min_amount_low, buy_min_amount_high,
        //       beneficiary, integrator_fee_bps, integrator_fee_recipient,
        //       routes_len, ...route_felts   ← we want from index 11 onwards
        //     ]
        //   }]
        // }
        let calls = build_result["calls"]
            .as_array()
            .ok_or_else(|| anyhow!("Missing calls array in AVNU build response"))?;

        let swap_call = calls
            .iter()
            .find(|c| c["entrypoint"].as_str() == Some("multi_route_swap"))
            .ok_or_else(|| anyhow!("No multi_route_swap call in AVNU build response"))?;

        let calldata = swap_call["calldata"]
            .as_array()
            .ok_or_else(|| anyhow!("Missing calldata in AVNU build response"))?;

        // Routes start at index 11: [routes_len, ...serialized routes]
        // Indices 0-10 are the fixed multi_route_swap params we build ourselves.
        const ROUTES_START: usize = 11;

        if calldata.len() <= ROUTES_START {
            return Err(anyhow!(
                "AVNU build calldata too short: {} felts (expected >{})",
                calldata.len(),
                ROUTES_START
            ));
        }

        let routes_calldata: Vec<Felt> = calldata[ROUTES_START..]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let s = v.as_str().ok_or_else(|| {
                    anyhow!("Calldata[{}] is not a string: {}", ROUTES_START + i, v)
                })?;
                parse_felt_str(s).with_context(|| {
                    format!("Failed to parse calldata[{}]={}", ROUTES_START + i, s)
                })
            })
            .collect::<Result<Vec<_>>>()?;

        info!(
            "AVNU build: {} route calldata felts extracted",
            routes_calldata.len()
        );

        Ok(AvnuQuote {
            buy_amount,
            min_dest_amount,
            routes_calldata,
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_felt_str(s: &str) -> Result<Felt> {
    let trimmed = s.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        Felt::from_hex(trimmed).map_err(|e| anyhow!("Invalid felt hex '{}': {}", trimmed, e))
    } else {
        let n: BigUint = trimmed
            .parse()
            .map_err(|_| anyhow!("Invalid felt decimal '{}'", trimmed))?;
        let bytes = n.to_bytes_be();
        let mut padded = [0u8; 32];
        let start = 32usize.saturating_sub(bytes.len());
        padded[start..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
        Ok(Felt::from_bytes_be(&padded))
    }
}

fn parse_amount_str(s: &str) -> Result<BigUint> {
    let trimmed = s.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        BigUint::from_str_radix(&trimmed[2..], 16)
            .map_err(|e| anyhow!("Invalid hex amount '{}': {}", trimmed, e))
    } else {
        trimmed
            .parse::<BigUint>()
            .map_err(|_| anyhow!("Invalid decimal amount '{}'", trimmed))
    }
}

fn biguint_to_felt_pair(n: &BigUint) -> Result<(Felt, Felt)> {
    let mask = BigUint::from(u128::MAX);
    let low_u128 = (n & &mask)
        .to_u128()
        .ok_or_else(|| anyhow!("u256 low limb overflow"))?;
    let high_u128 = (n >> 128u32)
        .to_u128()
        .ok_or_else(|| anyhow!("u256 high limb overflow"))?;
    Ok((Felt::from(low_u128), Felt::from(high_u128)))
}