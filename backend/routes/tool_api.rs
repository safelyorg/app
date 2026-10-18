use crate::handlers::domain_age::domain_age;
use axum::{Router, routing::get};
use sqlx::{Pool, Postgres};

/// The lookups behind the free tools. Answers change and depend on the
/// visitor, so these sit with the other API routes and are never cached.
///
/// - /api/v1/tools/domain-age?domain=supplier.com   Domain Age Checker
pub fn tool_api_routes() -> Router<Pool<Postgres>> {
    Router::new().route("/api/v1/tools/domain-age", get(domain_age))
}
