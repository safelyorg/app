use axum::http::{HeaderMap, HeaderValue};
use backend::{
    models::users::User,
    services::auth::{create_session, find_or_create_user_by_email},
};
use chrono::{DateTime, Utc};
use hex::encode;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
use std::env::var;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[allow(dead_code)]
pub struct TestSubscriptionOptions {
    pub current_period_end: Option<DateTime<Utc>>,
    pub scheduled_product_id: Option<&'static str>,
    pub scheduled_plan_name: Option<&'static str>,
}

pub async fn test_pool() -> Pool<Postgres> {
    dotenvy::dotenv().ok();
    let url = var("APP_URL").expect("the url needs to set in the .env file");
    Pool::<Postgres>::connect(&url)
        .await
        .expect("failed to connect to the real database")
}

pub async fn cleanup_test_user(pool: &Pool<Postgres>, email: &str) {
    let _ = query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await;
}

#[allow(dead_code)]
pub async fn cleanup_test_seller(pool: &Pool<Postgres>, platform: &str, platform_id: &str) {
    let _ = query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(platform)
        .bind(platform_id)
        .execute(pool)
        .await;
}

#[allow(dead_code)]
pub async fn create_test_user(pool: &Pool<Postgres>, email: &str) -> (User, bool) {
    cleanup_test_user(pool, email).await;
    let (user, yes) = find_or_create_user_by_email(pool, email)
        .await
        .expect("expected to create the test user");

    (user, yes)
}

#[allow(dead_code)]
pub async fn auth_headers_for(pool: &Pool<Postgres>, user_id: Uuid) -> HeaderMap {
    let token = create_session(pool, user_id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))
            .expect("expected to insert the header value"),
    );

    headers
}

#[allow(dead_code)]
pub async fn get_subscription_status_text(pool: &Pool<Postgres>, sub_id: &str) -> Option<String> {
    query_scalar("SELECT status::text FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .fetch_optional(pool)
        .await
        .expect("expected the query itself to succeed")
}

#[allow(dead_code)]
pub fn compute_creem_signature(secret: &str, raw_body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("expected to build a real HMAC instance");
    mac.update(raw_body.as_bytes());
    encode(mac.finalize().into_bytes())
}

/// Deletes any subscription row matching this creem_subscription_id, if
/// one exists. Used both before a test (guarding against leftover data
/// from a previous run) and after (cleaning up what the test itself
/// created).
#[allow(dead_code)]
pub async fn cleanup_test_subscription(pool: &Pool<Postgres>, sub_id: &str) {
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(sub_id)
        .execute(pool)
        .await;
}

/// Inserts a real, minimal subscription row for testing - filling in
/// every column the real `subscriptions` table requires (including
/// creem_customer_id and creem_product_id, both NOT NULL), using
/// plain, made-up values for the fields a given test doesn't actually
/// care about.
#[allow(dead_code)]
pub async fn insert_test_subscription(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    sub_id: &str,
    plan_name: &str,
    status: &str,
) {
    let _ = query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id, creem_product_id,
            plan_name, status, created_at, updated_at
        )
         VALUES ($1, $2, $3, $4, $5, $6, $7::subscription_status, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind(format!("prod_{}_test", plan_name.to_lowercase()))
    .bind(plan_name)
    .bind(status)
    .execute(pool)
    .await;
}

#[allow(dead_code)]
pub async fn insert_test_subscription_full(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    sub_id: &str,
    plan_name: &str,
    status: &str,
    options: TestSubscriptionOptions,
) {
    let _ = query(
        "INSERT INTO subscriptions (
            id, user_id, creem_subscription_id, creem_customer_id, creem_product_id,
            plan_name, status, current_period_end, scheduled_product_id, scheduled_plan_name,
            created_at, updated_at
        )
         VALUES ($1, $2, $3, $4, $5, $6, $7::subscription_status, $8, $9, $10, NOW(), NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(sub_id)
    .bind("cust_fake_test_001")
    .bind(format!("prod_{}_test", plan_name.to_lowercase()))
    .bind(plan_name)
    .bind(status)
    .bind(options.current_period_end)
    .bind(options.scheduled_product_id)
    .bind(options.scheduled_plan_name)
    .execute(pool)
    .await;
}

// auth

#[allow(dead_code)]
pub async fn session_exists(pool: &Pool<Postgres>, token: &str) -> bool {
    let row: Option<(String,)> = query_as("SELECT token FROM sessions WHERE token = $1")
        .bind(token)
        .fetch_optional(pool)
        .await
        .expect("expected the query itself to succeed");

    row.is_some()
}

#[allow(dead_code)]
pub async fn magic_link_exists_for_email(pool: &Pool<Postgres>, email: &str) -> bool {
    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await
            .expect("expected the query itself to succeed");

    row.is_some()
}

/// Deletes a seller and everything genuinely depending on it - analysis
/// rows, then listings, then the seller itself - in the correct order,
/// so foreign keys never block the cleanup. Safe to call before a test
/// too, as a guard against leftover data from a previous failed run.
#[allow(dead_code)]
pub async fn cleanup_test_seller_chain(pool: &Pool<Postgres>, platform: &str, platform_id: &str) {
    let _ = query(
        "DELETE FROM analysis WHERE listing_id IN (
            SELECT id FROM listings WHERE seller_id IN (
                SELECT id FROM sellers WHERE platform = $1 AND platform_id = $2
            )
        )",
    )
    .bind(platform)
    .bind(platform_id)
    .execute(pool)
    .await;

    let _ = query(
        "DELETE FROM listings WHERE seller_id IN (
            SELECT id FROM sellers WHERE platform = $1 AND platform_id = $2
        )",
    )
    .bind(platform)
    .bind(platform_id)
    .execute(pool)
    .await;

    let _ = query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(platform)
        .bind(platform_id)
        .execute(pool)
        .await;
}

// dashboard

/// Inserts a real, connected chain for testing anything that reads
/// analysis history - a seller, a listing tied to that seller, and an
/// analysis row tied to both the listing and a real user. Returns the
/// listing_id and seller_id, since callers often need them for
/// assertions or further setup.
#[allow(dead_code)]
pub async fn insert_test_history_chain(
    pool: &Pool<Postgres>,
    user_id: Uuid,
    platform: &str,
    platform_id: &str,
    listing_title: &str,
) -> (Uuid, Uuid) {
    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Test Seller")
    .execute(pool)
    .await
    .expect("expected to create the test seller");

    let listing_id = Uuid::now_v7();
    let listing_url = format!("https://{}.com/item/{}", platform, platform_id);
    query(
        "INSERT INTO listings (id, seller_id, platform, listing_url, listing_id, title, first_seen_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
    )
    .bind(listing_id)
    .bind(seller_id)
    .bind(platform)
    .bind(&listing_url)
    .bind(platform_id)
    .bind(listing_title)
    .execute(pool)
    .await
    .expect("expected to create the test listing");

    query(
        "INSERT INTO analysis (id, listing_id, risk_score, risk_level, signals, user_id, created_at)
         VALUES ($1, $2, $3, 'low'::risk_level_type, $4, $5, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(listing_id)
    .bind(15_i16)
    .bind(serde_json::json!([]))
    .bind(user_id)
    .execute(pool)
    .await
    .expect("expected to create the test analysis");

    (listing_id, seller_id)
}
