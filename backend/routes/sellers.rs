use crate::handlers::sellers::create_seller;
use axum::{Router, routing::post};
use sqlx::{Pool, Postgres};

pub fn seller_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/sellers", post(create_seller))
}
