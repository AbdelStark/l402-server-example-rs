use crate::api::handlers;
use crate::config::Config;
use crate::payments::PaymentService;
use crate::services::StockService;
use crate::storage::RedisStorage;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Create the API router with all routes
/// Application state shared between handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub storage: RedisStorage,
    pub payment_service: PaymentService,
    pub stock_service: StockService,
}

pub fn create_router(
    config: Arc<Config>,
    storage: RedisStorage,
    payment_service: PaymentService,
    stock_service: StockService,
) -> Router {
    // Create a CORS layer to allow cross-origin requests
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Public routes that don't require authentication
    let public_routes = Router::new()
        .route("/signup", get(handlers::signup))
        .route("/l402/payment-request", post(handlers::initiate_payment))
        .route("/webhook/lightning", post(handlers::lightning_webhook))
        .route("/webhook/coinbase", post(handlers::coinbase_webhook));

    // Protected routes that require authentication
    let protected_routes = Router::new()
        .route("/info", get(handlers::get_user_info))
        .route("/ticker/:symbol", get(handlers::get_ticker));

    let state = AppState {
        config,
        storage,
        payment_service,
        stock_service,
    };

    // Combine all routes with shared state
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
