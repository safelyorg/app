use axum::{
    Json,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub enum DashboardError {
    Unauthorized,
    NotFound(String),
    BadRequest(String),
    InternalError(String),
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            DashboardError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Sign in required".to_string())
            }
            DashboardError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            DashboardError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            DashboardError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
