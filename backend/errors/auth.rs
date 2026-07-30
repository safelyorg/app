use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;

const DASHBOARD_PATH: &str = "/dashboard/";

#[derive(Debug)]
pub enum AuthError {
    BadRequest,
    InternalServerError(String),
    DashboardPath(String),
    DashboardPathWithJar(CookieJar, String),
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
