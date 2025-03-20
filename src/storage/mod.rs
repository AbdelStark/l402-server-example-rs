use crate::models::{PaymentRequest, PaymentStatus, User};
use anyhow::Result;
use chrono::Utc;
use deadpool_redis::{Config as RedisConfig, Pool, Runtime};
use redis::{AsyncCommands, RedisError};
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur when interacting with storage
#[derive(Debug, Error)]
pub enum StorageError {
    /// Error connecting to Redis
    #[error("Redis connection error: {0}")]
    ConnectionError(#[from] RedisError),

    /// Pool error
    #[error("Redis pool error: {0}")]
    PoolError(String),

    /// User not found
    #[error("User not found")]
    UserNotFound,

    /// Payment request not found
    #[error("Payment request not found")]
    PaymentRequestNotFound,

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl From<deadpool_redis::PoolError> for StorageError {
    fn from(err: deadpool_redis::PoolError) -> Self {
        StorageError::PoolError(err.to_string())
    }
}

/// Storage implementation using Redis
#[derive(Clone)]
pub struct RedisStorage {
    pool: Pool,
}

impl std::fmt::Debug for RedisStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStorage")
            .field("pool", &"RedisPool".to_string())
            .finish()
    }
}

/// Prefixes for Redis keys
const USER_KEY_PREFIX: &str = "user:";
const PAYMENT_REQ_KEY_PREFIX: &str = "payment:";
const EXTERNAL_ID_KEY_PREFIX: &str = "external_payment:";
const STOCK_CACHE_KEY_PREFIX: &str = "stock:";

impl RedisStorage {
    /// Create a new Redis storage instance
    pub fn new(redis_url: &str) -> Result<Self> {
        let cfg = RedisConfig::from_url(redis_url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1))?;

        Ok(Self { pool })
    }

    /// Check if the Redis connection is working
    pub async fn check_connection(&self) -> Result<(), StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(StorageError::from)?;
        Ok(())
    }

    /// Create a new user with initial credits
    pub async fn create_user(&self, user: &User) -> Result<(), StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", USER_KEY_PREFIX, user.id);
        let user_json = serde_json::to_string(user).map_err(StorageError::from)?;

        let _: () = conn.set(key, user_json).await.map_err(StorageError::from)?;
        info!("Created new user with ID: {}", user.id);
        Ok(())
    }

    /// Get a user by ID
    pub async fn get_user(&self, user_id: &str) -> Result<User, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", USER_KEY_PREFIX, user_id);

        let user_json: String = conn.get(key).await.map_err(|e| {
            debug!("Error fetching user {}: {:?}", user_id, e);
            StorageError::UserNotFound
        })?;

        let user: User = serde_json::from_str(&user_json).map_err(StorageError::from)?;
        Ok(user)
    }

    /// Update a user's credits
    pub async fn update_user_credits(
        &self,
        user_id: &str,
        delta: i32,
    ) -> Result<User, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", USER_KEY_PREFIX, user_id);

        // Get the current user
        let user_json: String = conn
            .get(&key)
            .await
            .map_err(|_| StorageError::UserNotFound)?;
        let mut user: User = serde_json::from_str(&user_json).map_err(StorageError::from)?;

        // Update credits (ensuring it doesn't go below 0)
        if delta < 0 && user.credits < delta.unsigned_abs() {
            user.credits = 0;
        } else if delta < 0 {
            user.credits -= delta.unsigned_abs();
        } else {
            user.credits += delta as u32;
        }

        // Update the last credit update timestamp
        user.last_credit_update_at = Utc::now();

        // Save the updated user
        let updated_user_json = serde_json::to_string(&user).map_err(StorageError::from)?;
        let _: () = conn
            .set(key, updated_user_json)
            .await
            .map_err(StorageError::from)?;

        info!(
            "Updated credits for user {}: delta={}, new balance={}",
            user_id, delta, user.credits
        );
        Ok(user)
    }

    /// Store a new payment request
    pub async fn store_payment_request(
        &self,
        request: &PaymentRequest,
    ) -> Result<(), StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", PAYMENT_REQ_KEY_PREFIX, request.id);
        let req_json = serde_json::to_string(request).map_err(StorageError::from)?;

        // Calculate TTL (expiry time minus current time)
        let now = Utc::now();
        let expiry = request.expires_at;
        let ttl = if expiry > now {
            (expiry - now).num_seconds() as u64
        } else {
            // If already expired, set a short TTL
            60
        };

        // Set with expiry
        let _: () = conn
            .set_ex(key, req_json, ttl)
            .await
            .map_err(StorageError::from)?;

        // If there's an external ID, create a reference to the main payment request
        if let Some(ext_id) = &request.external_id {
            let ext_key = format!("{}{}", EXTERNAL_ID_KEY_PREFIX, ext_id);
            let _: () = conn
                .set_ex(ext_key, &request.id, ttl)
                .await
                .map_err(StorageError::from)?;
        }

        info!(
            "Stored payment request: id={}, method={:?}, offer={}",
            request.id, request.method, request.offer_id
        );
        Ok(())
    }

    /// Get a payment request by ID
    pub async fn get_payment_request(
        &self,
        request_id: &str,
    ) -> Result<PaymentRequest, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", PAYMENT_REQ_KEY_PREFIX, request_id);

        let req_json: String = conn
            .get(key)
            .await
            .map_err(|_| StorageError::PaymentRequestNotFound)?;
        let request: PaymentRequest =
            serde_json::from_str(&req_json).map_err(StorageError::from)?;

        Ok(request)
    }

    /// Get a payment request by external ID (e.g., Lightning invoice ID or Coinbase charge ID)
    pub async fn get_payment_request_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<PaymentRequest, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let ext_key = format!("{}{}", EXTERNAL_ID_KEY_PREFIX, external_id);

        // Get the payment request ID from the external ID reference
        let request_id: String = conn
            .get(ext_key)
            .await
            .map_err(|_| StorageError::PaymentRequestNotFound)?;

        // Then get the actual payment request
        self.get_payment_request(&request_id).await
    }

    /// Update a payment request status
    pub async fn update_payment_request_status(
        &self,
        request_id: &str,
        status: PaymentStatus,
    ) -> Result<PaymentRequest, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", PAYMENT_REQ_KEY_PREFIX, request_id);

        // Get the current request
        let req_json: String = conn
            .get(&key)
            .await
            .map_err(|_| StorageError::PaymentRequestNotFound)?;
        let mut request: PaymentRequest =
            serde_json::from_str(&req_json).map_err(StorageError::from)?;

        // Update status
        request.status = status;

        // Save the updated request
        let updated_req_json = serde_json::to_string(&request).map_err(StorageError::from)?;
        let _: () = conn
            .set(key, updated_req_json)
            .await
            .map_err(StorageError::from)?;

        info!(
            "Updated payment request {}: status={:?}",
            request_id, status
        );
        Ok(request)
    }

    /// Cache stock data with a 60-second TTL
    pub async fn cache_stock_data(&self, symbol: &str, data: &str) -> Result<(), StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", STOCK_CACHE_KEY_PREFIX, symbol.to_uppercase());

        // Cache for 60 seconds
        let _: () = conn
            .set_ex(key, data, 60)
            .await
            .map_err(StorageError::from)?;
        debug!("Cached stock data for {}", symbol);
        Ok(())
    }

    /// Get cached stock data if available
    pub async fn get_cached_stock_data(
        &self,
        symbol: &str,
    ) -> Result<Option<String>, StorageError> {
        let mut conn = self.pool.get().await.map_err(StorageError::from)?;
        let key = format!("{}{}", STOCK_CACHE_KEY_PREFIX, symbol.to_uppercase());

        let result: Result<String, RedisError> = conn.get(key).await;
        match result {
            Ok(data) => {
                debug!("Cache hit for stock data: {}", symbol);
                Ok(Some(data))
            }
            Err(_) => {
                debug!("Cache miss for stock data: {}", symbol);
                Ok(None)
            }
        }
    }
}

/// Create a Redis connection pool
pub fn create_pool(redis_url: &str) -> Result<Pool> {
    let cfg = RedisConfig::from_url(redis_url);
    let pool = cfg.create_pool(Some(Runtime::Tokio1))?;
    Ok(pool)
}
