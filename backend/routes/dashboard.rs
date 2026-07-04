use crate::handlers::dashboard::{get_history, get_history_item, get_reports};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

pub fn dashboard_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/history", get(get_history))
        .route("/api/v1/history/{id}", get(get_history_item))
        .route("/api/v1/reports", get(get_reports))
}
