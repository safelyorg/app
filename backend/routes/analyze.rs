use crate::handlers::analyze::analyze;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn analyze_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/analyze", post(analyze))
}
