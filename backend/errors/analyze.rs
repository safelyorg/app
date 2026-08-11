use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum AnalyzeError {
    // Contains exactly how many seconds remain before this user can try again
    RateLimited(u64),
}

impl IntoResponse for AnalyzeError {
    fn into_response(self) -> Response {
        match self {
            AnalyzeError::RateLimited(seconds_remaining) => {
                let body = json!({
                    "error": "rate_limited",
                    "message": format!(
                        "You've reached the request limit. Try again in {} seconds.",
                        seconds_remaining
                    ),
                    "retry_after_seconds": seconds_remaining
                });
                (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response()
            }
        }
    }
}
