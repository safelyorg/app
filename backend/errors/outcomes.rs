use axum::{
    Json,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub enum OutcomeError {
    Unauthorized,
    BadRequest(String),
    InternalError(String),
}

impl IntoResponse for OutcomeError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            OutcomeError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Sign in required".to_string())
            }
            OutcomeError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            OutcomeError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
