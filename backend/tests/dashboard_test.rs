mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_seller_with_reports,
    cleanup_test_user, create_test_user, insert_test_history_chain, test_pool,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use backend::{
    errors::dashboard::DashboardError,
    handlers::dashboard::{get_history, get_history_item, get_me, get_reports},
    services::auth::set_login_method,
};
use serde_json::json;
use sqlx::{query, query_scalar};
use uuid::Uuid;

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

#[tokio::test]
async fn get_history_item_unauthorized() {
    let pool = test_pool().await;

    let headers = HeaderMap::new();
    let fake_id = Uuid::new_v4();

    let result = get_history_item(State(pool), headers, Path(fake_id)).await;
    match result {
        Err(DashboardError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn get_history_item_success() {
    let pool = test_pool().await;
    let email = "get_history_item_success_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    cleanup_test_seller_chain(&pool, "olx", "history_item_success_001").await;
    let (listing_id, _seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_item_success_001",
        "Test Listing For Detail",
    )
    .await;

    let analysis_id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real analysis id");

    let result = get_history_item(State(pool.clone()), headers, Path(analysis_id))
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(result["listing_title"], json!("Test Listing For Detail"));
    assert_eq!(result["platform"], json!("olx"));
    assert_eq!(result["reported"], json!(false));
    assert_eq!(result["fraud_report_count"], json!(0));

    cleanup_test_seller_chain(&pool, "olx", "history_item_success_001").await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_item_not_found_nonexistent() {
    let pool = test_pool().await;
    let email = "get_history_item_not_found_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;
    let fake_id = Uuid::new_v4();

    let result = get_history_item(State(pool.clone()), headers, Path(fake_id)).await;
    match result {
        Err(DashboardError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound, got a different error: {:?}", other),
        Ok(_) => panic!("expected a nonexistent analysis to be rejected, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_item_not_found_belongs_to_another_user() {
    let pool = test_pool().await;

    let email_a = "history_item_owner@example.com";
    let (user_a, _) = create_test_user(&pool, email_a).await;

    let email_b = "history_item_intruder@example.com";
    let (user_b, _) = create_test_user(&pool, email_b).await;
    let headers_b = auth_headers_for(&pool, user_b.id).await;

    cleanup_test_seller_chain(&pool, "olx", "history_item_owned_by_a_001").await;
    let (listing_id, _seller_id) = insert_test_history_chain(
        &pool,
        user_a.id,
        "olx",
        "history_item_owned_by_a_001",
        "Person A's Private Listing",
    )
    .await;

    let analysis_id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real analysis id");

    let result = get_history_item(State(pool.clone()), headers_b, Path(analysis_id)).await;

    match result {
        Err(DashboardError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound, got a different error: {:?}", other),
        Ok(_) => {
            panic!("expected person B to be denied access to person A's analysis, but it succeeded")
        }
    }

    cleanup_test_seller_chain(&pool, "olx", "history_item_owned_by_a_001").await;
    cleanup_test_user(&pool, email_a).await;
    cleanup_test_user(&pool, email_b).await;
}

#[tokio::test]
async fn get_reports_unauthorized() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let result = get_reports(State(pool), headers).await;
    match result {
        Err(DashboardError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn get_reports_success() {
    let pool = test_pool().await;
    let email = "get_reports_success_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let platform = "olx";
    let platform_id = "get_reports_seller_001";

    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;

    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Reported Seller")
    .execute(&pool)
    .await
    .expect("expected to create the seller");

    let listing_url = "https://olx.com.pk/item/get-reports-test-listing";
    query(
        "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
         VALUES ($1, $2, $3, $4, $5, $6::report_types, $7, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(seller_id)
    .bind(user.id)
    .bind(platform)
    .bind(platform_id)
    .bind("scam")
    .bind(listing_url)
    .execute(&pool)
    .await
    .expect("expected to create the fraud report");

    let result = get_reports(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    let reports = result["reports"]
        .as_array()
        .expect("expected reports to be a real array");

    assert_eq!(reports.len(), 1, "expected exactly one report");
    assert_eq!(reports[0]["seller_name"], json!("Reported Seller"));
    assert_eq!(reports[0]["platform"], json!("olx"));
    assert_eq!(reports[0]["listing_url"], json!(listing_url));
    assert_eq!(reports[0]["report_type"], json!("scam"));

    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_me_unauthorized() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let result = get_me(State(pool), headers).await;
    match result {
        Err(DashboardError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn get_me_success_with_explicit_login_method() {
    let pool = test_pool().await;
    let email = "get_me_explicit_method_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    set_login_method(&pool, user.id, "google")
        .await
        .expect("expected to set the login method");

    let result = get_me(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(
        result["signed_in_with"],
        json!("google"),
        "expected the explicit, real login method to be used directly"
    );
    assert_eq!(result["email"], json!(email));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_me_success_fallback_google_linked() {
    let pool = test_pool().await;
    let email = "get_me_fallback_google_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    // Genuinely link a Google ID directly, WITHOUT ever calling
    // set_login_method - this leaves last_login_method NULL.
    query("UPDATE users SET google_id = $1 WHERE id = $2")
        .bind("fallback_test_google_id_001")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("expected to link the google id directly");

    let result = get_me(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(
        result["signed_in_with"],
        json!("google"),
        "expected the fallback to correctly guess 'google', since last_login_method is NULL but google_id is set"
    );
    assert_eq!(result["google_linked"], json!(true));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_me_success_fallback_no_google_link() {
    let pool = test_pool().await;
    let email = "get_me_fallback_email_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let result = get_me(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(
        result["signed_in_with"],
        json!("email"),
        "expected the fallback to default to 'email', since neither last_login_method nor google_id is set"
    );
    assert_eq!(result["google_linked"], json!(false));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_me_success_has_avatar() {
    let pool = test_pool().await;
    let email = "get_me_has_avatar_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    // Genuinely real bytes stored - the actual content doesn't matter
    // here, only that avatar_data is NOT NULL.
    let fake_image_bytes: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0];
    query("UPDATE users SET avatar_data = $1, avatar_content_type = $2 WHERE id = $3")
        .bind(&fake_image_bytes)
        .bind("image/png")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("expected to set the avatar data");

    let result = get_me(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(
        result["has_avatar"],
        json!(true),
        "expected has_avatar to be true, since avatar_data is genuinely set"
    );

    cleanup_test_user(&pool, email).await;
}

// Deliberately no setup here - a brand-new test user genuinely has
// no avatar_data set at all, which is exactly what this scenario
// needs.
#[tokio::test]
async fn get_me_success_no_avatar() {
    let pool = test_pool().await;
    let email = "get_me_no_avatar_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let result = get_me(State(pool.clone()), headers)
        .await
        .expect("expected the request to succeed")
        .0;

    assert_eq!(
        result["has_avatar"],
        json!(false),
        "expected has_avatar to be false, since no avatar was ever set"
    );

    cleanup_test_user(&pool, email).await;
}
