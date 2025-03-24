use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use tracing::debug;

/// Represents a credit purchase offer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    /// Unique identifier for the offer
    pub id: String,
    /// Short title for the offer
    pub title: String,
    /// Longer description of the offer
    pub description: String,
    /// Number of credits the user will receive
    pub credits: u32,
    /// Cost of the offer in the specified currency
    pub amount: f64,
    /// Currency for the offer (e.g., "USD")
    pub currency: String,
}

/// Global application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Server host address
    pub host: String,
    /// Server port
    pub port: u16,
    /// Redis connection URL
    pub redis_url: String,
    /// URL for payment requests
    pub payment_request_url: Option<String>,
    /// Whether Lightning payments are enabled
    pub lightning_enabled: bool,
    /// LNBits URL (if using LNBits)
    pub lnbits_url: Option<String>,
    /// LNBits admin key (if using LNBits)
    pub lnbits_admin_key: Option<String>,
    /// LNBits invoice read key (if using LNBits)
    pub lnbits_invoice_read_key: Option<String>,
    /// LNBits webhook url
    #[allow(dead_code)]
    pub lnbits_webhook_url: Option<String>,
    /// Whether Coinbase payments are enabled
    pub coinbase_enabled: bool,
    /// Coinbase Commerce API key (if applicable)
    pub coinbase_api_key: Option<String>,
    /// Coinbase webhook secret for verification (if applicable)
    pub coinbase_webhook_secret: Option<String>,
    /// Available credit purchase offers
    pub offers: Vec<Offer>,
}

impl Config {
    /// Load configuration from environment variables (or .env file)
    pub fn from_env() -> Self {
        // Load .env file if it exists
        dotenv().ok();

        // Parse offers from JSON
        let offers_json = env::var("OFFERS_JSON").unwrap_or_else(|_| {
            // Default offers if not specified
            r#"[
                {
                    "id": "offer1",
                    "title": "1 Credit Package",
                    "description": "Purchase 1 credit for API access",
                    "credits": 1,
                    "amount": 0.01,
                    "currency": "USD"
                },
                {
                    "id": "offer2",
                    "title": "5 Credits Package",
                    "description": "Purchase 5 credits for API access",
                    "credits": 5,
                    "amount": 0.05,
                    "currency": "USD"
                }
            ]"#
            .to_string()
        });

        let offers: Vec<Offer> = serde_json::from_str(&offers_json)
            .expect("Failed to parse OFFERS_JSON environment variable");

        // Get configuration from environment
        let host = env::var("HOST").unwrap_or_else(|_| {
            debug!("HOST not found in environment, using default");
            "127.0.0.1".to_string()
        });

        let port = match env::var("PORT") {
            Ok(val) => {
                debug!("Found PORT in environment: {}", val);
                val.parse().expect("PORT must be a number")
            }
            Err(_) => {
                debug!("PORT not found in environment, using default: 8080");
                8080
            }
        };

        let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| {
            debug!("REDIS_URL not found in environment, using default");
            "redis://localhost:6379".to_string()
        });
        debug!("REDIS_URL: {}", redis_url);

        let payment_request_url = env::var("PAYMENT_REQUEST_URL").ok();
        if let Some(url) = &payment_request_url {
            debug!("Found PAYMENT_REQUEST_URL: {}", url);
        } else {
            debug!(
                "PAYMENT_REQUEST_URL not found in environment, will use default based on HOST:PORT"
            );
        }

        let lightning_enabled = env::var("LIGHTNING_ENABLED")
            .map(|val| {
                debug!("Found LIGHTNING_ENABLED in environment: {}", val);
                val.parse().unwrap_or(true)
            })
            .unwrap_or_else(|_| {
                debug!("LIGHTNING_ENABLED not found in environment, using default: true");
                true
            });

        // LNBits configuration
        let lnbits_url = env::var("LNBITS_URL").ok();
        if let Some(url) = &lnbits_url {
            debug!("Found LNBITS_URL: {}", url);
        }
        let lnbits_admin_key = env::var("LNBITS_ADMIN_KEY").ok();
        if lnbits_admin_key.is_some() {
            debug!("Found LNBITS_ADMIN_KEY");
        }
        let lnbits_invoice_read_key = env::var("LNBITS_INVOICE_READ_KEY").ok();
        if lnbits_invoice_read_key.is_some() {
            debug!("Found LNBITS_INVOICE_READ_KEY");
        }
        let lnbits_webhook_url = env::var("LNBITS_WEBHOOK_URL").ok();
        if lnbits_webhook_url.is_some() {
            debug!("Found LNBITS_WEBHOOK_URL");
        }

        let coinbase_enabled = env::var("COINBASE_ENABLED")
            .map(|val| {
                debug!("Found COINBASE_ENABLED in environment: {}", val);
                val.parse().unwrap_or(true)
            })
            .unwrap_or_else(|_| {
                debug!("COINBASE_ENABLED not found in environment, using default: true");
                true
            });

        let coinbase_api_key = env::var("COINBASE_API_KEY").ok();
        if coinbase_api_key.is_some() {
            debug!("Found COINBASE_API_KEY");
        }
        let coinbase_webhook_secret = env::var("COINBASE_WEBHOOK_SECRET").ok();
        if coinbase_webhook_secret.is_some() {
            debug!("Found COINBASE_WEBHOOK_SECRET");
        }

        Self {
            host,
            port,
            redis_url,
            payment_request_url,
            lightning_enabled,
            lnbits_url,
            lnbits_admin_key,
            lnbits_invoice_read_key,
            lnbits_webhook_url,
            coinbase_enabled,
            coinbase_api_key,
            coinbase_webhook_secret,
            offers,
        }
    }

    /// Create a shared reference to this configuration
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Get the payment request URL, or construct a default based on host and port
    pub fn get_payment_request_url(&self) -> String {
        self.payment_request_url
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/l402/payment-request", self.host, self.port))
    }
}
