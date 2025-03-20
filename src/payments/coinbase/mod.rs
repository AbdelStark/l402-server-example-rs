use crate::config::Config;
use crate::models::PaymentRequestDetails;
use anyhow::Result;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info};

/// Errors that can occur when interacting with Coinbase
#[derive(Debug, Error)]
pub enum CoinbaseError {
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// API error
    #[error("Coinbase API error: {0}")]
    ApiError(String),

    /// Missing configuration
    #[error("Missing Coinbase configuration: {0}")]
    ConfigError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// HMAC verification error
    #[error("HMAC verification error: {0}")]
    HmacError(String),

    /// Invalid webhook
    #[error("Invalid webhook: {0}")]
    InvalidWebhook(String),
}

/// Coinbase payment provider
#[derive(Debug, Clone)]
pub struct CoinbaseProvider {
    client: Client,
    config: Arc<Config>,
}

/// Request to create a Coinbase charge
#[derive(Debug, Serialize)]
struct CreateChargeRequest {
    /// Name of the product/service
    name: String,
    /// Description of the product/service
    description: String,
    /// Unique reference for the order
    #[serde(rename = "metadata")]
    metadata: HashMap<String, String>,
    /// Currency code (e.g., "USD")
    local_price: PriceInfo,
    /// Email to send receipt to (optional)
    pricing_type: String,
}

/// Price information for a charge
#[derive(Debug, Serialize)]
struct PriceInfo {
    /// Amount to charge
    amount: String,
    /// Currency code (e.g., "USD")
    currency: String,
}

/// Response from create charge request
#[derive(Debug, Deserialize)]
struct CreateChargeResponse {
    /// Data field containing charge details
    data: ChargeData,
}

/// Charge data from Coinbase response
#[derive(Debug, Deserialize)]
struct ChargeData {
    /// Charge ID
    id: String,
    /// URL to hosted checkout page
    hosted_url: String,
    /// Payment addresses for different cryptocurrencies
    addresses: HashMap<String, String>,
    /// Pricing information
    pricing: Option<PricingInfo>,
}

/// Pricing information from Coinbase response
#[derive(Debug, Deserialize)]
struct PricingInfo {
    /// Amount in local currency
    local: Option<LocalPrice>,
}

/// Local price information
#[derive(Debug, Deserialize)]
struct LocalPrice {
    /// Amount in local currency
    amount: String,
    /// Currency code
    currency: String,
}

/// Webhook event from Coinbase
#[derive(Debug, Deserialize)]
pub struct CoinbaseWebhookEvent {
    /// Type of event
    #[serde(rename = "type")]
    event_type: String,
    /// Data field containing event details
    data: WebhookData,
}

/// Webhook data from Coinbase
#[derive(Debug, Deserialize)]
struct WebhookData {
    /// Charge ID
    id: String,
    /// Status of the charge
    status: String,
}

impl CoinbaseProvider {
    /// Create a new Coinbase payment provider
    pub fn new(config: Arc<Config>) -> Result<Self, CoinbaseError> {
        // Ensure required configs are present
        if !config.coinbase_enabled {
            return Err(CoinbaseError::ConfigError(
                "Coinbase payments are not enabled".to_string(),
            ));
        }

        if config.coinbase_api_key.is_none() {
            return Err(CoinbaseError::ConfigError(
                "Coinbase API key not configured".to_string(),
            ));
        }

        // Create HTTP client
        let client = Client::new();

        Ok(Self { client, config })
    }

    /// Create a Coinbase charge for the specified amount
    pub async fn create_charge(
        &self,
        amount: f64,
        currency: &str,
        description: &str,
        reference: &str,
    ) -> Result<(String, String, Option<String>), CoinbaseError> {
        // Format the amount as a string with 2 decimal places
        let amount_str = format!("{:.2}", amount);

        // Create a metadata map for reference
        let mut metadata = HashMap::new();
        metadata.insert("reference".to_string(), reference.to_string());

        // Build the request
        let request = CreateChargeRequest {
            name: "API Credits".to_string(),
            description: description.to_string(),
            metadata,
            local_price: PriceInfo {
                amount: amount_str,
                currency: currency.to_string(),
            },
            pricing_type: "fixed_price".to_string(),
        };

        // Get Coinbase API key
        let api_key = self.config.coinbase_api_key.as_ref().unwrap();

        // Construct the API URL
        let url = "https://api.commerce.coinbase.com/charges";

        // Make the request to Coinbase
        let response = self
            .client
            .post(url)
            .header("X-CC-Api-Key", api_key)
            .header("X-CC-Version", "2018-03-22")
            .json(&request)
            .send()
            .await?;

        // Check for errors
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Coinbase returned error: {} - {}", status, error_text);
            return Err(CoinbaseError::ApiError(format!(
                "Coinbase API error: {}",
                error_text
            )));
        }

        // Parse the response
        let charge_response: CreateChargeResponse = response.json().await?;
        let charge = charge_response.data;

        // Get the first crypto address if available (for USDC or other preferred crypto)
        let usdc_address = charge.addresses.get("usdc").cloned();

        info!("Created Coinbase charge: {}", charge.id);
        Ok((charge.id, charge.hosted_url, usdc_address))
    }

    /// Verify a webhook signature from Coinbase
    pub fn verify_webhook(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<CoinbaseWebhookEvent, CoinbaseError> {
        // Get the webhook secret
        let webhook_secret = match &self.config.coinbase_webhook_secret {
            Some(secret) => secret,
            None => {
                return Err(CoinbaseError::ConfigError(
                    "Coinbase webhook secret not configured".to_string(),
                ));
            }
        };

        // Create HMAC with the webhook secret
        let mut mac = Hmac::<Sha256>::new_from_slice(webhook_secret.as_bytes())
            .map_err(|e| CoinbaseError::HmacError(e.to_string()))?;

        // Update the HMAC with the request body
        mac.update(body);

        // Calculate the HMAC digest
        let hmac_bytes = mac.finalize().into_bytes();
        let calculated_signature = hex::encode(hmac_bytes);

        // Compare the signatures (constant time comparison for security)
        if !constant_time_eq(calculated_signature.as_bytes(), signature.as_bytes()) {
            return Err(CoinbaseError::InvalidWebhook(
                "Invalid signature".to_string(),
            ));
        }

        // Parse the webhook event
        let event: CoinbaseWebhookEvent =
            serde_json::from_slice(body).map_err(CoinbaseError::SerializationError)?;

        Ok(event)
    }

    /// Check if a webhook event represents a completed payment
    pub fn is_payment_completed(&self, event: &CoinbaseWebhookEvent) -> bool {
        // Check if the event type is a charge with confirmed status
        event.event_type == "charge:confirmed" && event.data.status == "CONFIRMED"
    }

    /// Get the charge ID from a webhook event
    pub fn get_charge_id<'a>(&self, event: &'a CoinbaseWebhookEvent) -> &'a str {
        &event.data.id
    }

    /// Generate payment details for the client
    pub fn generate_payment_details(
        &self,
        checkout_url: &str,
        address: Option<&str>,
        chain: Option<&str>,
        asset: Option<&str>,
    ) -> PaymentRequestDetails {
        PaymentRequestDetails::Coinbase {
            checkout_url: checkout_url.to_string(),
            address: address.map(|s| s.to_string()),
            chain: chain.map(|s| s.to_string()),
            asset: asset.map(|s| s.to_string()),
        }
    }
}

/// Constant-time comparison of two byte slices
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }

    result == 0
}
