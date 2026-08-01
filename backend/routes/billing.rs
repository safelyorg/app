use crate::handlers::billing::{
    cancel_subscription_handler, create_checkout_handler, creem_webhook, get_subscription_status,
};
use axum::{
    Router,
    routing::{get, post},
};
use sqlx::{Pool, Postgres};

pub fn billing_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/webhooks/creem", post(creem_webhook))
        .route("/api/v1/billing/checkout", post(create_checkout_handler))
        .route(
            "/api/v1/billing/cancel-subscription",
            post(cancel_subscription_handler),
        )
        .route(
            "/api/v1/billing/subscription-status",
            get(get_subscription_status),
        )
}
