use crate::handlers::reviews::{
    get_my_review, get_my_review_photo, get_review_photo, list_public_reviews, submit_review,
    upload_review_photo,
};
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

pub fn reviews_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route(
            "/api/v1/reviews",
            get(list_public_reviews).post(submit_review),
        )
        .route("/api/v1/reviews/mine", get(get_my_review))
        .route(
            "/api/v1/reviews/mine/photo",
            get(get_my_review_photo).post(upload_review_photo),
        )
        .route("/api/v1/reviews/{id}/photo", get(get_review_photo))
}
