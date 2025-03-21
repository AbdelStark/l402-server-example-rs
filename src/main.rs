mod api;
mod config;
mod models;
mod payments;
mod services;
mod storage;
mod utils;

use anyhow::Result;
use config::Config;
use payments::PaymentService;
use services::BlockService;
use storage::RedisStorage;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting L402 server...");

    // Load configuration
    let config = Config::from_env();
    info!("Server will run on {}:{}", config.host, config.port);

    // Initialize Redis storage
    let redis_url = config.redis_url.clone();
    let storage = match RedisStorage::new(&redis_url) {
        Ok(storage) => {
            info!("Connected to Redis at {}", redis_url);
            storage
        }
        Err(e) => {
            error!("Failed to connect to Redis: {}", e);
            return Err(anyhow::anyhow!("{}", e));
        }
    };

    // Check Redis connection
    if let Err(e) = storage.check_connection().await {
        error!("Redis connection test failed: {}", e);
        return Err(anyhow::anyhow!("{}", e));
    }

    // Create shared config
    let config_arc = config.into_arc();

    // Initialize payment service
    let payment_service = match PaymentService::new(config_arc.clone(), storage.clone()) {
        Ok(service) => {
            info!("Payment service initialized");
            service
        }
        Err(e) => {
            error!("Failed to initialize payment service: {}", e);
            return Err(anyhow::anyhow!("{}", e));
        }
    };

    // Initialize block service
    let block_service = BlockService::new(storage.clone());
    info!("Block service initialized");

    // Create the router
    let app = api::create_router(
        config_arc.clone(),
        storage.clone(),
        payment_service,
        block_service,
    );

    // Start the server
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config_arc.host, config_arc.port)).await?;
    info!("Server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
