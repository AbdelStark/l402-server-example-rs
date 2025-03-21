use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;

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
    /// Whether Lightning payments are enabled
    pub lightning_enabled: bool,
    /// LNBits URL (if using LNBits)
    pub lnbits_url: Option<String>,
    /// LNBits admin key (if using LNBits)
    pub lnbits_admin_key: Option<String>,
    /// LNBits invoice read key (if using LNBits)
    pub lnbits_invoice_read_key: Option<String>,
    /// LNBits webhook verification key (if using LNBits)
    pub lnbits_webhook_key: Option<String>,
    /// Legacy: Lightning node REST endpoint (if applicable)
    pub lnd_rest_endpoint: Option<String>,
    /// Legacy: LND macaroon in hex format (if applicable)
    pub lnd_macaroon_hex: Option<String>,
    /// Legacy: Path to LND TLS certificate (if applicable)
    pub lnd_cert_path: Option<String>,
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
        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .expect("PORT must be a number");
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let lightning_enabled = env::var("LIGHTNING_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        // LNBits configuration
        let lnbits_url = env::var("LNBITS_URL").ok();
        let lnbits_admin_key = env::var("LNBITS_ADMIN_KEY").ok();
        let lnbits_invoice_read_key = env::var("LNBITS_INVOICE_READ_KEY").ok();
        let lnbits_webhook_key = env::var("LNBITS_WEBHOOK_KEY").ok();

        // Legacy LND configuration - kept for backward compatibility
        let lnd_rest_endpoint = env::var("LND_REST_ENDPOINT").ok();
        let lnd_macaroon_hex = env::var("LND_MACAROON_HEX").ok();
        let lnd_cert_path = env::var("LND_CERT_PATH").ok();

        let coinbase_enabled = env::var("COINBASE_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let coinbase_api_key = env::var("COINBASE_API_KEY").ok();
        let coinbase_webhook_secret = env::var("COINBASE_WEBHOOK_SECRET").ok();

        Self {
            host,
            port,
            redis_url,
            lightning_enabled,
            lnbits_url,
            lnbits_admin_key,
            lnbits_invoice_read_key,
            lnbits_webhook_key,
            lnd_rest_endpoint,
            lnd_macaroon_hex,
            lnd_cert_path,
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
}
