use axum::{
    Json,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub enum FraudReportError {
    Unauthorized,
    NotFound(String),
    /// This person already reported this seller. Each person can report
    /// a seller once, so one person can't push a score up alone.
    AlreadyReported,
    InternalError(String),
}

impl IntoResponse for FraudReportError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            FraudReportError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Sign in required".to_string())
            }
            FraudReportError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            FraudReportError::AlreadyReported => {
                (StatusCode::CONFLICT, "already_reported".to_string())
            }
            FraudReportError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
