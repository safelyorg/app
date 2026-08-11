use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum AnalyzeError {
    Unauthorized,
    // Contains exactly how many seconds remain before this user can try again
    RateLimited(u64),
    Database(String),
    ClaudeAnalysisFailed(String),
    SerializationFailed(String),
}

impl IntoResponse for AnalyzeError {
    fn into_response(self) -> Response {
        match self {
            AnalyzeError::RateLimited(secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                format!("RATE_LIMITED:{}", secs),
            )
                .into_response(),

            other => {
                let (status, error_code, message) = match other {
                    AnalyzeError::Unauthorized => (
                        StatusCode::UNAUTHORIZED,
                        "unauthorized",
                        "Sign in required".to_string(),
                    ),
                    AnalyzeError::Database(msg) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "database_error", msg)
                    }
                    AnalyzeError::ClaudeAnalysisFailed(msg) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "analysis_failed", msg)
                    }
                    AnalyzeError::SerializationFailed(msg) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "serialization_failed",
                        msg,
                    ),
                    AnalyzeError::RateLimited(_) => unreachable!(),
                };

                let body = json!({ "error": error_code, "message": message });
                (status, Json(body)).into_response()
            }
        }
    }
}
