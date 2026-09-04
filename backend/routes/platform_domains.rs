use crate::services::platform_config::get_all_platform_domains;
use axum::{Json, Router, routing::get};
use serde_json::{json, Value};
use sqlx::{Pool, Postgres};

async fn get_platform_domains() -> Json<Value> {
    Json(json!(get_all_platform_domains()))
}

pub fn platform_domains_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/platform-domains", get(get_platform_domains))
}
