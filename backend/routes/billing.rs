use crate::handlers::billing::{
    cancel_subscription_handler, change_plan_handler, create_checkout_handler, creem_webhook,
    get_product_ids, get_subscription_status, resume_subscription_handler,
};
use crate::handlers::team::{change_seats, get_team, invite_member, leave_team, remove_member};
use axum::{
    Router,
    routing::{get, post},
};
use sqlx::{Pool, Postgres};

pub fn billing_routes() -> Router<Pool<Postgres>> {
    Router::new()
        .route("/api/v1/billing/checkout", post(create_checkout_handler))
        .route("/api/v1/webhooks/creem", post(creem_webhook))
        .route(
            "/api/v1/billing/cancel-subscription",
            post(cancel_subscription_handler),
        )
        .route(
            "/api/v1/billing/resume-subscription",
            post(resume_subscription_handler),
        )
        .route(
            "/api/v1/billing/subscription-status",
            get(get_subscription_status),
        )
        .route("/api/v1/billing/change-plan", post(change_plan_handler))
        .route("/api/v1/billing/product-ids", get(get_product_ids))
        // Team plans: invite people, extra seats (handlers/team.rs)
        .route("/api/v1/team", get(get_team))
        .route("/api/v1/team/invite", post(invite_member))
        .route("/api/v1/team/remove", post(remove_member))
        .route("/api/v1/team/leave", post(leave_team))
        .route("/api/v1/team/seats", post(change_seats))
}
