use crate::config::Config;
use crate::models::PaymentRequestDetails;
use anyhow::Result;
use lnbits_rs::{Client as LNBitsClient, Invoice, Payment, PaymentStatus};
use reqwest::Client as HttpClient;
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
    
    /// LNBits client error
    #[error("LNBits error: {0}")]
    LNBitsError(#[from] lnbits_rs::Error),
}

/// Lightning payment provider using LNBits
#[derive(Debug, Clone)]
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
        let lnbits_client = if let (Some(url), Some(admin_key)) = (&config.lnbits_url, &config.lnbits_admin_key) {
            // Create LNBits client
            match LNBitsClient::new(url, admin_key) {
                Ok(client) => {
                    info!("LNBits client initialized with admin key");
                    Some(client)
                }
                Err(err) => {
                    error!("Failed to initialize LNBits client: {}", err);
                    return Err(LightningError::LNBitsError(err));
                }
            }
        } else {
            // Check for legacy LND config as fallback
            if config.lnd_rest_endpoint.is_none() {
                return Err(LightningError::ConfigError(
                    "Neither LNBits nor LND configuration provided".to_string(),
                ));
            }
            
            // Using legacy LND client (not supported in this implementation)
            error!("Legacy LND REST API no longer supported - please use LNBits");
            return Err(LightningError::ConfigError(
                "Legacy LND REST API no longer supported - please use LNBits".to_string(),
            ));
        };

        Ok(Self { 
            http_client, 
            lnbits_client,
            config 
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
        
        // Convert USD to satoshis
        let amount_sats = self.convert_usd_to_sats(amount_usd).await?;

        // Create invoice with 30 minute expiry (in seconds)
        let expiry = 30 * 60;

        // Create the invoice
        let invoice = client.create_invoice(
            amount_sats,
            description.to_string(),
            Some(expiry),
            None, // webhook URL will be configured externally
        ).await?;

        info!("Created Lightning invoice for {} sats", amount_sats);
        
        // Return the payment request (BOLT11 invoice) and payment hash
        Ok((invoice.payment_request, invoice.payment_hash))
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

    /// Check if a payment has been settled
    pub async fn check_invoice(&self, payment_hash: &str) -> Result<bool, LightningError> {
        // Get the LNBits client with invoice read key
        let invoice_read_key = self.config.lnbits_invoice_read_key.as_ref()
            .ok_or_else(|| LightningError::ConfigError("LNBits invoice read key not configured".to_string()))?;
        
        // Use the URL from the admin client
        let url = self.config.lnbits_url.as_ref()
            .ok_or_else(|| LightningError::ConfigError("LNBits URL not configured".to_string()))?;
            
        // Create a client with invoice read key for checking payment status
        let client = LNBitsClient::new(url, invoice_read_key)?;
        
        // Check the payment status
        let payment = client.get_payment(payment_hash).await?;
        
        Ok(payment.paid)
    }

    /// Verify a webhook payload from Lightning provider
    pub fn verify_webhook(&self, body: &[u8], signature: &str) -> Result<WebhookEvent, LightningError> {
        // For simplicity, this implementation doesn't verify signatures
        // In a production environment, you should verify using your LNBits webhook key
        
        // Try to parse the webhook event
        let event: WebhookEvent = serde_json::from_slice(body)
            .map_err(|e| LightningError::SerializationError(e))?;
        
        debug!("Parsed webhook event for payment_hash: {}", event.payment_hash);
        
        Ok(event)
    }

    /// Generate payment details for the client
    pub fn generate_payment_details(&self, invoice: &str) -> PaymentRequestDetails {
        PaymentRequestDetails::Lightning {
            lightning_invoice: invoice.to_string(),
        }
    }
}
