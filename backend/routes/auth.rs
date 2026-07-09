use crate::handlers::auth::{
    google_callback, google_connect_redirect, google_redirect, request_magic_link,
    verify_magic_link,
};
use axum::{Router, routing::get, routing::post};
use sqlx::{Pool, Postgres};
pub fn auth_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/auth/magic-link", post(request_magic_link))
        .route("/api/v1/auth/verify", get(verify_magic_link))
        .route("/api/v1/auth/google", get(google_redirect))
        .route("/api/v1/auth/google/connect", get(google_connect_redirect))
        .route("/api/v1/auth/google/callback", get(google_callback))
}
