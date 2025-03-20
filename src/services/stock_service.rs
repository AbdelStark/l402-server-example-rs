use crate::models::{FinancialData, StockAdditionalData, StockData};
use crate::storage::RedisStorage;
use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur when fetching stock data
#[derive(Debug, Error)]
pub enum StockDataError {
    /// Network error
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Invalid ticker symbol
    #[error("Invalid ticker symbol: {0}")]
    InvalidTicker(String),

    /// Server error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Service for fetching stock market data
#[derive(Clone)]
pub struct StockService {
    client: Client,
    storage: RedisStorage,
}

impl StockService {
    /// Create a new stock service instance
    pub fn new(storage: RedisStorage) -> Self {
        Self {
            client: Client::new(),
            storage,
        }
    }

    /// Fetch stock data for a given ticker symbol
    pub async fn get_stock_data(&self, symbol: &str) -> Result<StockData, StockDataError> {
        // Check for a valid ticker symbol format
        if !Self::is_valid_ticker(symbol) {
            return Err(StockDataError::InvalidTicker(symbol.to_string()));
        }

        // Try to get from cache first
        if let Ok(Some(cached_data)) = self.storage.get_cached_stock_data(symbol).await {
            if let Ok(data) = serde_json::from_str::<StockData>(&cached_data) {
                debug!("Returning cached data for {}", symbol);
                return Ok(data);
            }
        }

        // Fetch the data from Yahoo Finance
        info!("Fetching stock data for {}", symbol);
        let data = self.fetch_yahoo_data(symbol).await?;

        // Cache the result
        if let Ok(json) = serde_json::to_string(&data) {
            let _ = self.storage.cache_stock_data(symbol, &json).await;
        }

        Ok(data)
    }

    /// Fetch stock data from Yahoo Finance API
    async fn fetch_yahoo_data(&self, symbol: &str) -> Result<StockData, StockDataError> {
        // Yahoo Finance API for quote summary
        let url = format!(
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=summaryDetail,financialData,earnings",
            symbol
        );

        // Make the request
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "l402-server-example-rs/0.1.0")
            .send()
            .await?;

        // Check if the request was successful
        if !response.status().is_success() {
            let status = response.status();
            error!("Yahoo Finance API returned status code: {}", status);
            if status.as_u16() == 404 {
                return Err(StockDataError::InvalidTicker(symbol.to_string()));
            } else {
                return Err(StockDataError::ServerError(format!(
                    "Yahoo Finance API returned status code: {}",
                    status
                )));
            }
        }

        // Parse the response
        let json: Value = response.json().await?;

        // Additional URL for income statements
        let income_url = format!(
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=incomeStatementHistory",
            symbol
        );

        // Make the second request
        let income_response = self
            .client
            .get(&income_url)
            .header("User-Agent", "l402-server-example-rs/0.1.0")
            .send()
            .await?;

        // Check if the request was successful
        if !income_response.status().is_success() {
            error!("Yahoo Finance API returned error for income data");
            return Err(StockDataError::ServerError(
                "Failed to fetch income data".to_string(),
            ));
        }

        // Parse the income statement response
        let income_json: Value = income_response.json().await?;

        // Extract data from responses
        self.extract_stock_data(symbol, &json, &income_json)
    }

    /// Extract and format stock data from Yahoo Finance response
    fn extract_stock_data(
        &self,
        symbol: &str,
        json: &Value,
        income_json: &Value,
    ) -> Result<StockData, StockDataError> {
        // Path to the quote summary data
        let quote_data = json
            .get("quoteSummary")
            .and_then(|qs| qs.get("result"))
            .and_then(|result| result.get(0));

        // If data is missing, return an error
        let quote_data = match quote_data {
            Some(data) => data,
            None => {
                error!(
                    "Yahoo Finance API returned invalid data structure for {}",
                    symbol
                );
                return Err(StockDataError::ServerError(
                    "Missing data in response".to_string(),
                ));
            }
        };

        // Extract summary data
        let summary_detail = quote_data
            .get("summaryDetail")
            .ok_or_else(|| StockDataError::ServerError("Missing summaryDetail".to_string()))?;

        // Extract financial data
        let financial_data = quote_data
            .get("financialData")
            .ok_or_else(|| StockDataError::ServerError("Missing financialData".to_string()))?;

        // Extract current price
        let current_price = financial_data
            .get("currentPrice")
            .and_then(|v| v.get("raw"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Extract PE ratio
        let pe_ratio = summary_detail
            .get("trailingPE")
            .and_then(|v| v.get("raw"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Extract EPS from earnings data
        let eps = quote_data
            .get("earnings")
            .and_then(|e| e.get("earningsChart"))
            .and_then(|ec| ec.get("quarterlyEarningsChart"))
            .and_then(|qec| qec.as_array())
            .and_then(|arr| arr.last())
            .and_then(|last| last.get("actual"))
            .and_then(|actual| actual.get("raw"))
            .and_then(|raw| raw.as_f64())
            .unwrap_or(0.0);

        // Extract income statement data
        let empty_vec = Vec::new();
        let income_statements = income_json
            .get("quoteSummary")
            .and_then(|qs| qs.get("result"))
            .and_then(|result| result.get(0))
            .and_then(|data| data.get("incomeStatementHistory"))
            .and_then(|history| history.get("incomeStatementHistory"))
            .and_then(|statements| statements.as_array())
            .unwrap_or(&empty_vec);

        // Build financial data history (last 4 quarters if available)
        let mut financial_history = Vec::new();
        for statement in income_statements.iter().take(4) {
            let fiscal_date = statement
                .get("endDate")
                .and_then(|d| d.get("fmt"))
                .and_then(|f| f.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let total_revenue = statement
                .get("totalRevenue")
                .and_then(|tr| tr.get("raw"))
                .and_then(|r| r.as_f64())
                .unwrap_or(0.0);

            let net_income = statement
                .get("netIncome")
                .and_then(|ni| ni.get("raw"))
                .and_then(|r| r.as_f64())
                .unwrap_or(0.0);

            financial_history.push(FinancialData {
                fiscal_date_ending: fiscal_date,
                total_revenue,
                net_income,
            });
        }

        // Build and return the stock data
        Ok(StockData {
            additional_data: StockAdditionalData {
                current_price,
                eps,
                pe_ratio,
            },
            financial_data: financial_history,
        })
    }

    /// Check if a ticker symbol is valid
    fn is_valid_ticker(symbol: &str) -> bool {
        // Basic validation: 1-10 alphanumeric characters, possibly with a period (e.g., BRK.A)
        let len = symbol.len();
        len > 0 && len <= 10 && symbol.chars().all(|c| c.is_alphanumeric() || c == '.')
    }
}
