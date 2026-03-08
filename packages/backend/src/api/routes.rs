use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::{
    api::{
        helper::{
            handle_commitment_added, handle_marked_settled_event, handle_root_updated_event,
            handle_settled_event, validate_hmac,
        },
        model::{
            ConfirmDepositRequest, IndexerEventRequest, IndexerEventResponse,
            InitiateBridgeRequest, InitiateBridgeResponse, IntentListResponse,
            IntentStatusResponse, MetricsData, MetricsResponse,
        },
    },
    models::models::{ChainId, IntentStatus, IntentStore, ShadowIntent},
    AppState,
};

// ============================================================================
// BRIDGE OPERATIONS
// ============================================================================

#[post("/bridge/initiate")]
pub async fn initiate_bridge(
    req: HttpRequest,
    body: web::Bytes,
    app_state: web::Data<AppState>,
) -> impl Responder {
    if let Err(response) = validate_hmac(&req, &body, &app_state) {
        return response;
    }

    let request: InitiateBridgeRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return HttpResponse::BadRequest().json(InitiateBridgeResponse {
                success: false,
                intent_id: String::new(),
                commitment: String::new(),
                message: "Invalid request body".to_string(),
                error: Some(e.to_string()),
            });
        }
    };

    let intent_id = request.intent_id.to_lowercase();
    if !intent_id.starts_with("0x") || intent_id.len() != 66 {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: intent_id.clone(),
            commitment: String::new(),
            message: "Invalid intent_id format (expected 0x + 64 hex chars)".to_string(),
            error: None,
        });
    }

    if !request.commitment.starts_with("0x") || request.commitment.len() != 66 {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: String::new(),
            commitment: String::new(),
            message: "Invalid commitment format (expected 0x + 64 hex chars)".to_string(),
            error: None,
        });
    }

    if !request.nullifier_hash.starts_with("0x") || request.nullifier_hash.len() != 66 {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: String::new(),
            commitment: String::new(),
            message: "Invalid nullifier_hash format".to_string(),
            error: None,
        });
    }

    if !request.encrypted_secret.starts_with("0x") || request.encrypted_secret.len() < 4 {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: String::new(),
            commitment: String::new(),
            message: "Invalid encrypted_secret format (expected 0x-prefixed hex)".to_string(),
            error: None,
        });
    }

    if !request.encrypted_nullifier.starts_with("0x") || request.encrypted_nullifier.len() < 4 {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: String::new(),
            commitment: String::new(),
            message: "Invalid encrypted_nullifier format (expected 0x-prefixed hex)".to_string(),
            error: None,
        });
    }

    // Fix #6: Use try_from_name to properly reject unknown chains (from_name silently maps to Evm)
    let source_chain = match ChainId::try_from_name(&request.source_chain) {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest().json(InitiateBridgeResponse {
                success: false,
                intent_id: String::new(),
                commitment: String::new(),
                message: format!("Unsupported source_chain: {}", request.source_chain),
                error: None,
            });
        }
    };

    let dest_chain = match ChainId::try_from_name(&request.dest_chain) {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest().json(InitiateBridgeResponse {
                success: false,
                intent_id: String::new(),
                commitment: String::new(),
                message: format!("Unsupported dest_chain: {}", request.dest_chain),
                error: None,
            });
        }
    };

    if source_chain == dest_chain {
        return HttpResponse::BadRequest().json(InitiateBridgeResponse {
            success: false,
            intent_id: String::new(),
            commitment: String::new(),
            message: "source_chain and dest_chain must differ".to_string(),
            error: None,
        });
    }

    // Fix #5: Reject duplicate nullifier_hash at intake (before DB write)
    match app_state
        .database
        .nullifier_hash_exists(&request.nullifier_hash)
    {
        Ok(true) => {
            warn!(
                "Duplicate nullifier_hash rejected at intake: {}",
                &request.nullifier_hash[..18]
            );
            return HttpResponse::Conflict().json(InitiateBridgeResponse {
                success: false,
                intent_id: String::new(),
                commitment: String::new(),
                message: "Nullifier already registered".to_string(),
                error: None,
            });
        }
        Err(e) => {
            error!("Failed to check nullifier uniqueness: {}", e);
            return HttpResponse::InternalServerError().json(InitiateBridgeResponse {
                success: false,
                intent_id: String::new(),
                commitment: String::new(),
                message: "Failed to validate nullifier".to_string(),
                error: Some(e.to_string()),
            });
        }
        Ok(false) => {} // not a duplicate — proceed
    }

    let now = Utc::now().timestamp() as u64;

    let intent = ShadowIntent {
        id: intent_id.clone(),
        commitment: request.commitment.clone(),
        nullifier_hash: request.nullifier_hash.clone(),
        view_key: request.view_key.clone(),
        near_intents_id: request.near_intents_id.clone(),
        source_chain,
        dest_chain,
        encrypted_recipient: request.encrypted_recipient.clone(),
        token: request.token.clone(),
        amount: request.amount.clone(),
        status: IntentStatus::Pending,
        deposit_address: request.deposit_address.clone(),
        near_correlation_id: None,
        near_status: None,
        dest_tx_hash: None,
        settle_tx_hash: None,
        source_settle_tx_hash: None,
        encrypted_secret: Some(request.encrypted_secret.clone()),
        encrypted_nullifier: Some(request.encrypted_nullifier.clone()),
        created_at: now,
        updated_at: now,
        dest_token: request.dest_token.clone(),
    };

    if let Err(e) = app_state.database.save_intent(&intent) {
        error!("Failed to save intent {}: {}", &intent_id[..10], e);
        return HttpResponse::InternalServerError().json(InitiateBridgeResponse {
            success: false,
            intent_id: intent_id.clone(),
            commitment: String::new(),
            message: "Failed to save intent".to_string(),
            error: Some(e.to_string()),
        });
    }

    info!(
        "Bridge intent created: {} ({} -> {})",
        &intent_id[..10],
        source_chain.as_chain_id_str(),
        dest_chain.as_chain_id_str()
    );

    HttpResponse::Ok().json(InitiateBridgeResponse {
        success: true,
        intent_id,
        commitment: request.commitment,
        message: "Intent created. Relay coordinator will process.".to_string(),
        error: None,
    })
}

// ============================================================================
// INTENT STATUS
// ============================================================================

#[get("/bridge/intent/{intent_id}")]
pub async fn get_intent_status(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let intent_id = path.into_inner();

    match app_state.database.get_intent(&intent_id) {
        Ok(Some(intent)) => HttpResponse::Ok().json(intent_to_response(&intent)),
        Ok(None) => HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Intent not found"
        })),
        Err(e) => {
            error!("Failed to get intent {}: {}", intent_id, e);
            HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to retrieve intent"
            }))
        }
    }
}

#[get("/bridge/intents")]
pub async fn list_intents(
    app_state: web::Data<AppState>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let status_filter = query.get("status").and_then(|s| IntentStatus::from_str(s));

    match status_filter {
        Some(status) => match app_state.database.get_intents_by_status(status) {
            Ok(intents) => {
                let limit: usize = query
                    .get("limit")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(50)
                    .min(200);

                let data: Vec<IntentStatusResponse> =
                    intents.iter().take(limit).map(intent_to_response).collect();

                HttpResponse::Ok().json(IntentListResponse {
                    status: "success".to_string(),
                    count: data.len(),
                    data,
                })
            }
            Err(e) => {
                error!("Failed to list intents: {}", e);
                HttpResponse::InternalServerError().json(json!({
                    "success": false,
                    "message": "Failed to retrieve intents"
                }))
            }
        },
        None => {
            let msg = query
                .get("status")
                .map(|s| format!("Invalid status filter: {}", s))
                .unwrap_or_else(|| "Missing 'status' query parameter".to_string());

            HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": msg
            }))
        }
    }
}

// ============================================================================
// CONFIRM DEPOSIT ADDRESS
// ============================================================================

#[post("/bridge/confirm-deposit")]
pub async fn confirm_deposit(
    req: HttpRequest,
    body: web::Bytes,
    app_state: web::Data<AppState>,
) -> impl Responder {
    if let Err(response) = validate_hmac(&req, &body, &app_state) {
        return response;
    }

    let request: ConfirmDepositRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Invalid request body",
                "error": e.to_string()
            }));
        }
    };

    match app_state.database.get_intent(&request.intent_id) {
        Ok(Some(intent)) => {
            if intent.status != IntentStatus::Batched && intent.status != IntentStatus::Pending {
                return HttpResponse::BadRequest().json(json!({
                    "success": false,
                    "message": format!("Intent is in {} state, cannot set deposit address", intent.status)
                }));
            }
        }
        Ok(None) => {
            return HttpResponse::NotFound().json(json!({
                "success": false,
                "message": "Intent not found"
            }));
        }
        Err(e) => {
            error!("Failed to get intent {}: {}", &request.intent_id[..10], e);
            return HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to retrieve intent"
            }));
        }
    }

    if let Err(e) = app_state
        .database
        .update_intent_deposit_address(&request.intent_id, &request.deposit_address)
    {
        error!("Failed to update deposit address: {}", e);
        return HttpResponse::InternalServerError().json(json!({
            "success": false,
            "message": "Failed to update deposit address"
        }));
    }

    info!(
        "Deposit address set for intent {}: {}",
        &request.intent_id[..10],
        &request.deposit_address
    );

    HttpResponse::Ok().json(json!({
        "success": true,
        "intent_id": request.intent_id,
        "deposit_address": request.deposit_address,
        "message": "Deposit address confirmed. Relay coordinator will start polling NEAR."
    }))
}

// ============================================================================
// INDEXER WEBHOOK
// ============================================================================

#[post("/indexer/event")]
pub async fn indexer_event(
    req: HttpRequest,
    app_state: web::Data<AppState>,
    body: web::Bytes,
) -> impl Responder {
    if let Ok(body_str) = std::str::from_utf8(&body) {
        debug!("Received indexer event: {}", body_str);
    }

    if let Err(response) = validate_hmac(&req, &body, &app_state) {
        return response;
    }

    let request: IndexerEventRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return HttpResponse::BadRequest().json(IndexerEventResponse {
                success: false,
                message: "Invalid request format".to_string(),
                error: Some(e.to_string()),
            });
        }
    };

    info!(
        "Indexer event: {} | chain: {} | tx: {}",
        request.event_type,
        request.chain,
        &request.transaction_hash[..18.min(request.transaction_hash.len())]
    );

    match request.event_type.as_str() {
        "commitment_added" => handle_commitment_added(&app_state, &request).await,
        "settled" | "settled_with_swap" => handle_settled_event(&app_state, &request).await,
        "marked_settled" | "intent_marked_settled" => handle_marked_settled_event(&app_state, &request).await,
        "merkle_root_updated" => handle_root_updated_event(&app_state, &request).await,
        _ => {
            warn!("Unknown event type: {}", request.event_type);
            HttpResponse::BadRequest().json(IndexerEventResponse {
                success: false,
                message: format!("Unknown event type: {}", request.event_type),
                error: None,
            })
        }
    }
}

// ============================================================================
// HEALTH & METRICS
// ============================================================================

/// Public health endpoint — minimal response for load balancers and uptime monitors.
/// Returns 200 OK when the service is running, 503 when the DB is unreachable.
/// Does NOT probe external chains (EVM/StarkNet/NEAR) to avoid false alarms.
#[get("/health")]
pub async fn health_check(app_state: web::Data<AppState>) -> impl Responder {
    let db_healthy = app_state.database.health_check().is_ok();

    let status_code = if db_healthy {
        actix_web::http::StatusCode::OK
    } else {
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    };

    HttpResponse::build(status_code).json(json!({
        "status": if db_healthy { "ok" } else { "unavailable" },
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

/// Operator health endpoint — full component status, HMAC-protected.
/// Returns per-component up/down for EVM, StarkNet, NEAR, and database.
/// Only expose to operators; never surface behind a public CDN.
#[get("/health/detailed")]
pub async fn health_check_detailed(
    req: HttpRequest,
    body: web::Bytes,
    app_state: web::Data<AppState>,
) -> impl Responder {
    if let Err(response) = validate_hmac(&req, &body, &app_state) {
        return response;
    }

    let db_healthy = app_state.database.health_check().is_ok();
    let evm_healthy = app_state.evm_relayer.health_check().await.is_ok();
    let starknet_healthy = app_state.starknet_relayer.health_check().await.is_ok();
    let near_healthy = app_state.near_client.health_check().await.is_ok();

    let overall = db_healthy && evm_healthy && starknet_healthy && near_healthy;

    let status_code = if overall {
        actix_web::http::StatusCode::OK
    } else {
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    };

    let component = |healthy: bool| if healthy { "up" } else { "down" };

    HttpResponse::build(status_code).json(json!({
        "status": if overall { "healthy" } else { "degraded" },
        "timestamp": Utc::now().to_rfc3339(),
        "components": {
            "database": component(db_healthy),
            "evm_relayer": component(evm_healthy),
            "starknet_relayer": component(starknet_healthy),
            "near_1click": component(near_healthy),
        }
    }))
}

/// Fix #8: Metrics endpoint — HMAC-protected to avoid leaking operational data.
#[get("/metrics")]
pub async fn get_metrics(
    req: HttpRequest,
    body: web::Bytes,
    app_state: web::Data<AppState>,
) -> impl Responder {
    if let Err(response) = validate_hmac(&req, &body, &app_state) {
        return response;
    }

    let count = |status: IntentStatus| -> u64 {
        app_state
            .database
            .get_intents_by_status(status)
            .map(|v| v.len() as u64)
            .unwrap_or(0)
    };

    let pending = count(IntentStatus::Pending) + count(IntentStatus::Batched);
    let in_flight = count(IntentStatus::NearSubmitted) + count(IntentStatus::TokensDelivered);
    let settled = count(IntentStatus::Settled) + count(IntentStatus::MarkedSettled);
    let failed = count(IntentStatus::Failed);
    let refunded = count(IntentStatus::Refunded);

    HttpResponse::Ok().json(MetricsResponse {
        status: "success".to_string(),
        data: MetricsData {
            total_intents: pending + in_flight + settled + failed + refunded,
            pending_intents: pending + in_flight,
            settled_intents: settled,
            failed_intents: failed,
            refunded_intents: refunded,
        },
    })
}

#[get("/")]
pub async fn root() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "service": "ShadowSwap Privacy Bridge",
        "version": "2.0.0",
        "status": "operational",
        "chains": ["ethereum", "starknet"],
        "bridge": "NEAR 1Click"
    }))
}

// ============================================================================
// HELPERS
// ============================================================================

fn intent_to_response(intent: &ShadowIntent) -> IntentStatusResponse {
    IntentStatusResponse {
        intent_id: intent.id.clone(),
        status: intent.status.as_str().to_string(),
        source_chain: intent.source_chain.as_chain_id_str().to_string(),
        dest_chain: intent.dest_chain.as_chain_id_str().to_string(),
        token: intent.token.clone(),
        amount: intent.amount.clone(),
        commitment: intent.commitment.clone(),
        deposit_address: intent.deposit_address.clone(),
        near_status: intent.near_status.clone(),
        dest_tx_hash: intent.dest_tx_hash.clone(),
        settle_tx_hash: intent.settle_tx_hash.clone(),
        source_settle_tx_hash: intent.source_settle_tx_hash.clone(),
        created_at: intent.created_at,
        updated_at: intent.updated_at,
    }
}
