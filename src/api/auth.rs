use crate::storage::StorageError;
use axum::{
    extract::{FromRequestParts, Request, State},
    http::{StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::{debug, error};

/// Error when authentication fails
#[derive(Debug)]
pub enum AuthError {
    /// Missing authorization header
    MissingToken,
    /// Invalid token format
    InvalidTokenFormat,
    /// User not found
    UserNotFound,
    /// Storage error
    StorageError(StorageError),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            AuthError::MissingToken | AuthError::InvalidTokenFormat => StatusCode::UNAUTHORIZED,
            AuthError::UserNotFound => StatusCode::UNAUTHORIZED,
            AuthError::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let message = match self {
            AuthError::MissingToken => "Missing authorization token",
            AuthError::InvalidTokenFormat => "Invalid authorization format",
            AuthError::UserNotFound => "Invalid token",
            AuthError::StorageError(e) => {
                error!("Storage error during authentication: {}", e);
                "Internal server error"
            }
        };

        let body = serde_json::json!({
            "error": message
        });

        (status, body.to_string()).into_response()
    }
}

/// Extract a user ID from Bearer token
#[derive(Debug, Clone)]
pub struct UserId(pub String);

/// Extract the user ID from the Authorization header
impl<S> FromRequestParts<S> for UserId
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or(AuthError::MissingToken)?;

        let auth_value = auth_header
            .to_str()
            .map_err(|_| AuthError::InvalidTokenFormat)?;

        if !auth_value.starts_with("Bearer ") {
            return Err(AuthError::InvalidTokenFormat);
        }

        let token = auth_value[7..].trim();
        if token.is_empty() {
            return Err(AuthError::InvalidTokenFormat);
        }

        Ok(UserId(token.to_string()))
    }
}

/// Authentication middleware to verify a user's token
pub async fn require_auth(
    State(state): State<crate::api::routes::AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let storage = &state.storage;
    // Extract the bearer token from the Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or(AuthError::MissingToken)?;

    let auth_value = auth_header
        .to_str()
        .map_err(|_| AuthError::InvalidTokenFormat)?;

    if !auth_value.starts_with("Bearer ") {
        return Err(AuthError::InvalidTokenFormat);
    }

    let token = &auth_value[7..];
    if token.is_empty() {
        return Err(AuthError::InvalidTokenFormat);
    }

    // Verify the token by checking if the user exists
    match storage.get_user(token).await {
        Ok(_) => {
            // User exists, proceed with the request
            debug!("User authenticated: {}", token);
            Ok(next.run(request).await)
        }
        Err(StorageError::UserNotFound) => Err(AuthError::UserNotFound),
        Err(e) => Err(AuthError::StorageError(e)),
    }
}
