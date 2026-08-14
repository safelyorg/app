use crate::handlers::subscribe::newsletter_subscribe;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn subscribe_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/newsletter/subscribe", post(newsletter_subscribe))
}
