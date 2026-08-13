use crate::handlers::newsletter::newsletter_subscribe;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn newsletter_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/newsletter/subscribe", post(newsletter_subscribe))
}
