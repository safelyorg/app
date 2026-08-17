use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use sqlx::Error;

const DASHBOARD_PATH: &str = "/dashboard/";

#[derive(Debug)]
pub enum AuthError {
    BadRequest,
    InternalServerError(String),
    DashboardPath(String),
    DashboardPathWithJar(CookieJar, String),
}

#[derive(Debug)]
pub enum AuthServiceError {
    InvalidEmail,
    Database(Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::BadRequest => (
                StatusCode::BAD_REQUEST,
                "invalid email address provided".to_string(),
            )
                .into_response(),
            AuthError::InternalServerError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
            AuthError::DashboardPath(reason) => {
                Redirect::to(&format!("{}?error={}", DASHBOARD_PATH, reason)).into_response()
            }
            AuthError::DashboardPathWithJar(jar, reason) => (
                jar,
                Redirect::to(&format!("{}?error={}", DASHBOARD_PATH, reason)),
            )
                .into_response(),
        }
    }
}

impl From<Error> for AuthServiceError {
    fn from(e: Error) -> Self {
        AuthServiceError::Database(e)
    }
}
