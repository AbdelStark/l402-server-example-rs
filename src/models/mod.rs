use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a user of the service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier for the user (used as API token)
    pub id: String,
    /// Number of credits available to the user
    pub credits: u32,
    /// When the user was created
    pub created_at: DateTime<Utc>,
    /// When the user's credits were last updated
    pub last_credit_update_at: DateTime<Utc>,
}

impl User {
    /// Create a new user with the specified number of credits
    pub fn new(initial_credits: u32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            credits: initial_credits,
            created_at: now,
            last_credit_update_at: now,
        }
    }
}

/// Payment methods supported by the service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentMethod {
    /// Lightning Network payment
    Lightning,
    /// Coinbase Commerce payment
    Coinbase,
}

/// Status of a payment request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    /// Payment is pending (waiting for confirmation)
    Pending,
    /// Payment has been confirmed
    Paid,
    /// Payment has expired without being paid
    Expired,
}

/// Represents a payment request to purchase credits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequest {
    /// Unique identifier for the payment request
    pub id: String,
    /// The user who is making the payment
    pub user_id: String,
    /// The offer being purchased
    pub offer_id: String,
    /// How many credits will be added when paid
    pub credits: u32,
    /// Current status of the payment
    pub status: PaymentStatus,
    /// Which payment method is being used
    pub method: PaymentMethod,
    /// When the payment request expires
    pub expires_at: DateTime<Utc>,
    /// External payment reference (e.g., invoice ID, charge ID)
    pub external_id: Option<String>,
}

impl PaymentRequest {
    /// Create a new payment request
    pub fn new(
        user_id: String,
        offer_id: String,
        credits: u32,
        method: PaymentMethod,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            offer_id,
            credits,
            status: PaymentStatus::Pending,
            method,
            expires_at,
            external_id: None,
        }
    }
}

/// Request to initiate a payment
#[derive(Debug, Deserialize)]
pub struct PaymentRequestInput {
    /// ID of the offer to purchase
    pub offer_id: String,
    /// Which payment method to use
    pub payment_method: PaymentMethod,
    /// Token to identify the user
    pub payment_context_token: String,
    /// Optional blockchain for crypto payments
    pub chain: Option<String>,
    /// Optional asset for crypto payments
    pub asset: Option<String>,
}

/// Details for a Lightning payment
#[derive(Debug, Serialize)]
pub struct LightningPaymentDetails {
    /// BOLT11 invoice string
    pub lightning_invoice: String,
}

/// Details for a Coinbase payment
#[derive(Debug, Serialize)]
pub struct CoinbasePaymentDetails {
    /// URL to the hosted checkout page
    pub checkout_url: String,
    /// Crypto address for direct payment (if available)
    pub address: Option<String>,
    /// Which asset to pay with (if specified)
    pub asset: Option<String>,
    /// Which blockchain to use (if specified)
    pub chain: Option<String>,
}

/// Response for a payment request
#[derive(Debug, Serialize)]
pub struct PaymentRequestResponse {
    /// Details specific to the payment method
    #[serde(flatten)]
    pub payment_request: PaymentRequestDetails,
    /// ID of the offer being purchased
    pub offer_id: String,
    /// When the payment request expires
    pub expires_at: DateTime<Utc>,
}

/// Union type for different payment method details
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PaymentRequestDetails {
    /// Lightning payment details
    Lightning {
        /// BOLT11 invoice string
        lightning_invoice: String,
    },
    /// Coinbase payment details
    Coinbase {
        /// URL to the hosted checkout page
        checkout_url: String,
        /// Crypto address for direct payment (if available)
        address: Option<String>,
        /// Which asset to pay with (if specified)
        asset: Option<String>,
        /// Which blockchain to use (if specified)
        chain: Option<String>,
    },
}

/// Response for a 402 Payment Required status
#[derive(Debug, Serialize)]
pub struct PaymentRequiredResponse {
    /// When the offer expires
    pub expiry: DateTime<Utc>,
    /// Available credit purchase options
    pub offers: Vec<crate::config::Offer>,
    /// Token to identify the user in the payment flow
    pub payment_context_token: String,
    /// URL to initiate payment
    pub payment_request_url: String,
}

/// Bitcoin block data
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockData {
    /// Block hash
    pub hash: String,
    /// Timestamp when the data was fetched
    pub timestamp: DateTime<Utc>,
}
