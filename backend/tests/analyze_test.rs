mod common;

use axum::http::{HeaderMap, HeaderValue};
use backend::{
    handlers::analyze::{RATE_LIMITS, authorize_request, check_rate_limit},
    services::auth::{create_session, find_or_create_user_by_email},
};
use common::test_pool;
use std::{collections::HashMap, sync::Mutex, time::Duration, time::Instant};
use uuid::Uuid;

use crate::common::cleanup_test_user;

#[test]
fn checking_rate_limit_one_time() {
    let user_id = Uuid::new_v4();

    let result = check_rate_limit(user_id);

    assert!(result.is_ok(), "expected the very first call to succeed");
}

#[test]
fn checking_rate_limit_five_times() {
    let user_id = Uuid::new_v4();

    for call_number in 0..=5 {
        let result = check_rate_limit(user_id);
        assert!(
            result.is_ok(),
            "expected call number {} to succeed, got: {:?}",
            call_number,
            result
        );
    }
}

#[test]
fn check_rate_limit_blocks_the_eleventh_call_within_the_window() {
    let user_id = Uuid::new_v4();

    for call_number in 1..=10 {
        let result = check_rate_limit(user_id);
        assert!(
            result.is_ok(),
            "expected call number {} to succeed, got: {:?}",
            call_number,
            result
        );
    }

    let eleventh_result = check_rate_limit(user_id);
    assert!(
        eleventh_result.is_err(),
        "expected the 11th call to be blocked, got: {:?}",
        eleventh_result
    );
}

#[test]
fn check_rate_limit_resets_after_the_window_expires() {
    let user_id = Uuid::new_v4();

    let map = RATE_LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let mut map = map.lock().expect("expected to lock the map");
        let time_passed = Instant::now() - Duration::from_secs(400);
        map.insert(user_id, (11, time_passed));
    }

    let result = check_rate_limit(user_id);

    assert!(
        result.is_ok(),
        "expected the expired window to reset, allowing this call through, got: {:?}",
        result
    );
}

#[tokio::test]
async fn authorize_request_success() {
    let pool = test_pool().await;
    let email = "authorize_success@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create or find a user");

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header values"),
    );

    let result = authorize_request(&headers, &pool)
        .await
        .expect("expected to run the authorize request");

    assert_eq!(result, user.id);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn authorize_request_failure() {
    let pool = test_pool().await;
    let email = "authorize_failure@example.com";
    cleanup_test_user(&pool, email).await;

    let (_user, _) = find_or_create_user_by_email(&pool, &email)
        .await
        .expect("expected to find or create a user");

    let headers = HeaderMap::new();
    let result = authorize_request(&headers, &pool).await;

    assert!(result.is_err(), "expected an empty header to be rejected");

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn authorize_request_rate_limit_passed() {
    let pool = test_pool().await;
    let email = "authorize_rate_limit@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create or find a user");

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header values"),
    );

    for call_number in 1..=10 {
        let result = check_rate_limit(user.id);
        assert!(
            result.is_ok(),
            "expected call number {} to succeed, got: {:?}",
            call_number,
            result
        );
    }

    let result = authorize_request(&headers, &pool).await;

    assert!(
        result.is_err(),
        "expected authorize_request to reject a rate-limited user"
    );

    cleanup_test_user(&pool, email).await;
}
