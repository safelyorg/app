use crate::handlers::outcomes::create_outcome;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn outcomes_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/outcomes", post(create_outcome))
}
