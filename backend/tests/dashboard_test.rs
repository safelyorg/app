mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_user, create_test_user,
    insert_test_history_chain, test_pool,
};
use axum::{extract::State, http::HeaderMap};
use backend::{errors::dashboard::DashboardError, handlers::dashboard::get_history};
use serde_json::json;

#[tokio::test]
async fn get_history_unauthorized() {
    let pool = test_pool().await;

    let headers = HeaderMap::new();

    let result = get_history(State(pool), headers).await;

    match result {
        Err(DashboardError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn get_history_success() {
    let pool = test_pool().await;
    let email = "get_history_success_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    cleanup_test_seller_chain(&pool, "olx", "history_test_seller_001").await;
    let (_, _) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_test_seller_001",
        "Test Listing Title",
    )
    .await;

    let result = get_history(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    let history = result["history"]
        .as_array()
        .expect("expected history to be a real array");

    assert_eq!(history.len(), 1, "expected exactly one history item");
    assert_eq!(history[0]["listing_title"], json!("Test Listing Title"));
    assert_eq!(history[0]["platform"], json!("olx"));
    assert_eq!(history[0]["seller_name"], json!("Test Seller"));
    assert_eq!(history[0]["reported"], json!(false));

    cleanup_test_seller_chain(&pool, "olx", "history_test_seller_001").await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_database_error() {
    let pool = test_pool().await;
    let email = "get_history_db_error_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    cleanup_test_user(&pool, email).await;
    pool.close().await;

    let result = get_history(State(pool), headers).await;

    match result {
        Err(DashboardError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => panic!("expected a genuine database error, but the request succeeded"),
    }
}
