use crate::models::BlockData;
use crate::storage::RedisStorage;
use anyhow::Result;
use reqwest::Client;
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur when fetching block data
#[derive(Debug, Error)]
pub enum BlockDataError {
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Server error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Service for fetching Bitcoin blockchain data
#[derive(Clone)]
pub struct BlockService {
    client: Client,
    storage: RedisStorage,
}

impl BlockService {
    /// Create a new block service instance
    pub fn new(storage: RedisStorage) -> Self {
        Self {
            client: Client::new(),
            storage,
        }
    }

    /// Fetch the latest Bitcoin block hash
    pub async fn get_latest_block(&self) -> Result<BlockData, BlockDataError> {
        // Cache key for the latest block
        let cache_key = "latest_block";

        // Try to get from cache first
        if let Ok(Some(cached_data)) = self.storage.get_cached_stock_data(cache_key).await {
            if let Ok(data) = serde_json::from_str::<BlockData>(&cached_data) {
                debug!("Returning cached block data");
                return Ok(data);
            }
        }

        // Fetch the latest block hash from Blockstream API
        info!("Fetching latest Bitcoin block hash");
        let block_hash = self.fetch_latest_block_hash().await?;

        // Create a BlockData object
        let block_data = BlockData {
            hash: block_hash,
            timestamp: chrono::Utc::now(),
        };

        // Cache the result
        if let Ok(json) = serde_json::to_string(&block_data) {
            let _ = self.storage.cache_stock_data(cache_key, &json).await;
        }

        Ok(block_data)
    }

    /// Fetch the latest Bitcoin block hash from Blockstream API
    async fn fetch_latest_block_hash(&self) -> Result<String, BlockDataError> {
        // Blockstream API for the latest block hash
        let url = "https://blockstream.info/api/blocks/tip/hash";

        // Make the request
        let response = self
            .client
            .get(url)
            .header("User-Agent", "l402-server-example-rs/0.1.0")
            .send()
            .await?;

        // Check if the request was successful
        if !response.status().is_success() {
            let status = response.status();
            error!("Blockstream API returned status code: {}", status);
            return Err(BlockDataError::ServerError(format!(
                "Blockstream API returned status code: {}",
                status
            )));
        }

        // Get the text response (block hash as plain text)
        let block_hash = response.text().await?;
        
        Ok(block_hash)
    }
}