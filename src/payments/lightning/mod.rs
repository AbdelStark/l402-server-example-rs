use crate::config::Config;
use crate::models::PaymentRequestDetails;
use crate::utils::{self, ConversionError};
use anyhow::Result;
use lnbits_rs::{LNBitsClient, api::invoice::CreateInvoiceRequest};
use reqwest::Client as HttpClient;
use serde::Deserialize;
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

    /// LNBits client error
    #[error("LNBits error: {0}")]
    LNBitsError(String),

    /// Currency conversion error
    #[error("Currency conversion error: {0}")]
    ConversionError(#[from] ConversionError),
}

/// Lightning payment provider using LNBits
#[derive(Clone)]
pub struct LightningProvider {
    http_client: HttpClient,
    lnbits_client: Option<LNBitsClient>,
    config: Arc<Config>,
}

/// Invoice webhook event data from LNBits
#[derive(Debug, Deserialize)]
pub struct WebhookEvent {
    /// Payment hash
    pub payment_hash: String,
    /// Status of the payment (true if paid)
    pub payment_status: bool,
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

        // Create HTTP client
        let http_client = HttpClient::new();

        // Check for LNBits configuration
        let lnbits_client = if let (Some(url), Some(admin_key), Some(invoice_read_key)) = (
            &config.lnbits_url,
            &config.lnbits_admin_key,
            &config.lnbits_invoice_read_key,
        ) {
            // Create LNBits client
            match LNBitsClient::new("", admin_key, invoice_read_key, url, None) {
                Ok(client) => {
                    info!("LNBits client initialized with admin key");
                    Some(client)
                }
                Err(err) => {
                    error!("Failed to initialize LNBits client: {}", err);
                    return Err(LightningError::LNBitsError(err.to_string()));
                }
            }
        } else {
            None
        };

        Ok(Self {
            http_client,
            lnbits_client,
            config,
        })
    }

    /// Create a Lightning invoice for the specified amount
    pub async fn create_invoice(
        &self,
        amount_usd: f64,
        description: &str,
    ) -> Result<(String, String), LightningError> {
        // Get the LNBits client
        let client = self.lnbits_client.as_ref().ok_or_else(|| {
            LightningError::ConfigError("LNBits client not configured".to_string())
        })?;

        // Convert USD to satoshis using market rate
        let amount_sats = utils::convert_usd_to_sats(amount_usd).await?;

        // Create invoice with 30 minute expiry (in seconds)
        let expiry = 30 * 60;

        let invoice_request = CreateInvoiceRequest {
            amount: amount_sats,
            memo: Some(description.to_string()),
            unit: "sat".to_string(),
            expiry: Some(expiry),
            webhook: None,
            internal: None,
            out: false,
        };

        // Create the invoice
        let invoice = client
            .create_invoice(&invoice_request)
            .await
            .map_err(|e| LightningError::ApiError(format!("Failed to create invoice: {}", e)))?;

        info!("Created Lightning invoice for {} sats", amount_sats);

        // Return the payment request (BOLT11 invoice) and payment hash
        Ok((invoice.payment_request, invoice.payment_hash))
    }

    /// Check if a payment has been settled
    pub async fn check_invoice(&self, payment_hash: &str) -> Result<bool, LightningError> {
        // Get the LNBits client
        let client = self.lnbits_client.as_ref().ok_or_else(|| {
            LightningError::ConfigError("LNBits client not configured".to_string())
        })?;

        // Check the payment status
        let invoice_paid = client.is_invoice_paid(payment_hash).await.map_err(|e| {
            LightningError::ApiError(format!("Failed to check invoice status: {}", e))
        })?;

        Ok(invoice_paid)
    }

    /// Verify a webhook payload from Lightning provider
    pub fn verify_webhook(
        &self,
        body: &[u8],
        _signature: &str,
    ) -> Result<WebhookEvent, LightningError> {
        // For simplicity, this implementation doesn't verify signatures
        // In a production environment, you should verify using your LNBits webhook key

        // Try to parse the webhook event
        let event: WebhookEvent =
            serde_json::from_slice(body).map_err(|e| LightningError::SerializationError(e))?;

        debug!(
            "Parsed webhook event for payment_hash: {}",
            event.payment_hash
        );

        Ok(event)
    }

    /// Generate payment details for the client
    pub fn generate_payment_details(&self, invoice: &str) -> PaymentRequestDetails {
        PaymentRequestDetails::Lightning {
            lightning_invoice: invoice.to_string(),
        }
    }
}
