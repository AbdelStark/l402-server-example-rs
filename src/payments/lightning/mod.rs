use crate::config::Config;
use crate::models::PaymentRequestDetails;
use crate::payments::lnbits::{CreateInvoiceRequest, LNBitsClient, LNBitsError};
use crate::utils::ConversionError;
use anyhow::Result;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

/// Errors that can occur when interacting with Lightning
#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum LightningError {
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Missing configuration
    #[error("Missing configuration: {0}")]
    ConfigError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// LNBits client error
    #[error("LNBits error: {0}")]
    LNBitsError(#[from] LNBitsError),

    /// Currency conversion error
    #[error("Currency conversion error: {0}")]
    ConversionError(#[from] ConversionError),
}

/// Lightning payment provider using LNBits
#[derive(Clone)]
pub struct LightningProvider {
    lnbits_client: Option<LNBitsClient>,
}

/// Invoice webhook event data from LNBits
#[derive(Debug, Deserialize)]
pub struct WebhookEvent {
    /// Payment hash
    pub payment_hash: String,
}

impl LightningProvider {
    /// Create a new Lightning payment provider
    pub fn new(config: Arc<Config>) -> Result<Self, LightningError> {
        let lnbits_client = if config.lightning_enabled {
            // Check if all required LNBits configs are present
            let url = config.lnbits_url.as_ref().ok_or_else(|| {
                LightningError::ConfigError("LNBits URL not configured".to_string())
            })?;
            let admin_key = config.lnbits_admin_key.as_ref().ok_or_else(|| {
                LightningError::ConfigError("LNBits admin key not configured".to_string())
            })?;
            let invoice_read_key = config.lnbits_invoice_read_key.as_ref().ok_or_else(|| {
                LightningError::ConfigError("LNBits invoice read key not configured".to_string())
            })?;

            Some(
                LNBitsClient::new(
                    "default", // wallet_id - not used but required
                    admin_key,
                    invoice_read_key,
                    url,
                    None, // cert_path
                )
                .map_err(LightningError::LNBitsError)?,
            )
        } else {
            None
        };

        Ok(Self { lnbits_client })
    }

    /// Create a Lightning invoice for the specified amount
    pub async fn create_invoice(
        &self,
        amount_sats: u64,
        memo: &str,
    ) -> Result<(String, String), LightningError> {
        let client = self
            .lnbits_client
            .as_ref()
            .ok_or_else(|| LightningError::ConfigError("LNBits not configured".to_string()))?;

        let invoice_request = CreateInvoiceRequest {
            amount: amount_sats,
            memo: Some(memo.to_owned()),
            unit: "sat".to_string(),
            expiry: Some(1800), // 30 minutes
            webhook: None,      // We'll use polling instead
            internal: false,
            out: false,
        };

        let invoice = client.create_invoice(&invoice_request).await?;
        let payment_hash = invoice.payment_hash.clone();

        Ok((invoice.bolt11, payment_hash))
    }

    /// Check if an invoice has been paid
    pub async fn check_invoice(&self, payment_hash: &str) -> Result<bool, LightningError> {
        let client = self
            .lnbits_client
            .as_ref()
            .ok_or_else(|| LightningError::ConfigError("LNBits not configured".to_string()))?;

        let status = client.is_invoice_paid(payment_hash).await?;
        Ok(status)
    }

    /// Verify a webhook signature and parse the event
    pub fn verify_webhook(
        &self,
        body: &[u8],
        _signature: &str,
    ) -> Result<WebhookEvent, LightningError> {
        // For now, we just parse the body without verifying the signature
        // since LNBits doesn't provide webhook signatures
        let event: WebhookEvent = serde_json::from_slice(body)?;
        Ok(event)
    }

    /// Generate payment details for the client
    pub fn generate_payment_details(&self, invoice: &str) -> PaymentRequestDetails {
        PaymentRequestDetails::Lightning {
            lightning_invoice: invoice.to_string(),
        }
    }
}
