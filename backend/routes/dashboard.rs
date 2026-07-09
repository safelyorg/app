use crate::handlers::dashboard::{
    get_avatar, get_history, get_history_item, get_me, get_reports, update_me, upload_avatar,
};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

pub fn dashboard_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/history", get(get_history))
        .route("/api/v1/history/{id}", get(get_history_item))
        .route("/api/v1/reports", get(get_reports))
        .route("/api/v1/me", get(get_me).patch(update_me))
        .route("/api/v1/me/avatar", get(get_avatar).post(upload_avatar))
}
