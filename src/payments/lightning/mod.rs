use crate::config::Config;
use crate::models::PaymentRequestDetails;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur when interacting with Lightning
#[derive(Debug, Error)]
pub enum LightningError {
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// API error
    #[error("Lightning API error: {0}")]
    ApiError(String),

    /// Missing configuration
    #[error("Missing Lightning configuration: {0}")]
    ConfigError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Lightning payment provider
#[derive(Debug, Clone)]
pub struct LightningProvider {
    client: Client,
    config: Arc<Config>,
}

/// Request to create a Lightning invoice
#[derive(Debug, Serialize)]
struct CreateInvoiceRequest {
    /// Amount in satoshis
    #[serde(rename = "value")]
    amount_sats: u64,
    /// Invoice memo/description
    memo: String,
    /// Expiry in seconds
    expiry: u64,
}

/// Response from create invoice request
#[derive(Debug, Deserialize)]
struct CreateInvoiceResponse {
    /// Payment request (BOLT11 invoice string)
    #[serde(rename = "payment_request")]
    payment_request: String,
    /// Payment hash
    r_hash: String,
}

impl LightningProvider {
    /// Create a new Lightning payment provider
    pub fn new(config: Arc<Config>) -> Result<Self, LightningError> {
        // Ensure required configs are present
        if !config.lightning_enabled {
            return Err(LightningError::ConfigError(
                "Lightning payments are not enabled".to_string(),
            ));
        }

        if config.lnd_rest_endpoint.is_none() {
            return Err(LightningError::ConfigError(
                "LND REST endpoint not configured".to_string(),
            ));
        }

        if config.lnd_macaroon_hex.is_none() {
            return Err(LightningError::ConfigError(
                "LND macaroon not configured".to_string(),
            ));
        }

        // Create HTTP client
        let client = Client::new();

        Ok(Self { client, config })
    }

    /// Create a Lightning invoice for the specified amount
    pub async fn create_invoice(
        &self,
        amount_usd: f64,
        description: &str,
    ) -> Result<(String, String), LightningError> {
        // Convert USD to satoshis
        let amount_sats = self.convert_usd_to_sats(amount_usd).await?;

        // Create invoice with 30 minute expiry
        let expiry = 30 * 60; // 30 minutes in seconds

        // Build the request to LND
        let request = CreateInvoiceRequest {
            amount_sats,
            memo: description.to_string(),
            expiry,
        };

        // Get LND endpoint and macaroon
        let endpoint = self.config.lnd_rest_endpoint.as_ref().unwrap();
        let macaroon = self.config.lnd_macaroon_hex.as_ref().unwrap();

        // Construct the full URL
        let url = format!("{}/v1/invoices", endpoint);

        // Make the request to LND
        let response = self
            .client
            .post(&url)
            .header("Grpc-Metadata-macaroon", macaroon)
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
            error!("LND returned error: {} - {}", status, error_text);
            return Err(LightningError::ApiError(format!(
                "LND API error: {}",
                error_text
            )));
        }

        // Parse the response
        let invoice_response: CreateInvoiceResponse = response.json().await?;

        info!("Created Lightning invoice for {} sats", amount_sats);
        Ok((invoice_response.payment_request, invoice_response.r_hash))
    }

    /// Convert USD to satoshis using current exchange rate
    async fn convert_usd_to_sats(&self, amount_usd: f64) -> Result<u64, LightningError> {
        // For simplicity, we'll use a fixed exchange rate
        // In a production environment, you'd call an API to get the current rate

        // Example: Assume 1 BTC = $50,000 USD
        // Then 1 sat = $0.0000005 USD (1/100,000,000 of $50,000)
        // So $1 USD = 2,000,000 sats

        const SATS_PER_USD: f64 = 2_000_000.0;

        // Convert to satoshis (rounding up to nearest sat)
        let amount_sats = (amount_usd * SATS_PER_USD).ceil() as u64;

        debug!("Converted ${} USD to {} sats", amount_usd, amount_sats);
        Ok(amount_sats)
    }

    /// Verify a webhook payload from Lightning provider
    pub fn verify_webhook(&self, _body: &[u8], _signature: &str) -> Result<bool, LightningError> {
        // In a real implementation, you would verify the signature here
        // For this example, we'll just return true
        Ok(true)
    }

    /// Generate payment details for the client
    pub fn generate_payment_details(&self, invoice: &str) -> PaymentRequestDetails {
        PaymentRequestDetails::Lightning {
            lightning_invoice: invoice.to_string(),
        }
    }
}
