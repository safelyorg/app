use crate::handlers::dashboard::{get_history, get_reports};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

pub fn dashboard_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/history", get(get_history))
        .route("/api/v1/reports", get(get_reports))
}
