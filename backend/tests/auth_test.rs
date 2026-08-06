mod common;

use axum::http::{Request, StatusCode};
use backend::{
    routes::auth::auth_routes,
    services::{
        auth::{insert_magic_link, validate_email_format},
        email::send_magic_link_email,
    },
};
use chrono::{Duration, Utc};
use common::{cleanup_test_user, test_pool};
use dotenvy::var;
use reqwest::Body;
use sqlx::{query, query_as, query_scalar};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn requesting_a_magic_link_succeeds_for_a_valid_email() {
    let pool = test_pool().await;
    let test_email = "test_user@example.com";
    cleanup_test_user(&pool, test_email).await;

    let app = auth_routes().with_state(pool.clone());
    let body_test_email = format!(r#"{{"email": "{}"}}"#, test_email);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/magic-link")
        .header("content-type", "application/json")
        .body(Body::from(body_test_email))
        .expect("the request needs to be built");

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(test_email)
            .fetch_optional(&pool)
            .await
            .unwrap();

    assert!(row.is_some(), "expected a magic link to be inserted");

    cleanup_test_user(&pool, test_email).await;
}

#[tokio::test]
async fn requesting_a_magic_link_rejects_an_invalid_email() {
    let pool = test_pool().await;
    let app = auth_routes().with_state(pool);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/magic-link")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email": "not-an-email"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn checking_email_format() {
    let test_email = "test_user@example.com";
    let trimmed_email = test_email.trim();
    let lowercase_email = trimmed_email.to_lowercase();
    let contains_ampersand = lowercase_email.contains("@");
    let not_empty = !lowercase_email.is_empty();
    let validate = validate_email_format(test_email);
    let pool = test_pool().await;
    cleanup_test_user(&pool, test_email).await;

    assert_eq!(trimmed_email, test_email);
    assert_eq!(lowercase_email, test_email);
    assert_eq!(contains_ampersand, true);
    assert_eq!(not_empty, true);
    assert_eq!(validate.is_ok(), true);
}

#[tokio::test]
async fn requesting_magic_link() {
    let test_email = "delivered@resend.dev ";
    let pool = test_pool().await;
    let trimmed_email = validate_email_format(test_email).expect("email needs to be trimmed");

    cleanup_test_user(&pool, &trimmed_email).await;

    let email_token = insert_magic_link(&pool, trimmed_email.as_str())
        .await
        .expect("the magic link needs to be inserted");
    let base_url = var("PUBLIC_BASE_URL").expect("public base url needs to be configured");
    let verify_url = format!("{}/api/v1/auth/verify?token={}", base_url, email_token);

    send_magic_link_email(&trimmed_email, &verify_url)
        .await
        .expect("magic link needs to be sent");

    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(&trimmed_email)
            .fetch_optional(&pool)
            .await
            .expect("query is expecting email from magic link");

    assert!(
        row.is_some(),
        "expected a magic link row to actually exist in the database"
    );

    cleanup_test_user(&pool, &trimmed_email).await;
}

#[tokio::test]
async fn checking_magic_link_insertion() {
    let pool = test_pool().await;
    let id = Uuid::now_v7();
    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(15);
    let email = "test_user@example.com";
    cleanup_test_user(&pool, email).await;

    query("INSERT INTO magic_links (id, email, token, expires_at) VALUES($1, $2, $3, $4)")
        .bind(id)
        .bind(email)
        .bind(&token)
        .bind(expires_at)
        .execute(&pool)
        .await
        .expect("magic link needs to be inserted");

    let email_address: String =
        query_scalar("SELECT email FROM magic_links WHERE token = $1 AND expires_at = $2")
            .bind(&token)
            .bind(expires_at)
            .fetch_one(&pool)
            .await
            .expect("magic link row needs to exist");

    assert_eq!(email_address, String::from(email));

    cleanup_test_user(&pool, &email).await;
}
