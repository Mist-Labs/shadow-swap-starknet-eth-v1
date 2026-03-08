use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use tracing::{error, info, warn};

use crate::{
    api::model::{IndexerEventRequest, IndexerEventResponse},
    models::models::{ChainId, IntentStatus, IntentStore},
    AppState,
};

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// HMAC VALIDATION
// ============================================================================

pub fn validate_hmac(
    req: &HttpRequest,
    body: &web::Bytes,
    app_state: &web::Data<AppState>,
) -> Result<(), HttpResponse> {
    let timestamp = req
        .headers()
        .get("x-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Missing or invalid x-timestamp header"
            }))
        })?;

    let provided_signature = req
        .headers()
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Missing or invalid x-signature header"
            }))
        })?;

    let request_timestamp: i64 = timestamp.parse().map_err(|_| {
        HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Invalid timestamp format"
        }))
    })?;

    let time_diff = (Utc::now().timestamp() - request_timestamp).abs();
    let tolerance = app_state.config.server.timestamp_tolerance_secs;
    if time_diff > tolerance {
        return Err(HttpResponse::Unauthorized().json(json!({
            "success": false,
            "message": "Request timestamp too old or in future"
        })));
    }

    let hmac_secret = &app_state.config.server.hmac_secret;
    let body_str = std::str::from_utf8(body).map_err(|_| {
        HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Invalid UTF-8 in body"
        }))
    })?;

    // Decode the provided signature to raw bytes before comparison.
    // This must happen before the MAC is consumed by verify_slice.
    let sig_bytes = hex::decode(provided_signature).map_err(|_| {
        HttpResponse::Unauthorized().json(json!({
            "success": false,
            "message": "Invalid signature encoding"
        }))
    })?;

    let message = format!("{}{}", timestamp, body_str);
    let mut mac = HmacSha256::new_from_slice(hmac_secret.as_bytes()).map_err(|_| {
        HttpResponse::InternalServerError().json(json!({
            "success": false,
            "message": "HMAC initialization failed"
        }))
    })?;
    mac.update(message.as_bytes());

    // verify_slice is constant-time — immune to timing attacks.
    // String equality (`==`) short-circuits and leaks the signature byte-by-byte.
    if mac.verify_slice(&sig_bytes).is_err() {
        error!("Invalid HMAC signature");
        return Err(HttpResponse::Unauthorized().json(json!({
            "success": false,
            "message": "Invalid signature"
        })));
    }

    Ok(())
}

// ============================================================================
// CHAIN HELPERS
// ============================================================================

fn chain_id_from_indexer(chain_str: &str) -> Option<ChainId> {
    match chain_str {
        "ethereum" | "sepolia" | "evm" | "eth" | "1" | "11155111" => Some(ChainId::Evm),
        "starknet" | "starknet_sepolia" | "SN_SEPOLIA" | "SN_MAIN" => Some(ChainId::Starknet),
        _ => None,
    }
}

fn chain_to_commitment_key(chain: ChainId) -> &'static str {
    match chain {
        ChainId::Evm => "evm",
        ChainId::Starknet => "starknet",
    }
}

fn ok_response(msg: impl Into<String>) -> HttpResponse {
    HttpResponse::Ok().json(IndexerEventResponse {
        success: true,
        message: msg.into(),
        error: None,
    })
}

fn bad_request(msg: impl Into<String>) -> HttpResponse {
    HttpResponse::BadRequest().json(IndexerEventResponse {
        success: false,
        message: msg.into(),
        error: None,
    })
}

fn internal_error(msg: impl Into<String>, err: impl ToString) -> HttpResponse {
    HttpResponse::InternalServerError().json(IndexerEventResponse {
        success: false,
        message: msg.into(),
        error: Some(err.to_string()),
    })
}

// ============================================================================
// EVENT: commitment_added
// Emitted by ShadowIntentPool on source chain when user creates intent.
// Stores the commitment in the commitments table for Merkle tree building.
// ============================================================================

pub async fn handle_commitment_added(
    app_state: &web::Data<AppState>,
    request: &IndexerEventRequest,
) -> HttpResponse {
    info!("Processing commitment_added on {}", request.chain);

    let commitment = match request
        .event_data
        .get("commitment")
        .and_then(|v| v.as_str())
    {
        Some(c) if !c.is_empty() => c,
        _ => return bad_request("Missing commitment in event_data"),
    };

    let intent_id = request.event_data.get("intentId").and_then(|v| v.as_str());

    let chain = match chain_id_from_indexer(&request.chain) {
        Some(c) => c,
        None => return bad_request(format!("Unknown chain: {}", request.chain)),
    };

    let chain_key = chain_to_commitment_key(chain);

    if let Err(e) = app_state.database.add_commitment_record(
        chain_key,
        commitment,
        intent_id,
        Some(request.block_number),
        Some(request.log_index),
    ) {
        if e.to_string().contains("duplicate") || e.to_string().contains("unique") {
            info!(
                "Commitment {} already stored (idempotent)",
                &commitment[..18]
            );
            return ok_response("Commitment already recorded");
        }
        error!("Failed to store commitment: {}", e);
        return internal_error("Failed to store commitment", e);
    }

    // Also add to Merkle tree
    if let Err(e) = app_state
        .database
        .add_leaf(&format!("{}_commitments", chain_key), commitment)
    {
        error!("Failed to add Merkle leaf: {}", e);
    }

    info!(
        "Commitment {} stored for {} chain",
        &commitment[..18],
        chain_key
    );

    // If intent exists, update its status to Batched (commitment confirmed on-chain)
    if let Some(id) = intent_id {
        if let Ok(Some(_)) = app_state.database.get_intent(id) {
            if let Err(e) = app_state
                .database
                .update_intent_status(id, IntentStatus::Batched)
            {
                warn!("Failed to update intent {} to Batched: {}", &id[..10], e);
            }
        }
    }

    ok_response(format!(
        "Commitment {} recorded on {}",
        &commitment[..18],
        chain_key
    ))
}

// ============================================================================
// EVENT: settled
// Emitted by ShadowSettlement on destination chain after settleAndRelease.
// ============================================================================

pub async fn handle_settled_event(
    app_state: &web::Data<AppState>,
    request: &IndexerEventRequest,
) -> HttpResponse {
    let is_swap = request
        .event_data
        .get("is_swap")
        .and_then(|v| v.as_str())
        .map(|v| v == "true")
        .unwrap_or(false);

    info!(
        "Processing {} on {}",
        if is_swap {
            "settled_with_swap"
        } else {
            "settled"
        },
        request.chain
    );

    // ── commitment is OPTIONAL (settled_with_swap events don't have it) ──
    let commitment = request
        .event_data
        .get("commitment")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let nullifier_hash = request
        .event_data
        .get("nullifierHash")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let intent_id = match request.event_data.get("intentId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            // Only log commitment if it exists
            if let Some(c) = commitment {
                info!("Settled event for commitment {} (no intentId)", &c[..18]);
            } else {
                info!("Settled event (no intentId, no commitment)");
            }
            return ok_response("Settled event recorded (no intentId to update)");
        }
    };

    let chain = match chain_id_from_indexer(&request.chain) {
        Some(c) => c,
        None => return bad_request(format!("Unknown chain: {}", request.chain)),
    };

    if let Err(e) = app_state
        .database
        .update_intent_settle_tx(intent_id, &request.transaction_hash)
    {
        warn!(
            "Failed to update settle_tx_hash for {}: {}",
            &intent_id[..10],
            e
        );
    }

    if let Err(e) = app_state
        .database
        .update_intent_status(intent_id, IntentStatus::Settled)
    {
        error!(
            "Failed to update intent {} to Settled: {}",
            &intent_id[..10],
            e
        );
        return internal_error("Failed to update intent status", e);
    }

    if let Err(e) = app_state.database.log_transaction(
        intent_id,
        chain,
        "settle_and_release",
        &request.transaction_hash,
        "confirmed",
    ) {
        warn!("Failed to log settle transaction: {}", e);
    }

    info!(
        "Intent {} settled on destination{} (nullifier: {})",
        &intent_id[..10],
        if is_swap { " via AVNU swap" } else { "" },
        &nullifier_hash[..18.min(nullifier_hash.len())]
    );

    ok_response(format!("Intent {} settled", &intent_id[..10]))
}

// ============================================================================
// EVENT: marked_settled
// Emitted by ShadowIntentPool on source chain after markSettled.
// Final lifecycle state.
// ============================================================================

pub async fn handle_marked_settled_event(
    app_state: &web::Data<AppState>,
    request: &IndexerEventRequest,
) -> HttpResponse {
    info!("Processing marked_settled on {}", request.chain);

    let commitment = match request
        .event_data
        .get("commitment")
        .and_then(|v| v.as_str())
    {
        Some(c) if !c.is_empty() => c,
        _ => return bad_request("Missing commitment in event_data"),
    };

    let intent_id = match request.event_data.get("intentId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            info!(
                "MarkedSettled for commitment {} (no intentId)",
                &commitment[..18]
            );
            return ok_response("MarkedSettled recorded (no intentId to update)");
        }
    };

    let chain = match chain_id_from_indexer(&request.chain) {
        Some(c) => c,
        None => return bad_request(format!("Unknown chain: {}", request.chain)),
    };

    if let Err(e) = app_state
        .database
        .update_intent_source_settle_tx(intent_id, &request.transaction_hash)
    {
        warn!(
            "Failed to update source_settle_tx for {}: {}",
            &intent_id[..10],
            e
        );
    }

    if let Err(e) = app_state
        .database
        .update_intent_status(intent_id, IntentStatus::MarkedSettled)
    {
        error!(
            "Failed to update intent {} to MarkedSettled: {}",
            &intent_id[..10],
            e
        );
        return internal_error("Failed to update intent status", e);
    }

    if let Err(e) = app_state.database.log_transaction(
        intent_id,
        chain,
        "mark_settled",
        &request.transaction_hash,
        "confirmed",
    ) {
        warn!("Failed to log mark_settled transaction: {}", e);
    }

    info!("Intent {} fully settled (MarkedSettled)", &intent_id[..10]);

    ok_response(format!("Intent {} marked settled", &intent_id[..10]))
}

// ============================================================================
// EVENT: merkle_root_updated
// Emitted by both ShadowIntentPool and ShadowSettlement when root changes.
// Used for cross-chain root sync verification.
// ============================================================================

pub async fn handle_root_updated_event(
    app_state: &web::Data<AppState>,
    request: &IndexerEventRequest,
) -> HttpResponse {
    info!("Processing merkle_root_updated on {}", request.chain);

    let new_root = match request
        .event_data
        .get("new_root")
        .or_else(|| request.event_data.get("newRoot"))
        .and_then(|v| v.as_str())
    {
        Some(r) if !r.is_empty() => r,
        _ => return bad_request("Missing new_root in event_data"),
    };

    let chain = match chain_id_from_indexer(&request.chain) {
        Some(c) => c,
        None => return bad_request(format!("Unknown chain: {}", request.chain)),
    };

    let _tree_name = format!("{}_commitments", chain_to_commitment_key(chain));

    if let Err(e) = app_state.database.log_transaction(
        "root_sync",
        chain,
        "merkle_root_updated",
        &request.transaction_hash,
        "confirmed",
    ) {
        warn!("Failed to log root update: {}", e);
    }

    info!(
        "Root updated on {}: {}",
        chain_to_commitment_key(chain),
        &new_root[..18]
    );

    ok_response(format!("Root update recorded: {}", &new_root[..18]))
}

// ============================================================================
// EVENT: batch_processed
// Emitted by ShadowSettlement on source chain when a pending batch is flushed
// into the Merkle tree. Informational — commitments already handled via
// commitment_added events; root via merkle_root_updated.
// ============================================================================

pub async fn handle_batch_processed_event(
    _app_state: &web::Data<AppState>,
    request: &IndexerEventRequest,
) -> HttpResponse {
    let batch_id = request
        .event_data
        .get("batch_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let count = request
        .event_data
        .get("commitments_count")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let reason = request
        .event_data
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    info!(
        "BatchProcessed on {}: batch_id={} count={} reason={}",
        request.chain, batch_id, count, reason
    );

    ok_response(format!(
        "BatchProcessed acknowledged: batch_id={} count={}",
        batch_id, count
    ))
}
