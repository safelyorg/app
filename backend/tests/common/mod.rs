use sqlx::{Pool, Postgres, query};
use std::env::var;
use uuid::Uuid;

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
