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
    SubscriptionRequired,
    ScanLimitReached(i32),
    TrialScanLimitReached(i32),
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
                        "You've used all {} scans included in your plan this billing period.",
                        limit
                    ),
                    "limit": limit,
                });
                (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response()
            }

            AnalyzeError::TrialScanLimitReached(limit) => {
                let body = json!({
                    "error": "trial_scan_limit_reached",
                    "message": format!(
                        "You've used all {} scans included in your free trial. Your subscription will begin billing and unlock your full plan limit once the trial period ends.",
                        limit
                    ),
                    "limit": limit,
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
                    AnalyzeError::SubscriptionRequired => (
                        StatusCode::PAYMENT_REQUIRED,
                        "subscription_required",
                        "An active subscription is required to analyze listings.".to_string(),
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
                    AnalyzeError::TrialScanLimitReached(_) => unreachable!(),
                };

                let body = json!({ "error": error_code, "message": message });
                (status, Json(body)).into_response()
            }
        }
    }
}
