use chrono::{DateTime, Utc};
use reqwest::{
    Client as HttpClient,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json;
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Error)]
pub enum LNBitsError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone)]
pub struct LNBitsClient {
    http_client: HttpClient,
    base_url: String,
    admin_key: String,
    invoice_read_key: String,
}

#[derive(Debug, Serialize)]
pub struct CreateInvoiceRequest {
    pub amount: u64,
    pub memo: Option<String>,
    pub unit: String,
    pub expiry: Option<u32>,
    pub webhook: Option<String>,
    pub internal: bool,
    pub out: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceResponse {
    pub checking_id: String,
    pub payment_hash: String,
    pub wallet_id: String,
    pub amount: u64,
    pub fee: u64,
    pub bolt11: String,
    pub status: String,
    pub memo: Option<String>,
    pub expiry: Option<String>,
    pub webhook: Option<String>,
    pub webhook_status: Option<u32>,
    pub preimage: Option<String>,
    pub tag: Option<String>,
    pub extension: Option<String>,
    pub time: String,
    pub created_at: String,
    pub updated_at: String,
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PaymentStatus {
    pub paid: bool,
    pub status: String,
    pub preimage: Option<String>,
    pub details: PaymentDetails,
}

#[derive(Debug, Deserialize)]
pub struct PaymentDetails {
    pub checking_id: String,
    pub payment_hash: String,
    pub wallet_id: String,
    pub amount: u64,
    pub fee: u64,
    pub bolt11: String,
    pub status: String,
    pub memo: Option<String>,
    pub expiry: Option<String>,
    pub webhook: Option<String>,
    pub webhook_status: Option<u32>,
    pub preimage: Option<String>,
    pub tag: Option<String>,
    pub extension: Option<String>,
    pub time: String,
    pub created_at: String,
    pub updated_at: String,
    pub extra: serde_json::Value,
}

impl LNBitsClient {
    pub fn new(
        _wallet_id: &str,
        admin_key: &str,
        invoice_read_key: &str,
        base_url: &str,
        _cert_path: Option<String>,
    ) -> Result<Self, LNBitsError> {
        let http_client = HttpClient::builder()
            .build()
            .map_err(|e| LNBitsError::NetworkError(e))?;

        Ok(Self {
            http_client,
            base_url: base_url.trim_end_matches('/').to_string(),
            admin_key: admin_key.to_string(),
            invoice_read_key: invoice_read_key.to_string(),
        })
    }

    pub async fn create_invoice(
        &self,
        request: &CreateInvoiceRequest,
    ) -> Result<CreateInvoiceResponse, LNBitsError> {
        let url = format!("{}/api/v1/payments", self.base_url);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Api-Key",
            HeaderValue::from_str(&self.admin_key)
                .map_err(|_| LNBitsError::InvalidResponse("Invalid admin key".to_string()))?,
        );
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));

        debug!("Creating invoice with request: {:?}", request);

        let response = self
            .http_client
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Failed to create invoice: {}", error_text);
            return Err(LNBitsError::ApiError(error_text));
        }

        // Get the raw response text for debugging
        let response_text = response.text().await?;
        debug!("Raw response: {}", response_text);

        let invoice =
            serde_json::from_str::<CreateInvoiceResponse>(&response_text).map_err(|e| {
                LNBitsError::InvalidResponse(format!("Failed to parse response: {}", e))
            })?;

        debug!(
            "Created invoice with payment hash: {}",
            invoice.payment_hash
        );
        Ok(invoice)
    }

    pub async fn is_invoice_paid(&self, payment_hash: &str) -> Result<bool, LNBitsError> {
        let url = format!("{}/api/v1/payments/{}", self.base_url, payment_hash);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Api-Key",
            HeaderValue::from_str(&self.invoice_read_key).map_err(|_| {
                LNBitsError::InvalidResponse("Invalid invoice read key".to_string())
            })?,
        );

        debug!("Checking payment status for hash: {}", payment_hash);

        let response = self.http_client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Failed to check payment status: {}", error_text);
            return Err(LNBitsError::ApiError(error_text));
        }

        // Get the raw response text for debugging
        let response_text = response.text().await?;
        debug!("Raw payment status response: {}", response_text);

        let payment = serde_json::from_str::<PaymentStatus>(&response_text).map_err(|e| {
            LNBitsError::InvalidResponse(format!("Failed to parse response: {}", e))
        })?;

        debug!(
            "Payment status: paid={}, status={}",
            payment.paid, payment.status
        );
        Ok(payment.paid)
    }
}
