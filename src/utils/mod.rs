use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Mutex;
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Deserialize)]
struct KrakenTickerResponse {
    error: Vec<String>,
    result: Option<KrakenTickerResult>,
}

#[derive(Debug, Deserialize)]
struct KrakenTickerResult {
    #[serde(rename = "XXBTZUSD")]
    btc_usd: KrakenTickerData,
}

#[derive(Debug, Deserialize)]
struct KrakenTickerData {
    #[serde(rename = "c")]
    last_trade_price: Vec<String>,
}

struct PriceCache {
    timestamp: DateTime<Utc>,
    sats_per_usd: f64,
}

static PRICE_CACHE: Lazy<Mutex<PriceCache>> = Lazy::new(|| {
    Mutex::new(PriceCache {
        timestamp: Utc::now() - Duration::hours(1), // Initialize with expired timestamp
        sats_per_usd: 0.0,
    })
});

static HTTP_CLIENT: Lazy<Client> = Lazy::new(Client::new);

/// Convert USD amount to satoshis using current market rate from Kraken
pub async fn convert_usd_to_sats(amount_usd: f64) -> Result<u64, ConversionError> {
    let cache_duration = Duration::seconds(600); // 10 minutes

    // We need to check if update is needed and get the current value in separate blocks
    // to avoid holding the mutex across .await points (which would make the future !Send)
    let needs_update = {
        let cache = PRICE_CACHE.lock().unwrap();
        Utc::now() - cache.timestamp > cache_duration
    };

    // Update the cache if needed
    if needs_update {
        // Fetch current BTC price from Kraken - do this outside the mutex lock
        let response = HTTP_CLIENT
            .get("https://api.kraken.com/0/public/Ticker?pair=BTCUSD")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ConversionError::ApiError(format!(
                "Kraken API error: {}",
                response.status()
            )));
        }

        let ticker: KrakenTickerResponse = response.json().await?;

        // Check for API errors
        if !ticker.error.is_empty() {
            return Err(ConversionError::ApiError(format!(
                "Kraken API error: {}",
                ticker.error.join(", ")
            )));
        }

        // Parse BTC price
        let btc_price = ticker
            .result
            .ok_or_else(|| ConversionError::ParseError("Missing result data".to_string()))?
            .btc_usd
            .last_trade_price
            .first()
            .ok_or_else(|| ConversionError::ParseError("Missing price data".to_string()))?
            .parse::<f64>()
            .map_err(|e| ConversionError::ParseError(format!("Invalid price format: {}", e)))?;

        // Calculate sats per USD (1 BTC = 100,000,000 sats)
        let sats_per_usd = 100_000_000.0 / btc_price;

        // Update cache - only acquire the mutex after the async operations
        {
            let mut cache = PRICE_CACHE.lock().unwrap();
            cache.timestamp = Utc::now();
            cache.sats_per_usd = sats_per_usd;
        }

        debug!(
            "Updated BTC price cache. Current price: ${:.2}, sats per USD: {:.2}",
            btc_price, sats_per_usd
        );
    }

    // Get the current cached rate in a separate lock
    let sats_per_usd = {
        let cache = PRICE_CACHE.lock().unwrap();
        cache.sats_per_usd
    };

    // Convert USD to sats using cached rate (rounding up to nearest sat)
    let amount_sats = (amount_usd * sats_per_usd).ceil() as u64;
    debug!("Converted ${} USD to {} sats", amount_usd, amount_sats);

    Ok(amount_sats)
}
