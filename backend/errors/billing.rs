use axum::{
    Json,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub enum BillingError {
    Unauthorized,
    NotFound(String),
    InvalidRequest(String),
    ServiceUnavailable(String),
    InternalError(String),
}

#[derive(Debug)]
pub enum WebhookError {
    Misconfigured(String),
    MissingSignature,
    InvalidBody,
    InvalidSignature,
    InvalidPayload(String),
}

impl IntoResponse for BillingError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            BillingError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Not authenticated".to_string())
            }
            BillingError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            BillingError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            BillingError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            BillingError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl IntoResponse for WebhookError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            WebhookError::Misconfigured(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            WebhookError::MissingSignature => (
                StatusCode::BAD_REQUEST,
                "Missing signature header".to_string(),
            ),
            WebhookError::InvalidBody => (
                StatusCode::BAD_REQUEST,
                "Body is not valid UTF-8".to_string(),
            ),
            WebhookError::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "Signature verification failed".to_string(),
            ),
            WebhookError::InvalidPayload(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
