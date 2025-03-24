pub mod coinbase;
pub mod lightning;
pub mod lnbits;

use crate::config::{Config, Offer};
use crate::storage::{RedisStorage, StorageError};
use crate::utils;
use crate::{
    models::{
        PaymentMethod, PaymentRequest, PaymentRequestDetails, PaymentRequestInput, PaymentStatus,
    },
    utils::ConversionError,
};
use anyhow::Result;
use chrono::{Duration, Utc};
use coinbase::CoinbaseProvider;
use lightning::LightningProvider;
use std::sync::Arc;
use thiserror::Error;
use tokio::time;
use tracing::{debug, error, info, warn};

// No exports needed

// Re-export LNBits types

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
    #[allow(dead_code)]
    OfferNotFound(String),

    /// User not found
    #[error("User not found: {0}")]
    #[allow(dead_code)]
    UserNotFound(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    #[allow(dead_code)]
    InvalidInput(String),

    /// Payment already processed
    #[error("Payment already processed: {0}")]
    #[allow(dead_code)]
    AlreadyProcessed(String),

    /// Payment expired
    #[error("Payment expired: {0}")]
    #[allow(dead_code)]
    PaymentExpired(String),

    /// Payment not found
    #[error("Payment not found: {0}")]
    #[allow(dead_code)]
    PaymentNotFound(String),

    /// Conversion error
    #[error("Conversion error: {0}")]
    ConversionError(#[from] ConversionError),

    /// Invalid offer
    #[error("Invalid offer: {0}")]
    InvalidOffer(String),
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
    /// Create a new payment service without providers
    pub fn new_without_providers(config: Arc<Config>, storage: RedisStorage) -> Self {
        Self {
            storage,
            config,
            lightning_provider: None,
            coinbase_provider: None,
        }
    }

    /// Initialize payment providers
    pub fn init_providers(&mut self) -> Result<(), PaymentError> {
        // Initialize Lightning provider if configured
        if self.config.lightning_enabled {
            match LightningProvider::new(Arc::clone(&self.config)) {
                Ok(provider) => {
                    info!("Lightning payment provider initialized");
                    self.lightning_provider = Some(provider);
                }
                Err(err) => {
                    error!("Failed to initialize Lightning provider: {}", err);
                }
            }
        }

        // Initialize Coinbase provider if configured
        if self.config.coinbase_enabled {
            match CoinbaseProvider::new(Arc::clone(&self.config)) {
                Ok(provider) => {
                    info!("Coinbase payment provider initialized");
                    self.coinbase_provider = Some(provider);
                }
                Err(err) => {
                    error!("Failed to initialize Coinbase provider: {}", err);
                }
            }
        }

        Ok(())
    }

    /// Process a payment request
    pub async fn process_payment_request(
        &self,
        input: PaymentRequestInput,
    ) -> Result<(PaymentRequest, PaymentRequestDetails), PaymentError> {
        // Get the offer
        let offer = self
            .config
            .offers
            .iter()
            .find(|o| o.id == input.offer_id)
            .ok_or_else(|| PaymentError::InvalidOffer(input.offer_id.clone()))?;

        // Create payment request
        let payment_request = PaymentRequest::new(
            input.payment_context_token.clone(),
            input.offer_id.clone(),
            offer.credits,
            input.payment_method,
            Utc::now() + Duration::minutes(30),
        );

        // Store the payment request
        self.storage
            .store_payment_request(&payment_request)
            .await
            .map_err(PaymentError::from)?;

        // Process payment based on method
        let payment_details = match input.payment_method {
            PaymentMethod::Lightning => {
                self.create_lightning_payment(&payment_request, offer)
                    .await?
            }
            PaymentMethod::Coinbase => {
                self.create_coinbase_payment(&payment_request, offer, input.chain, input.asset)
                    .await?
            }
        };

        Ok((payment_request, payment_details))
    }

    /// Create a Lightning payment
    async fn create_lightning_payment(
        &self,
        payment_request: &PaymentRequest,
        offer: &Offer,
    ) -> Result<PaymentRequestDetails, PaymentError> {
        let provider = self
            .lightning_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Lightning))?;

        // Convert USD to sats
        let amount_sats = utils::convert_usd_to_sats(offer.amount)
            .await
            .map_err(PaymentError::from)?;

        // Create invoice
        let (invoice, payment_hash) = provider
            .create_invoice(amount_sats, &format!("Purchase {} credits", offer.credits))
            .await?;

        // Update payment request with external ID
        let mut updated_request = payment_request.clone();
        updated_request.external_id = Some(payment_hash.clone());
        self.storage
            .store_payment_request(&updated_request)
            .await
            .map_err(PaymentError::from)?;

        // Start polling for payment in background
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.start_payment_polling(payment_hash, None).await {
                error!("Error polling payment status: {}", e);
            }
        });

        Ok(provider.generate_payment_details(&invoice))
    }

    /// Create a Coinbase payment
    async fn create_coinbase_payment(
        &self,
        payment_request: &PaymentRequest,
        offer: &Offer,
        chain: Option<String>,
        asset: Option<String>,
    ) -> Result<PaymentRequestDetails, PaymentError> {
        let provider = self
            .coinbase_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Coinbase))?;

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
            .await?;

        // Store the charge ID as the external ID
        let mut updated_request = payment_request.clone();
        updated_request.external_id = Some(charge_id);

        // Save the updated payment request
        self.storage
            .store_payment_request(&updated_request)
            .await
            .map_err(PaymentError::from)?;

        // Generate payment details for the client
        Ok(provider.generate_payment_details(
            &checkout_url,
            usdc_address.as_deref(),
            chain.as_deref(),
            asset.as_deref(),
        ))
    }

    /// Process a successful payment and update user credits
    async fn process_successful_payment(
        &self,
        payment_request: &mut PaymentRequest,
    ) -> Result<(), PaymentError> {
        // Only process if payment is still pending
        if payment_request.status != PaymentStatus::Pending {
            debug!(
                "Payment {} already processed (status: {:?})",
                payment_request.id, payment_request.status
            );
            return Ok(());
        }

        // Update payment status
        payment_request.status = PaymentStatus::Paid;

        // Store the updated payment request
        self.storage
            .store_payment_request(payment_request)
            .await
            .map_err(PaymentError::from)?;

        // Update user credits
        self.storage
            .update_user_credits(&payment_request.user_id, payment_request.credits as i32)
            .await
            .map_err(PaymentError::from)?;

        info!(
            "Payment {} processed successfully for user {}",
            payment_request.id, payment_request.user_id
        );

        Ok(())
    }

    /// Start polling for payment status
    pub async fn start_payment_polling(
        &self,
        payment_hash: String,
        timeout_minutes: Option<u64>,
    ) -> Result<(), PaymentError> {
        let provider = self
            .lightning_provider
            .as_ref()
            .ok_or_else(|| PaymentError::InvalidPaymentMethod(PaymentMethod::Lightning))?;

        let timeout = timeout_minutes.unwrap_or(30);
        let timeout_duration = Duration::minutes(timeout as i64);
        let start_time = Utc::now();
        let poll_interval = time::Duration::from_millis(500); // Poll every 500ms

        info!("Starting payment polling for hash: {}", payment_hash);

        loop {
            // Check if we've exceeded the timeout
            if Utc::now() - start_time > timeout_duration {
                warn!("Payment polling timed out for hash: {}", payment_hash);
                return Ok(());
            }

            // Check payment status
            match provider.check_invoice(&payment_hash).await {
                Ok(true) => {
                    // Get the payment request
                    match self
                        .storage
                        .get_payment_request_by_external_id(&payment_hash)
                        .await
                    {
                        Ok(mut payment_request) => {
                            // Process the successful payment
                            match self.process_successful_payment(&mut payment_request).await {
                                Ok(_) => {
                                    info!("Payment confirmed and processed: {}", payment_hash);
                                    return Ok(());
                                }
                                Err(PaymentError::AlreadyProcessed(_)) => {
                                    debug!(
                                        "Payment already processed, stopping polling: {}",
                                        payment_hash
                                    );
                                    return Ok(());
                                }
                                Err(PaymentError::PaymentExpired(_)) => {
                                    debug!("Payment expired, stopping polling: {}", payment_hash);
                                    return Ok(());
                                }
                                Err(e) => {
                                    error!("Error processing payment: {}", e);
                                    // Continue polling in case it's a temporary error
                                }
                            }
                        }
                        Err(StorageError::PaymentRequestNotFound) => {
                            debug!("Payment request not found for hash: {}", payment_hash);
                            return Ok(());
                        }
                        Err(e) => {
                            error!("Error retrieving payment request: {}", e);
                            // Continue polling in case it's a temporary error
                        }
                    }
                }
                Ok(false) => {
                    debug!("Payment not yet received: {}", payment_hash);
                }
                Err(e) => {
                    error!("Error checking payment status: {}", e);
                    // Continue polling in case it's a temporary error
                }
            }

            // Wait before next poll
            time::sleep(poll_interval).await;
        }
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

        // Verify and parse the webhook
        let event = provider
            .verify_webhook(body, signature)
            .map_err(PaymentError::from)?;

        // Get the payment request
        let mut payment_request = match self
            .storage
            .get_payment_request_by_external_id(&event.payment_hash)
            .await
        {
            Ok(request) => request,
            Err(StorageError::PaymentRequestNotFound) => {
                debug!("Payment request not found for hash: {}", event.payment_hash);
                return Ok(None);
            }
            Err(e) => return Err(PaymentError::from(e)),
        };

        // Check if already processed
        if payment_request.status == PaymentStatus::Paid {
            debug!("Payment already processed: {}", event.payment_hash);
            return Ok(None);
        }

        // Check if expired
        if Utc::now() > payment_request.expires_at {
            debug!("Payment expired: {}", event.payment_hash);
            return Ok(None);
        }

        // Verify payment is settled
        if !provider
            .check_invoice(&event.payment_hash)
            .await
            .map_err(PaymentError::from)?
        {
            debug!("Payment not yet settled: {}", event.payment_hash);
            return Ok(None);
        }

        // Process the successful payment
        self.process_successful_payment(&mut payment_request)
            .await?;

        Ok(Some(payment_request.user_id.clone()))
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

        // Verify and parse the webhook
        let event = provider
            .verify_webhook(body, signature)
            .map_err(PaymentError::from)?;

        // Get the charge ID from the event
        let charge_id = provider.get_charge_id(&event);

        // Get the payment request
        let mut payment_request = match self
            .storage
            .get_payment_request_by_external_id(charge_id)
            .await
        {
            Ok(request) => request,
            Err(StorageError::PaymentRequestNotFound) => {
                debug!("Payment request not found for charge: {}", charge_id);
                return Ok(None);
            }
            Err(e) => return Err(PaymentError::from(e)),
        };

        // Check if already processed
        if payment_request.status == PaymentStatus::Paid {
            debug!("Payment already processed: {}", charge_id);
            return Ok(None);
        }

        // Check if expired
        if Utc::now() > payment_request.expires_at {
            debug!("Payment expired: {}", charge_id);
            return Ok(None);
        }

        // Check if payment is completed
        if !provider.is_payment_completed(&event) {
            debug!("Payment not completed: {}", charge_id);
            return Ok(None);
        }

        // Process the successful payment
        self.process_successful_payment(&mut payment_request)
            .await?;

        Ok(Some(payment_request.user_id.clone()))
    }
}
