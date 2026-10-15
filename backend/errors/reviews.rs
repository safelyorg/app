use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum ReviewError {
    /// Not signed in, or the session has expired.
    Unauthorized,
    /// Something the user sent is not valid. The text is shown to them.
    BadRequest(String),
    /// The review or photo does not exist (or is not approved yet).
    NotFound,
    /// A database or server problem. Logged, never shown to the user.
    InternalError(String),
}

impl IntoResponse for ReviewError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ReviewError::Unauthorized => (StatusCode::UNAUTHORIZED, "Please sign in".to_string()),
            ReviewError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ReviewError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            ReviewError::InternalError(detail) => {
                eprintln!("Safely: review error: {}", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Something went wrong. Please try again.".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
