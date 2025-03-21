use crate::api::auth::UserId;
use crate::models::{PaymentRequestInput, PaymentRequestResponse, PaymentRequiredResponse, User};
use crate::storage::StorageError;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{Duration, Utc};
use serde_json::json;
use tracing::{error, info};

/// Handler for creating a new user
pub async fn signup(State(state): State<crate::api::routes::AppState>) -> impl IntoResponse {
    let storage = &state.storage;
    // Create a new user with 1 free credit
    let user = User::new(1);

    // Store the user in Redis
    match storage.create_user(&user).await {
        Ok(_) => {
            info!("Created new user: {}", user.id);
            (StatusCode::OK, Json(user)).into_response()
        }
        Err(e) => {
            error!("Error creating user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to create user"})),
            )
                .into_response()
        }
    }
}

/// Handler for retrieving user info
pub async fn get_user_info(
    State(state): State<crate::api::routes::AppState>,
    UserId(user_id): UserId,
) -> impl IntoResponse {
    let storage = &state.storage;
    match storage.get_user(&user_id).await {
        Ok(user) => (StatusCode::OK, Json(user)).into_response(),
        Err(StorageError::UserNotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        )
            .into_response(),
        Err(e) => {
            error!("Error retrieving user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to retrieve user"})),
            )
                .into_response()
        }
    }
}

/// Handler for initiating a payment
#[axum::debug_handler]
pub async fn initiate_payment(
    State(state): State<crate::api::routes::AppState>,
    Json(input): Json<PaymentRequestInput>,
) -> impl IntoResponse {
    let payment_service = &state.payment_service;
    match payment_service.process_payment_request(input).await {
        Ok((request, payment_details)) => {
            // Create the response
            let response = PaymentRequestResponse {
                payment_request: payment_details,
                offer_id: request.offer_id,
                expires_at: request.expires_at,
            };

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Error processing payment request: {}", e);
            let message = match e {
                _ if e.to_string().contains("Offer not found") => "Invalid offer ID",
                _ if e.to_string().contains("Invalid payment method") => "Invalid payment method",
                _ if e.to_string().contains("User not found") => "Invalid user token",
                _ => "Failed to process payment request",
            };

            let status = if message == "Invalid user token" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::BAD_REQUEST
            };

            (status, Json(json!({"error": message}))).into_response()
        }
    }
}

/// Handler for Lightning webhooks
pub async fn lightning_webhook(
    State(state): State<crate::api::routes::AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let payment_service = &state.payment_service;
    // Get the signature from headers
    let signature = headers
        .get("X-Lightning-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Process the webhook
    match payment_service
        .process_lightning_webhook(&body, signature)
        .await
    {
        Ok(Some(user_id)) => {
            info!("Processed Lightning payment for user {}", user_id);
            StatusCode::OK
        }
        Ok(None) => {
            // Webhook was valid but no action was taken (e.g., already processed)
            StatusCode::OK
        }
        Err(e) => {
            error!("Error processing Lightning webhook: {}", e);
            StatusCode::BAD_REQUEST
        }
    }
}

/// Handler for retrieving Bitcoin latest block hash
pub async fn get_latest_block(
    State(state): State<crate::api::routes::AppState>,
    UserId(user_id): UserId,
) -> impl IntoResponse {
    let storage = &state.storage;
    let block_service = &state.block_service;
    let config = &state.config;

    // Get the user
    let user = match storage.get_user(&user_id).await {
        Ok(user) => user,
        Err(StorageError::UserNotFound) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "User not found"})),
            )
                .into_response();
        }
        Err(e) => {
            error!("Error retrieving user: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to retrieve user"})),
            )
                .into_response();
        }
    };

    // Check if the user has enough credits
    if user.credits < 1 {
        // User is out of credits, return 402 Payment Required
        info!("User {} is out of credits", user_id);

        // Create the payment required response
        let expiry = Utc::now() + Duration::minutes(30);
        let payment_required = PaymentRequiredResponse {
            expiry,
            offers: config.offers.clone(),
            payment_context_token: user_id,
            payment_request_url: format!(
                "http://{}:{}/l402/payment-request",
                config.host, config.port
            ),
        };

        return (StatusCode::PAYMENT_REQUIRED, Json(payment_required)).into_response();
    }

    // User has credits, try to get the latest block data
    match block_service.get_latest_block().await {
        Ok(block_data) => {
            // Deduct one credit for the successful request
            match storage.update_user_credits(&user_id, -1).await {
                Ok(_) => {
                    info!("User {} used 1 credit for latest block hash", user_id);
                }
                Err(e) => {
                    error!("Failed to deduct credit from user {}: {}", user_id, e);
                    // Continue anyway since the data was fetched
                }
            }

            // Return the block data
            (StatusCode::OK, Json(block_data)).into_response()
        }
        Err(e) => {
            error!("Error fetching latest block hash: {}", e);

            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": format!("Failed to fetch latest block hash: {}", e)})),
            )
                .into_response()
        }
    }
}

/// Handler for Coinbase webhooks
pub async fn coinbase_webhook(
    State(state): State<crate::api::routes::AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let payment_service = &state.payment_service;
    // Get the signature from headers
    let signature = headers
        .get("X-CC-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Process the webhook
    match payment_service
        .process_coinbase_webhook(&body, signature)
        .await
    {
        Ok(Some(user_id)) => {
            info!("Processed Coinbase payment for user {}", user_id);
            StatusCode::OK
        }
        Ok(None) => {
            // Webhook was valid but no action was taken (e.g., already processed)
            StatusCode::OK
        }
        Err(e) => {
            error!("Error processing Coinbase webhook: {}", e);
            StatusCode::BAD_REQUEST
        }
    }
}
