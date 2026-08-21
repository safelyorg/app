mod common;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    response::IntoResponse,
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use backend::{
    errors::auth::AuthError,
    handlers::auth::{
        OAUTH_LINK_USER_COOKIE, OAUTH_STATE_COOKIE, google_callback, google_connect_redirect,
        google_redirect, handle_google_connect, logout, request_magic_link, verify_magic_link,
    },
    models::users::{
        GoogleAfterLoginQuery, GoogleConnectQuery, GoogleUserInfoEndpoint, MagicLinkRequest,
        VerifyMagicLinkToken,
    },
    routes::auth::auth_routes,
    services::{
        auth::{
            create_session, extract_user_id, find_or_create_user_by_email,
            find_or_create_user_by_google, find_user_by_email, find_user_by_google_id,
            finish_sign_in, insert_magic_link, link_google_account, validate_email_format,
            validate_magic_link,
        },
        email::{get_tera, send_magic_link_email},
        google_oauth::build_google_authorize_url,
    },
};
use chrono::{Duration as chrono_duration, Utc};
use common::{cleanup_test_user, create_test_user, test_pool};
use dotenvy::var;
use reqwest::Body;
use serial_test::serial;
use sqlx::{query, query_as, query_scalar};
use std::env::{remove_var, set_var};
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
        .expect("expected to build the request");

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
    let expires_at = Utc::now() + chrono_duration::minutes(15);

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

    let (user, is_new) = create_test_user(&pool, &validate.email).await;

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
    let (created_user, _) = create_test_user(&pool, &formatted_email).await;

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
    let (user, is_new) = create_test_user(&pool, email).await;

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
    let (first_user, first_is_new) = create_test_user(&pool, email).await;

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

#[tokio::test]
async fn handle_google_connect_succeeds_when_emails_match() {
    let pool = test_pool().await;
    let email = "google_link_success@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, user.id.to_string());
    let google_user = GoogleUserInfoEndpoint {
        id: "google_id_success_123".to_string(),
        email: email.to_string(), // matches the Safely account's real email
        name: Some("Test User".to_string()),
    };

    let jar = CookieJar::new();
    let result = handle_google_connect(&pool, jar, link_cookie, &google_user).await;

    assert!(
        result.is_ok(),
        "expected linking to succeed, got: {:?}",
        result
    );

    // Confirm the actual database was genuinely updated.
    let linked = find_user_by_google_id(&pool, "google_id_success_123")
        .await
        .expect("expected the query to succeed");
    assert!(
        linked.is_some(),
        "expected the user to now have a linked Google account"
    );
    assert_eq!(linked.unwrap().id, user.id);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn handle_google_connect_rejects_a_mismatched_email() {
    let pool = test_pool().await;
    let email = "google_link_mismatch@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, user.id.to_string());
    let google_user = GoogleUserInfoEndpoint {
        id: "google_id_mismatch_456".to_string(),
        email: "a_totally_different_email@example.com".to_string(), // deliberately different
        name: None,
    };

    let jar = CookieJar::new();
    let result = handle_google_connect(&pool, jar, link_cookie, &google_user).await;

    assert!(result.is_err(), "expected mismatched emails to be rejected");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn handle_google_connect_rejects_a_google_account_already_linked_elsewhere() {
    let pool = test_pool().await;
    let email_a = "google_link_owner@example.com";
    let email_b = "google_link_intruder@example.com";
    let (user_a, _) = create_test_user(&pool, email_a).await;
    let (user_b, _) = create_test_user(&pool, email_b).await;

    let shared_google_id = "google_id_already_taken_789";

    // User A already linked this Google account.
    link_google_account(&pool, user_a.id, shared_google_id)
        .await
        .expect("expected the first link to succeed");

    // Now user B tries to link the SAME Google account.
    let link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, user_b.id.to_string());
    let google_user = GoogleUserInfoEndpoint {
        id: shared_google_id.to_string(),
        email: email_b.to_string(), // matches user B's own email - only the Google ID conflicts
        name: None,
    };

    let jar = CookieJar::new();
    let result = handle_google_connect(&pool, jar, link_cookie, &google_user).await;

    assert!(
        result.is_err(),
        "expected an already-linked Google account to be rejected"
    );

    cleanup_test_user(&pool, email_a).await;
    cleanup_test_user(&pool, email_b).await;
}

#[tokio::test]
async fn handle_google_connect_rejects_a_malformed_link_cookie() {
    let pool = test_pool().await;

    let link_cookie = Cookie::new(OAUTH_LINK_USER_COOKIE, "not-a-real-uuid");
    let google_user = GoogleUserInfoEndpoint {
        id: "google_id_whatever".to_string(),
        email: "irrelevant@example.com".to_string(),
        name: None,
    };

    let jar = CookieJar::new();
    let result = handle_google_connect(&pool, jar, link_cookie, &google_user).await;

    assert!(
        result.is_err(),
        "expected a malformed cookie value to be rejected"
    );
}

#[tokio::test]
#[serial]
async fn google_redirect_success() {
    dotenvy::dotenv().ok();

    let jar = CookieJar::new();
    let (returned_jar, _redirect) = google_redirect(jar)
        .await
        .expect("expected google_redirect to succeed");

    let cookie = returned_jar
        .get(OAUTH_STATE_COOKIE)
        .expect("expected an oauth_state cookie to be present");

    assert!(
        !cookie.value().is_empty(),
        "expected the cookie's value to be a real, non-empty code"
    );
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.same_site(), Some(SameSite::Lax));
    assert_eq!(cookie.max_age(), Some(time::Duration::minutes(10)));

    let expected_secure = var("PUBLIC_BASE_URL")
        .map(|url| url.starts_with("https://"))
        .unwrap_or(false);

    assert_eq!(cookie.secure(), Some(expected_secure))
}

#[tokio::test]
#[serial]
async fn google_redirect_failure() {
    dotenvy::dotenv().ok();
    let original_value = var("GOOGLE_CLIENT_ID").ok();

    unsafe {
        remove_var("GOOGLE_CLIENT_ID");
    }

    let jar = CookieJar::new();
    let result = google_redirect(jar).await;

    assert!(
        result.is_err(),
        "expected google_redirect to fail when GOOGLE_CLIENT_ID is missing"
    );

    unsafe {
        if let Some(value) = original_value {
            set_var("GOOGLE_CLIENT_ID", value);
        }
    }
}

#[test]
#[serial]
fn google_authorize_url_success() {
    let client_key = "GOOGLE_CLIENT_ID";
    let uri_key = "GOOGLE_REDIRECT_URI";
    let random_code = Uuid::new_v4().to_string();
    let check_code = random_code.trim();

    unsafe {
        set_var(client_key, "CLIENT_VALUE");
        set_var(uri_key, "URI_VALUE");
    }

    let result = build_google_authorize_url(check_code);
    let url = result.expect("expected the URL to build successfully");

    assert!(
        url.contains("CLIENT_VALUE"),
        "expected the URL to contain the client id"
    );
    assert!(
        url.contains("URI_VALUE"),
        "expected the URL to contain the redirect uri"
    );
    assert!(
        url.contains(check_code),
        "expected the URL to contain the state code"
    );

    unsafe {
        remove_var(client_key);
        remove_var(uri_key);
    }
}

#[tokio::test]
#[serial]
async fn google_authorize_url_failure_on_client_id_missing() {
    dotenvy::dotenv().ok();

    let client_key = "GOOGLE_CLIENT_ID";
    let uri_key = "GOOGLE_REDIRECT_URI";
    let random_code = Uuid::new_v4().to_string();
    let check_code = random_code.trim();

    let original_client_id = var(client_key).ok();

    unsafe {
        remove_var(client_key);
        set_var(uri_key, "URI_VALUE");
    }

    let result = build_google_authorize_url(check_code);
    assert!(
        result.is_err(),
        "expected the URL building to fail when GOOGLE_CLIENT_ID is missing"
    );

    unsafe {
        remove_var(uri_key);
        if let Some(value) = original_client_id {
            set_var(client_key, value);
        }
    }
}

#[test]
#[serial]
fn google_authorize_url_failure_on_redirect_uri_missing() {
    let client_key = "GOOGLE_CLIENT_ID";
    let uri_key = "GOOGLE_REDIRECT_URI";
    let random_code = Uuid::new_v4().to_string();
    let check_code = random_code.trim();

    unsafe {
        set_var(client_key, "CLIENT_VALUE");
        remove_var(uri_key);
    }

    let result = build_google_authorize_url(check_code);
    assert!(
        result.is_err(),
        "expected the URL building to fail when GOOGLE_REDIRECT_URI is missing"
    );

    unsafe {
        remove_var(client_key);
    }
}

#[tokio::test]
#[serial]
async fn google_connect_redirect_succeeds_with_a_valid_session() {
    dotenvy::dotenv().ok();

    let pool = test_pool().await;
    let email = "google_connect_success@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    // Step 2 - call the actual function, using that real token.
    let jar = CookieJar::new();
    let result = google_connect_redirect(
        State(pool.clone()),
        Query(GoogleConnectQuery {
            session: real_session_token,
        }),
        jar,
    )
    .await;

    assert!(
        result.is_ok(),
        "expected the connect flow to succeed, got: {:?}",
        result
    );

    // Step 3 - confirm the special link_cookie contains the CORRECT
    // user's real ID, not just that some cookie exists.
    let (returned_jar, _redirect) = result.unwrap();
    let link_cookie = returned_jar
        .get(OAUTH_LINK_USER_COOKIE)
        .expect("expected an oauth_link_user_id cookie to be present");

    assert_eq!(link_cookie.value(), user.id.to_string());

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn google_connect_redirect_fails_with_an_invalid_session() {
    dotenvy::dotenv().ok();

    let pool = test_pool().await;
    let fake_token = Uuid::new_v4().to_string();

    let jar = CookieJar::new();
    let result = google_connect_redirect(
        State(pool.clone()),
        Query(GoogleConnectQuery {
            session: fake_token,
        }),
        jar,
    )
    .await;

    assert!(
        result.is_err(),
        "expected an invalid session to be rejected"
    );
}

#[tokio::test]
async fn extract_user_id_missing_authorization() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();
    let result = extract_user_id(&headers, &pool)
        .await
        .expect("expected the query itself to succeed");

    assert!(
        result.is_none(),
        "expected no user to be found for a missing header"
    );
}

#[tokio::test]
async fn extract_user_id_malformed_authorization() {
    let pool = test_pool().await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str("not-a-bearer-token").expect("expected to insert the header values"),
    );

    let result = extract_user_id(&headers, &pool)
        .await
        .expect("expected the user id to be extracted");

    assert!(
        result.is_none(),
        "expected no user to be found for a malformed header"
    );
}

#[tokio::test]
async fn extract_user_id_token_mismatch() {
    let pool = test_pool().await;
    let fake_token = Uuid::new_v4().to_string();

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", fake_token))
            .expect("expected to insert the header value"),
    );

    let result = extract_user_id(&headers, &pool)
        .await
        .expect("expected the user id to be extracted");

    assert!(
        result.is_none(),
        "expected no user to be found for a token that doesn't match any real session"
    );
}

#[tokio::test]
async fn extract_user_id_token_match() {
    let pool = test_pool().await;
    let email = "token_match@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to get the real session token");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header value"),
    );

    let result = extract_user_id(&headers, &pool)
        .await
        .expect("expected the user id to be extracted");

    assert_eq!(
        result,
        Some(user.id),
        "expected the correct user's ID to be found for a genuinely valid token"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn extract_user_db_error() {
    let pool = test_pool().await;
    pool.close().await;

    let fake_token = Uuid::new_v4().to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", fake_token))
            .expect("expected to insert the header value"),
    );

    let result = extract_user_id(&headers, &pool).await;

    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

#[tokio::test]
#[serial]
async fn logout_with_session_present() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "logged_out_success@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to enter the header value"),
    );

    let response = logout(State(pool.clone()), headers).await;
    assert_eq!(response.0["success"], true);

    let still_exists: Option<(String,)> = query_as("SELECT token FROM sessions WHERE token = $1")
        .bind(&real_session_token)
        .fetch_optional(&pool)
        .await
        .expect("expected the query to succeed");

    assert!(still_exists.is_none(), "expected the session to be deleted");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn logout_with_no_header() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "logged_out_success@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let headers = HeaderMap::new();

    let response = logout(State(pool.clone()), headers).await;
    assert_eq!(response.0["success"], true);

    let still_exists: Option<(String,)> = query_as("SELECT token FROM sessions WHERE token = $1")
        .bind(&real_session_token)
        .fetch_optional(&pool)
        .await
        .expect("expected the query to succeed");

    assert!(
        still_exists.is_some(),
        "expected the session to remain untouched, since no header was provided"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn logout_with_fake_token() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;

    let fake_session_token = Uuid::new_v4().to_string();

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", fake_session_token)).unwrap(),
    );

    let response = logout(State(pool.clone()), headers).await;
    assert_eq!(response.0["success"], true);
}

#[tokio::test]
#[serial]
async fn finish_sign_in_with_new_user() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "signin_with_new_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let session_token = finish_sign_in(&pool, user.id, true, email, "google")
        .await
        .expect("expected sign-in to complete successfully");

    assert!(
        !session_token.is_empty(),
        "expected a real, non-empty session token"
    );

    let found: Option<(String,)> = query_as("SELECT token FROM sessions WHERE token = $1")
        .bind(&session_token)
        .fetch_optional(&pool)
        .await
        .expect("expected the query to succeed");

    assert!(
        found.is_some(),
        "expected the session to genuinely exist in the database"
    );

    cleanup_test_user(&pool, email).await;
}
#[tokio::test]
#[serial]
async fn finish_sign_in_with_old_user() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "signin_with_old_user@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let session_token = finish_sign_in(&pool, user.id, false, email, "google")
        .await
        .expect("expected sign-in to complete successfully");

    assert!(
        !session_token.is_empty(),
        "expected a real, non-empty session token"
    );

    let found: Option<(String,)> = query_as("SELECT token FROM sessions WHERE token = $1")
        .bind(&session_token)
        .fetch_optional(&pool)
        .await
        .expect("expected the query to succeed");

    assert!(
        found.is_some(),
        "expected the session to genuinely exist in the database"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn google_callback_error_reported() {
    let pool = test_pool().await;
    let jar = CookieJar::new();
    let query = GoogleAfterLoginQuery {
        error: Some("access_denied".to_string()),
        code: None,
        state: None,
    };

    let result = google_callback(State(pool), jar, Query(query)).await;

    assert!(
        result.is_err(),
        "expected google_callback to reject a Google-reported error"
    );
}

#[tokio::test]
#[serial]
async fn google_callback_code_missing() {
    let pool = test_pool().await;
    let jar = CookieJar::new();
    let query = GoogleAfterLoginQuery {
        error: None,
        code: None,
        state: None,
    };
    let result = google_callback(State(pool), jar, Query(query)).await;
    assert!(
        result.is_err(),
        "expected google_callback to reject a request with no authorization code"
    );
}

#[tokio::test]
#[serial]
async fn google_callback_state_mismatch() {
    let pool = test_pool().await;
    let jar = CookieJar::new();
    let query = GoogleAfterLoginQuery {
        error: None,
        code: Some("some_real_looking_code".to_string()),
        state: Some("state_mismatch".to_string()),
    };
    let result = google_callback(State(pool), jar, Query(query)).await;
    assert!(
        result.is_err(),
        "expected google_callback to reject a request with a mismatched or missing state"
    );
}

#[tokio::test]
#[serial]
async fn fresh_google_signin_creates_account_and_session() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "fresh_google_signin@example.com";
    let google_id = "google_id_fresh_signin_001";
    cleanup_test_user(&pool, email).await;

    let (user, is_new) = find_or_create_user_by_google(&pool, google_id, email, Some("Test User"))
        .await
        .expect("expected to create a new user from Google info");

    assert!(is_new, "expected a brand-new user to be created");
    assert_eq!(user.email, email);

    let session_token = finish_sign_in(&pool, user.id, is_new, email, "google")
        .await
        .expect("expected sign-in to complete");

    assert!(!session_token.is_empty(), "expected a real session token");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
#[serial]
async fn google_signin_links_onto_an_existing_email_match() {
    dotenvy::dotenv().ok();
    let pool = test_pool().await;
    let email = "existing_magic_link_user@example.com";
    let google_id = "google_id_linking_to_existing_001";
    let (existing_user, _) = create_test_user(&pool, email).await;

    let (user, is_new) =
        find_or_create_user_by_google(&pool, google_id, email, Some("Existing User"))
            .await
            .expect("expected the call to succeed");

    assert!(
        !is_new,
        "expected this to link onto an existing account, not create a new one"
    );
    assert_eq!(
        user.id, existing_user.id,
        "expected the SAME user, not a new one"
    );

    let linked = find_user_by_google_id(&pool, google_id)
        .await
        .expect("expected the query to succeed");

    assert!(
        linked.is_some(),
        "expected the existing user to now have this Google ID linked"
    );
    assert_eq!(linked.unwrap().id, existing_user.id);

    cleanup_test_user(&pool, email).await;
}
