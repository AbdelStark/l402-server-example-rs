pub mod coinbase;
pub mod lightning;

use crate::config::{Config, Offer};
use crate::models::{
    PaymentMethod, PaymentRequest, PaymentRequestDetails, PaymentRequestInput, PaymentStatus,
};
use crate::storage::{RedisStorage, StorageError};
use anyhow::Result;
use chrono::{Duration, Utc};
use coinbase::CoinbaseProvider;
use lightning::LightningProvider;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info};

/// Errors that can occur in payment processing
#[derive(Debug, Error)]
pub enum PaymentError {
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),

    /// Lightning error
    #[error("Lightning error: {0}")]
    LightningError(#[from] lightning::LightningError),

    /// Coinbase error
    #[error("Coinbase error: {0}")]
    CoinbaseError(#[from] coinbase::CoinbaseError),

    /// Invalid payment method
    #[error("Invalid payment method: {0:?}")]
    InvalidPaymentMethod(PaymentMethod),

    /// Offer not found
    #[error("Offer not found: {0}")]
    OfferNotFound(String),

    /// User not found
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Service for handling payments
#[derive(Clone)]
pub struct PaymentService {
    storage: RedisStorage,
    config: Arc<Config>,
    lightning_provider: Option<LightningProvider>,
    coinbase_provider: Option<CoinbaseProvider>,
}

impl PaymentService {
    /// Create a new payment service
    pub fn new(config: Arc<Config>, storage: RedisStorage) -> Result<Self, PaymentError> {
        // Initialize payment providers based on configuration
        let lightning_provider = if config.lightning_enabled {
            match LightningProvider::new(Arc::clone(&config)) {
                Ok(provider) => {
                    info!("Lightning payment provider initialized");
                    Some(provider)
                }
                Err(err) => {
                    error!("Failed to initialize Lightning provider: {}", err);
                    None
                }
            }
        } else {
            None
        };

        let coinbase_provider = if config.coinbase_enabled {
            match CoinbaseProvider::new(Arc::clone(&config)) {
                Ok(provider) => {
                    info!("Coinbase payment provider initialized");
                    Some(provider)
                }
                Err(err) => {
                    error!("Failed to initialize Coinbase provider: {}", err);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            storage,
            config,
            lightning_provider,
            coinbase_provider,
        })
    }

    /// Process a payment request
    pub async fn process_payment_request(
        &self,
        input: PaymentRequestInput,
    ) -> Result<(PaymentRequest, PaymentRequestDetails), PaymentError> {
        // Get the offer being purchased
        let offer = self.get_offer(&input.offer_id)?;

        // Validate the payment method
        match input.payment_method {
            PaymentMethod::Lightning => {
                if self.lightning_provider.is_none() {
                    return Err(PaymentError::InvalidPaymentMethod(input.payment_method));
                }
            }
            PaymentMethod::Coinbase => {
                if self.coinbase_provider.is_none() {
                    return Err(PaymentError::InvalidPaymentMethod(input.payment_method));
                }
            }
        }

        // Create a payment request
        let expires_at = Utc::now() + Duration::minutes(30); // 30 minute expiry
        let mut payment_request = PaymentRequest::new(
            input.payment_context_token.clone(),
            offer.id.clone(),
            offer.credits,
            input.payment_method,
            expires_at,
        );

        // Process the payment based on the selected method
        let payment_details = match input.payment_method {
            PaymentMethod::Lightning => {
                self.create_lightning_payment(&mut payment_request, offer)
                    .await?
            }
            PaymentMethod::Coinbase => {
                self.create_coinbase_payment(
                    &mut payment_request,
                    offer,
                    input.chain.as_deref(),
                    input.asset.as_deref(),
                )
                .await?
            }
        };

        // Store the payment request
        self.storage.store_payment_request(&payment_request).await?;

        Ok((payment_request, payment_details))
    }

    /// Create a Lightning payment
    async fn create_lightning_payment(
        &self,
        payment_request: &mut PaymentRequest,
        offer: &Offer,
    ) -> Result<PaymentRequestDetails, PaymentError> {
        let provider = self
            .lightning_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Lightning))?;

        // Create a description for the invoice
        let description = format!(
            "Purchase {} credits for API access - {}",
            offer.credits, offer.title
        );

        // Create the Lightning invoice
        let (invoice, r_hash) = provider
            .create_invoice(offer.amount, &description)
            .await
            .map_err(PaymentError::from)?;

        // Store the payment hash as the external ID
        payment_request.external_id = Some(r_hash);

        // Generate payment details for the client
        let details = provider.generate_payment_details(&invoice);

        Ok(details)
    }

    /// Create a Coinbase payment
    async fn create_coinbase_payment(
        &self,
        payment_request: &mut PaymentRequest,
        offer: &Offer,
        chain: Option<&str>,
        asset: Option<&str>,
    ) -> Result<PaymentRequestDetails, PaymentError> {
        let provider = self
            .coinbase_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Coinbase))?;

        // Create a description for the charge
        let description = format!(
            "Purchase {} credits for API access - {}",
            offer.credits, offer.title
        );

        // Create the Coinbase charge
        let (charge_id, checkout_url, usdc_address) = provider
            .create_charge(
                offer.amount,
                &offer.currency,
                &description,
                &payment_request.id,
            )
            .await
            .map_err(PaymentError::from)?;

        // Store the charge ID as the external ID
        payment_request.external_id = Some(charge_id);

        // Generate payment details for the client
        let details = provider.generate_payment_details(
            &checkout_url,
            usdc_address.as_deref(),
            chain,
            asset.unwrap_or("USDC").into(),
        );

        Ok(details)
    }

    /// Process a Lightning webhook
    pub async fn process_lightning_webhook(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<Option<String>, PaymentError> {
        let provider = self
            .lightning_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Lightning))?;

        // Verify and parse the webhook event
        let event = match provider.verify_webhook(body, signature) {
            Ok(event) => event,
            Err(e) => {
                error!("Failed to verify webhook: {}", e);
                return Ok(None);
            }
        };
        
        // Check if payment is actually settled
        if !event.payment_status {
            // Event doesn't indicate a completed payment
            debug!("Ignoring webhook for non-paid invoice: {}", event.payment_hash);
            return Ok(None);
        }

        // Get the payment hash from the webhook data
        let payment_hash = &event.payment_hash;
        
        // Additionally verify the payment status directly (extra safety check)
        let is_paid = match provider.check_invoice(payment_hash).await {
            Ok(paid) => paid,
            Err(e) => {
                error!("Failed to verify payment status: {}", e);
                // Continue processing based on webhook data alone
                event.payment_status
            }
        };
        
        if !is_paid {
            debug!("Payment verification failed for hash: {}", payment_hash);
            return Ok(None);
        }

        // Look up the payment request by the payment hash
        let payment_request = match self
            .storage
            .get_payment_request_by_external_id(payment_hash)
            .await
        {
            Ok(request) => request,
            Err(_) => {
                debug!("Payment request not found for hash: {}", payment_hash);
                return Ok(None); // Payment not found, ignore
            }
        };

        // Check if the payment is already paid
        if payment_request.status == PaymentStatus::Paid {
            debug!("Payment already processed: {}", payment_hash);
            return Ok(None); // Already processed, ignore
        }

        // Update the payment status
        let updated_request = self
            .storage
            .update_payment_request_status(&payment_request.id, PaymentStatus::Paid)
            .await?;

        // Credit the user's account
        self.storage
            .update_user_credits(&updated_request.user_id, updated_request.credits as i32)
            .await?;

        info!(
            "Processed Lightning payment for user {}: {} credits",
            updated_request.user_id, updated_request.credits
        );

        Ok(Some(updated_request.user_id))
    }

    /// Process a Coinbase webhook
    pub async fn process_coinbase_webhook(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<Option<String>, PaymentError> {
        let provider = self
            .coinbase_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Coinbase))?;

        // Verify the webhook and parse the event
        let event = match provider.verify_webhook(body, signature) {
            Ok(event) => event,
            Err(_) => return Ok(None), // Invalid webhook, ignore
        };

        // Check if this is a completed payment
        if !provider.is_payment_completed(&event) {
            return Ok(None); // Not a completed payment, ignore
        }

        // Get the charge ID from the event
        let charge_id = provider.get_charge_id(&event);

        // Look up the payment request by the charge ID
        let payment_request = match self
            .storage
            .get_payment_request_by_external_id(charge_id)
            .await
        {
            Ok(request) => request,
            Err(_) => return Ok(None), // Payment not found, ignore
        };

        // Check if the payment is already paid
        if payment_request.status == PaymentStatus::Paid {
            return Ok(None); // Already processed, ignore
        }

        // Update the payment status
        let updated_request = self
            .storage
            .update_payment_request_status(&payment_request.id, PaymentStatus::Paid)
            .await?;

        // Credit the user's account
        self.storage
            .update_user_credits(&updated_request.user_id, updated_request.credits as i32)
            .await?;

        info!(
            "Processed Coinbase payment for user {}: {} credits",
            updated_request.user_id, updated_request.credits
        );

        Ok(Some(updated_request.user_id))
    }

    /// Find an offer by ID
    fn get_offer(&self, offer_id: &str) -> Result<&Offer, PaymentError> {
        self.config
            .offers
            .iter()
            .find(|o| o.id == offer_id)
            .ok_or_else(|| PaymentError::OfferNotFound(offer_id.to_string()))
    }
}
