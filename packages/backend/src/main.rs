mod api;
mod config;
mod database;
mod encryption;
mod evm;
mod merkle_manager;
mod models;
mod near_client;
mod relay_coordinator;
mod root_sync_coordinator;
mod starknet;

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{middleware::Logger, web, App, HttpServer};
use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::{
    config::config::AppConfig,
    database::db::Database,
    evm::relayer::EvmRelayer,
    merkle_manager::tree_manager::MerkleTreeManager,
    near_client::model::NearClient,
    relay_coordinator::{
        relay_coordinator::RelayCoordinator, settlement_coordinator::SettlementCoordinator,
    },
    root_sync_coordinator::root_sync_coord::RootSyncCoordinator,
    starknet::relayer::StarkNetRelayer,
};

pub struct AppState {
    pub database: Arc<Database>,
    pub evm_relayer: Arc<EvmRelayer>,
    pub starknet_relayer: Arc<StarkNetRelayer>,
    pub near_client: Arc<NearClient>,
    pub config: AppConfig,
}

#[actix_web::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shadowswap=info,actix_web=info".into()),
        )
        .init();

    info!("Starting ShadowSwap Privacy Bridge Relayer");

    // ── Config ──────────────────────────────────────────────────────
    let config = AppConfig::from_env().context("Failed to load configuration")?;

    // ── Database ────────────────────────────────────────────────────
    let database = Arc::new(Database::from_env().context("Failed to initialize database")?);
    database
        .run_migrations()
        .context("Failed to run database migrations")?;
    info!("Database initialized and migrations applied");

    // ── Chain relayers ──────────────────────────────────────────────
    let evm_relayer = Arc::new(
        EvmRelayer::new(
            &config.evm.rpc_url,
            &config.evm.settlement_address,
            Some(config.evm.private_key.as_str()),
            config.evm.owner_private_key.as_deref(),
        )
        .await
        .context("Failed to initialize EVM relayer")?,
    );
    info!("EVM relayer initialized");

    let starknet_contract_address = config
        .starknet
        .contract_address
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("STARKNET_CONTRACT_ADDRESS must be set"))?;

    let starknet_relayer = Arc::new(
        StarkNetRelayer::new(
            &config.starknet.rpc_url,
            starknet_contract_address,
            Some(config.starknet.private_key.as_str()),
            config.starknet.account_address.as_deref(),
            &config.starknet.chain_id,
        )
        .await
        .context("Failed to initialize StarkNet relayer")?,
    );
    info!("StarkNet relayer initialized");

    // ── NEAR 1Click client ──────────────────────────────────────────
    let near_client = Arc::new(
        NearClient::new(config.near.api_key.as_deref())
            .context("Failed to initialize NEAR client")?,
    );
    info!("NEAR 1Click client initialized");

    // ── Merkle manager ──────────────────────────────────────────────
    let merkle_manager = Arc::new(MerkleTreeManager::new(database.clone()));

    if let Err(e) = merkle_manager.load_from_store().await {
        error!("Failed to load Merkle state from DB: {}", e);
    } else {
        info!("Merkle trees loaded from database");
    }

    // ── Root sync (must init before relay_coordinator) ──────────────
    let root_sync_coordinator = Arc::new(RootSyncCoordinator::new(
        database.clone(),
        evm_relayer.clone(),
        starknet_relayer.clone(),
        merkle_manager.clone(),
        30,
    ));

    // ── Background services ─────────────────────────────────────────
    let relay_handle = spawn_service("Relay coordinator", {
        let coordinator = RelayCoordinator::new(
            database.clone(),
            evm_relayer.clone(),
            starknet_relayer.clone(),
            near_client.clone(),
            merkle_manager.clone(),
            10,
        );
        async move { coordinator.start().await }
    });

    let settlement_handle = spawn_service("Settlement coordinator", {
        let coordinator = SettlementCoordinator::new(
            database.clone(),
            evm_relayer.clone(),
            starknet_relayer.clone(),
            near_client.clone(),
            merkle_manager.clone(),
            config.server.relayer_private_key.clone(),
            15,
            5,
        );
        async move { coordinator.run().await }
    });

    let root_sync_handle = spawn_service("Root sync coordinator", {
        let coordinator = root_sync_coordinator.clone();
        async move {
            coordinator.run().await;
            Ok(())
        }
    });

    // ── HTTP server ─────────────────────────────────────────────────
    let app_state = web::Data::new(AppState {
        database: database.clone(),
        evm_relayer: evm_relayer.clone(),
        starknet_relayer: starknet_relayer.clone(),
        near_client: near_client.clone(),
        config: config.clone(),
    });

    let host = config.server.host.clone();
    let port = config.server.port;

    info!("Starting HTTP server on {}:{}", host, port);

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _| {
                let s = origin.to_str().unwrap_or("");
                s == "https://shadowswap.vercel.app"
                    || s.starts_with("http://localhost")
                    || s == std::env::var("CORS_ORIGIN").unwrap_or_default().as_str()
            })
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::CONTENT_TYPE,
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::HeaderName::from_static("x-timestamp"),
                actix_web::http::header::HeaderName::from_static("x-signature"),
            ])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(app_state.clone())
            .configure(crate::config::config_scope::configure)
    })
    .bind((host.as_str(), port))
    .context("Failed to bind HTTP server")?
    .run();

    info!("All services started");

    tokio::select! {
        result = server => {
            error!("HTTP server exited: {:?}", result);
        }
        _ = relay_handle => {
            error!("Relay coordinator exited unexpectedly");
        }
        _ = settlement_handle => {
            error!("Settlement coordinator exited unexpectedly");
        }
        _ = root_sync_handle => {
            error!("Root sync coordinator exited unexpectedly");
        }
    }

    Ok(())
}

fn spawn_service<F>(name: &'static str, fut: F) -> JoinHandle<()>
where
    F: std::future::Future<Output = Result<()>> + Send + 'static,
{
    tokio::task::spawn(async move {
        info!("Starting {}", name);
        if let Err(e) = fut.await {
            error!("{} failed: {:?}", name, e);
        }
    })
}
