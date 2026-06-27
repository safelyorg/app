use crate::handlers::fraud_reports::create_fraud_report;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn fraud_reports_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/report", post(create_fraud_report))
}
