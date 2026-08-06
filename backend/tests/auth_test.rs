mod common;

use axum::{
    Json,
    extract::State,
    http::{Request, StatusCode},
};
use backend::{
    errors::auth::AuthError,
    handlers::auth::request_magic_link,
    models::users::MagicLinkRequest,
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

    let response = app
        .oneshot(request)
        .await
        .expect("expected to get the response");

    assert_eq!(response.status(), StatusCode::OK);

    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(test_email)
            .fetch_optional(&pool)
            .await
            .expect("expected to select a query from magic link");

    assert!(row.is_some(), "expected a magic link to be inserted");

    let magic_link_response = request_magic_link(
        State(pool.clone()),
        Json(MagicLinkRequest {
            email: test_email.to_string(),
        }),
    )
    .await
    .expect("expected to request the magic link");

    assert!(magic_link_response.success);

    cleanup_test_user(&pool, test_email).await;
}

#[tokio::test]
async fn requesting_a_magic_link_rejects_an_invalid_email() {
    let pool = test_pool().await;
    let app = auth_routes().with_state(pool.clone());

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/magic-link")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email": "not-an-email"}"#))
        .unwrap();

    let response = app
        .oneshot(request)
        .await
        .expect("expected to get the response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let result = request_magic_link(
        State(pool.clone()),
        Json(MagicLinkRequest {
            email: "not-an-email".to_string(),
        }),
    )
    .await;

    assert!(
        result.is_err(),
        "expected the invalid email address to be rejected"
    );
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
    let validated_email = validate_email_format(test_email).expect("email needs to be trimmed");
    cleanup_test_user(&pool, &validated_email).await;

    let email_token = insert_magic_link(&pool, validated_email.as_str())
        .await
        .expect("the magic link needs to be inserted");
    let base_url = var("PUBLIC_BASE_URL").expect("public base url needs to be configured");
    let verify_url = format!("{}/api/v1/auth/verify?token={}", base_url, email_token);

    send_magic_link_email(&validated_email, &verify_url)
        .await
        .expect("magic link needs to be sent");

    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(&validated_email)
            .fetch_optional(&pool)
            .await
            .expect("query is expecting email from magic link");

    assert!(
        row.is_some(),
        "expected a magic link row to actually exist in the database"
    );

    let result = request_magic_link(
        State(pool.clone()),
        Json(MagicLinkRequest {
            email: validated_email.clone(),
        }),
    )
    .await
    .expect("expected the magic link request to succeed");

    assert_eq!(result.success, true);
    assert_eq!(
        result.message,
        "If that email is valid, a sign-in link is on its way."
    );

    cleanup_test_user(&pool, &validated_email).await;
}

#[tokio::test]
async fn checking_magic_link_insertion() {
    let pool = test_pool().await;
    let email = "test_user@example.com";
    cleanup_test_user(&pool, email).await;

    // Method 1 - manual, direct SQL insert
    let manual_id = Uuid::now_v7();
    let manual_token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(15);

    query("INSERT INTO magic_links (id, email, token, expires_at) VALUES($1, $2, $3, $4)")
        .bind(manual_id)
        .bind(email)
        .bind(&manual_token)
        .bind(expires_at)
        .execute(&pool)
        .await
        .expect("manual magic link needs to be inserted");

    // Method 2 - calling the real, actual function
    let function_token = insert_magic_link(&pool, email)
        .await
        .expect("expected insert_magic_link to succeed");

    // The tokens themselves will NEVER match - they're independently
    // random by design. What we CAN genuinely verify: both rows
    // exist, are tied to the same email, and are two DISTINCT rows
    // (proving insert_magic_link doesn't collide with or overwrite
    // an existing row for the same email).
    let manual_row_email: String = query_scalar("SELECT email FROM magic_links WHERE token = $1")
        .bind(&manual_token)
        .fetch_one(&pool)
        .await
        .expect("manual row should exist");

    let function_row_email: String = query_scalar("SELECT email FROM magic_links WHERE token = $1")
        .bind(&function_token)
        .fetch_one(&pool)
        .await
        .expect("function-created row should exist");

    assert_eq!(manual_row_email, email);
    assert_eq!(function_row_email, email);
    assert_ne!(
        manual_token, function_token,
        "expected two genuinely distinct tokens"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn sending_magic_link_email_succeeds() {
    dotenvy::dotenv().ok();

    let to_email = "delivered@resend.dev";
    let to_validated =
        validate_email_format(to_email).expect("needs to have a validated email address");
    let base_url = "http://localhost:3000";
    let verify_url = format!("{}/api/v1/auth/verify?token=1234", base_url);

    let result = send_magic_link_email(&to_validated, &verify_url).await;

    assert!(
        result.is_ok(),
        "expected the email to send successfully, got: {:?}",
        result
    );
}

#[tokio::test]
async fn sending_magic_link_email_fails_for_a_blocked_domain() {
    dotenvy::dotenv().ok();

    let to_email = "test_user@example.com";
    let to_validated =
        validate_email_format(to_email).expect("needs to have a validated email address");
    let base_url = "http://localhost:3000";
    let verify_url = format!("{}/api/v1/auth/verify?token=1234", base_url);

    let result = send_magic_link_email(&to_validated, &verify_url).await;

    match result {
        Err(AuthError::InternalServerError(message)) => {
            assert!(
                message.contains("Invalid `to` field"),
                "expected Resend's specific rejection message, got: {}",
                message
            )
        }
        other => panic!("expected an InternalServerError, got: {:?}", other),
    }
}
