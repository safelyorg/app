use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::NaiveDate;
use serde_json::json;

#[derive(Debug)]
pub enum AnalyzeError {
    Unauthorized,
    // Contains exactly how many seconds remain before this user can try again
    RateLimited(u64),
    Database(String),
    ClaudeAnalysisFailed(String),
    SerializationFailed(String),
    ScanLimitReached(i32),
    /// Free plan used up for this month; comes back on `resets_on`.
    FreeScanLimitReached {
        limit: i32,
        resets_on: NaiveDate,
    },
}

impl IntoResponse for AnalyzeError {
    fn into_response(self) -> Response {
        match self {
            AnalyzeError::RateLimited(secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                format!("RATE_LIMITED:{}", secs),
            )
                .into_response(),

            AnalyzeError::ScanLimitReached(limit) => {
                let body = json!({
                    "error": "scan_limit_reached",
                    "message": format!(
                        "You've used all {} scans included in your plan this month.",
                        limit
                    ),
                    "limit": limit,
                });
                (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
            }

            AnalyzeError::FreeScanLimitReached { limit, resets_on } => {
                let body = json!({
                    "error": "free_scan_limit_reached",
                    "message": format!(
                        "You've used your {} free scans for this month. Upgrade to Team or Enterprise to keep scanning now, or your free scans come back on {}.",
                        limit,
                        resets_on.format("%B %-d, %Y")
                    ),
                    "limit": limit,
                    "resets_on": resets_on.to_string(),
                });
                (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
            }

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
                    AnalyzeError::ScanLimitReached(_) => unreachable!(),
                    AnalyzeError::FreeScanLimitReached { .. } => unreachable!(),
                };

                let body = json!({ "error": error_code, "message": message });
                (status, Json(body)).into_response()
            }
        }
    }
}
