use sqlx::{Pool, Postgres, query};
use std::env::var;

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
