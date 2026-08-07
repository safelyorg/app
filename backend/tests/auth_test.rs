mod common;

use axum::{
    Json,
    extract::{Query, State},
    http::{Request, StatusCode},
    response::IntoResponse,
};
use backend::{
    errors::auth::AuthError,
    handlers::auth::{finish_sign_in, request_magic_link, verify_magic_link},
    models::users::{MagicLinkRequest, VerifyMagicLinkToken},
    routes::auth::auth_routes,
    services::{
        auth::{
            find_or_create_user_by_email, find_user_by_email, insert_magic_link,
            validate_email_format, validate_magic_link,
        },
        email::{get_tera, send_magic_link_email},
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
    let formatted_email = validate_email_format(test_email).expect("email needs to be trimmed");
    cleanup_test_user(&pool, &formatted_email).await;

    let email_token = insert_magic_link(&pool, formatted_email.as_str())
        .await
        .expect("the magic link needs to be inserted");
    let base_url = var("PUBLIC_BASE_URL").expect("public base url needs to be configured");
    let verify_url = format!("{}/api/v1/auth/verify?token={}", base_url, email_token);

    send_magic_link_email(&formatted_email, &verify_url)
        .await
        .expect("magic link needs to be sent");

    let row: Option<(String,)> =
        query_as("SELECT email FROM magic_links WHERE email = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(&formatted_email)
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
            email: formatted_email.clone(),
        }),
    )
    .await
    .expect("expected the magic link request to succeed");

    assert_eq!(result.success, true);
    assert_eq!(
        result.message,
        "If that email is valid, a sign-in link is on its way."
    );

    cleanup_test_user(&pool, &formatted_email).await;
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

    let base_url = "http://localhost:3000";
    let to_email = "delivered@resend.dev";
    let to_formatted =
        validate_email_format(to_email).expect("needs to have a validated email address");
    let verify_url = format!("{}/api/v1/auth/verify?token=1234", base_url);

    let result = send_magic_link_email(&to_formatted, &verify_url).await;

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

#[test]
fn get_tera_loads_all_five_email_templates() {
    let tera = get_tera();
    let registered_names: Vec<&str> = tera.get_template_names().collect();

    let expected_templates = [
        "email.html",
        "welcome_email.html",
        "payment_failed_email.html",
        "subscription_ended_email.html",
        "subscription_canceled_email.html",
    ];

    for name in expected_templates {
        assert!(
            registered_names.contains(&name),
            "expected template '{}' to be registered, found: {:?}",
            name,
            registered_names
        )
    }
}

#[tokio::test]
async fn checking_email_link_verification() {
    let pool = test_pool().await;
    let email = "test_user_verify_link@example.com";
    let formatted_email = validate_email_format(email).expect("required to have a formatted email");
    cleanup_test_user(&pool, email).await;

    let real_token = insert_magic_link(&pool, &formatted_email)
        .await
        .expect("it's expected to have the magic link inserted into the database");

    let validate = validate_magic_link(&pool, &real_token)
        .await
        .expect("expected the query to succeed")
        .expect("expected a valid, unexpired magic link to be found");

    assert_eq!(validate.email, email);

    let (user, is_new) = find_or_create_user_by_email(&pool, &validate.email)
        .await
        .expect("expected to find or create the user");

    let session_token = finish_sign_in(&pool, user.id, is_new, &validate.email, "email")
        .await
        .expect("expected sign-in to complete successfully");

    assert!(!session_token.is_empty(), "expected a real session token");

    cleanup_test_user(&pool, &formatted_email).await;
}

#[tokio::test]
async fn verify_magic_link_succeeds_with_a_real_token() {
    let pool = test_pool().await;
    let email = "test_user_verify_handler@example.com";
    cleanup_test_user(&pool, email).await;

    let real_token = insert_magic_link(&pool, email)
        .await
        .expect("expected to insert a real magic link");

    let result = verify_magic_link(
        State(pool.clone()),
        Query(VerifyMagicLinkToken { token: real_token }),
    )
    .await;

    assert!(
        result.is_ok(),
        "expected verify_magic_link to succeed, got: {:?}",
        result
    );

    let response = result.unwrap().into_response();
    let location = response
        .headers()
        .get("location")
        .expect("expected a Location header")
        .to_str()
        .unwrap();

    assert!(location.starts_with("/dashboard/#session="));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn verify_magic_link_rejects_a_token_that_was_never_inserted() {
    let pool = test_pool().await;
    let fake_token = Uuid::new_v4().to_string();

    let result = verify_magic_link(
        State(pool.clone()),
        Query(VerifyMagicLinkToken { token: fake_token }),
    )
    .await;

    assert!(
        result.is_err(),
        "expected a nonexistent token to be rejected"
    );
}

#[tokio::test]
async fn verify_magic_link_rejects_a_token_that_was_already_used() {
    let pool = test_pool().await;
    let email = "test_user2@example.com";
    cleanup_test_user(&pool, email).await;

    let real_token = insert_magic_link(&pool, email)
        .await
        .expect("expected to insert a real magic link");

    // First use - should succeed, and marks the token as used.
    let first_result = verify_magic_link(
        State(pool.clone()),
        Query(VerifyMagicLinkToken {
            token: real_token.clone(),
        }),
    )
    .await;
    assert!(first_result.is_ok(), "expected the first use to succeed");

    // Second use, same token - should be rejected, since
    // validate_magic_link only matches rows where used_at IS NULL.
    let second_result = verify_magic_link(
        State(pool.clone()),
        Query(VerifyMagicLinkToken { token: real_token }),
    )
    .await;
    assert!(
        second_result.is_err(),
        "expected a reused token to be rejected"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn find_user_by_email_finds_an_existing_user() {
    let pool = test_pool().await;
    let email = "test_user_find@example.com";
    let formatted_email = validate_email_format(email).expect("expected to format the email");
    cleanup_test_user(&pool, email).await;

    let (created_user, _) = find_or_create_user_by_email(&pool, &formatted_email)
        .await
        .expect("expected to create the user");

    let found = find_user_by_email(&pool, email)
        .await
        .expect("expected the query to succeed");

    let found_user = found.expect("expected to find the user that was just created");
    assert_eq!(found_user.id, created_user.id);
    assert_eq!(found_user.email, email);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn find_user_by_email_returns_none_for_a_nonexistent_user() {
    let pool = test_pool().await;
    let email = "definitely_does_not_exist_anywhere@example.com";
    let formatted_email = validate_email_format(email).expect("expected to format the email");
    cleanup_test_user(&pool, email).await;

    let found = find_user_by_email(&pool, &formatted_email)
        .await
        .expect("expected the query itself to succeed");

    assert!(found.is_none(), "expected no user to be found");
}

#[tokio::test]
async fn find_or_create_user_by_email_creates_a_new_user_when_none_exists() {
    let pool = test_pool().await;
    let email = "find_or_create_new@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, is_new) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected the call to succeed");

    assert!(is_new, "expected a brand-new user to be reported as new");
    assert_eq!(user.email, email);

    // Confirm the row genuinely exists now, independent of the
    // function's own return value.
    let found = find_user_by_email(&pool, email)
        .await
        .expect("expected the query to succeed");

    assert!(
        found.is_some(),
        "expected the new user to actually be in the database"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn find_or_create_user_by_email_returns_the_existing_user_when_one_already_exists() {
    let pool = test_pool().await;
    let email = "find_or_create_existing@example.com";
    cleanup_test_user(&pool, email).await;

    // First call - genuinely creates the user.
    let (first_user, first_is_new) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected the first call to succeed");
    assert!(first_is_new, "expected the first call to report a new user");

    // Second call, same email - should find the SAME user, not create another.
    let (second_user, second_is_new) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected the second call to succeed");

    assert!(
        !second_is_new,
        "expected the second call to report an existing user, not new"
    );
    assert_eq!(
        second_user.id, first_user.id,
        "expected the exact same user to be returned"
    );

    cleanup_test_user(&pool, email).await;
}
