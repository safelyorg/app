mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_seller_with_reports,
    cleanup_test_user, create_test_user, insert_test_history_chain, set_analysis_created_at,
    test_pool,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use backend::{
    errors::dashboard::DashboardError,
    handlers::dashboard::{
        UpdateMeRequest, get_history, get_history_item, get_me, get_reports, update_me,
    },
    models::fraud_reports::ReportTypes,
    services::{
        auth::set_login_method,
        history::{get_history_detail, get_user_history, get_user_reports},
    },
};
use chrono::{Duration, Utc};
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
async fn get_user_history_empty_when_no_history() {
    let pool = test_pool().await;
    let email = "get_user_history_empty_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let result = get_user_history(&pool, user.id)
        .await
        .expect("expected the query itself to succeed");

    assert!(
        result.is_empty(),
        "expected an empty list, not an error, when no history exists"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_history_success_single_item() {
    let pool = test_pool().await;
    let email = "get_user_history_single_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    cleanup_test_seller_chain(&pool, "olx", "history_single_item_001").await;
    let (listing_id, seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_single_item_001",
        "Single Item Test Listing",
    )
    .await;

    let result = get_user_history(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(result.len(), 1, "expected exactly one history item");
    let item = &result[0];

    assert_eq!(
        item.listing_title,
        Some("Single Item Test Listing".to_string())
    );
    assert_eq!(item.platform, "olx");
    assert_eq!(item.seller_name, Some("Test Seller".to_string()));
    assert_eq!(item.seller_id, seller_id);
    assert!(
        !item.reported,
        "expected reported to be false, since no fraud report was filed"
    );

    cleanup_test_seller_chain(&pool, "olx", "history_single_item_001").await;
    cleanup_test_user(&pool, email).await;

    let _ = listing_id;
}

#[tokio::test]
async fn get_user_history_deduplicates_same_listing() {
    let pool = test_pool().await;
    let email = "get_user_history_dedup_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    cleanup_test_seller_chain(&pool, "olx", "history_dedup_001").await;

    let (listing_id, _seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_dedup_001",
        "Duplicated Listing",
    )
    .await;

    // A SECOND, genuinely later analysis of the SAME listing - a
    // deliberately different risk_score (99) lets us confirm exactly
    // which row survives deduplication.
    query(
        "INSERT INTO analysis (id, listing_id, risk_score, risk_level, signals, user_id, created_at)
         VALUES ($1, $2, $3, 'low'::risk_level_type, $4, $5, NOW() + INTERVAL '1 second')",
    )
    .bind(Uuid::now_v7())
    .bind(listing_id)
    .bind(99_i16)
    .bind(serde_json::json!([]))
    .bind(user.id)
    .execute(&pool)
    .await
    .expect("expected to create the second, later analysis");

    let result = get_user_history(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(
        result.len(),
        1,
        "expected only ONE entry, even though this listing was analyzed twice"
    );
    assert_eq!(
        result[0].risk_score, 99,
        "expected the MOST RECENT analysis (risk_score 99) to be the one kept"
    );

    cleanup_test_seller_chain(&pool, "olx", "history_dedup_001").await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_history_reported_scoped_per_listing() {
    let pool = test_pool().await;
    let email = "get_user_history_scoped_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "history_scoped_seller_001";

    // Clean up any leftovers - fraud_reports first, then the usual chain.
    query("DELETE FROM fraud_reports WHERE seller_id IN (SELECT id FROM sellers WHERE platform = $1 AND platform_id = $2)")
        .bind(platform)
        .bind(platform_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_test_seller_chain(&pool, platform, platform_id).await;

    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Same Seller, Two Listings")
    .execute(&pool)
    .await
    .expect("expected to create the seller");

    let listing_id_reported = Uuid::now_v7();
    let listing_url_reported = "https://olx.com/item/reported-listing-001";
    query(
        "INSERT INTO listings (id, seller_id, platform, listing_url, listing_id, title, first_seen_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
    )
    .bind(listing_id_reported)
    .bind(seller_id)
    .bind(platform)
    .bind(listing_url_reported)
    .bind("reported_listing_001")
    .bind("Reported Listing")
    .execute(&pool)
    .await
    .expect("expected to create the reported listing");

    let listing_id_clean = Uuid::now_v7();
    let listing_url_clean = "https://olx.com/item/clean-listing-002";
    query(
        "INSERT INTO listings (id, seller_id, platform, listing_url, listing_id, title, first_seen_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
    )
    .bind(listing_id_clean)
    .bind(seller_id)
    .bind(platform)
    .bind(listing_url_clean)
    .bind("clean_listing_002")
    .bind("Clean Listing")
    .execute(&pool)
    .await
    .expect("expected to create the clean listing");

    for listing_id in [listing_id_reported, listing_id_clean] {
        query(
            "INSERT INTO analysis (id, listing_id, risk_score, risk_level, signals, user_id, created_at)
             VALUES ($1, $2, $3, 'low'::risk_level_type, $4, $5, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(listing_id)
        .bind(15_i16)
        .bind(serde_json::json!([]))
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("expected to create the analysis");
    }

    // A fraud report, filed by THIS user, against ONLY the first listing.
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
    .bind(listing_url_reported)
    .execute(&pool)
    .await
    .expect("expected to create the fraud report");

    let result = get_user_history(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(result.len(), 2, "expected both listings to appear");

    let reported_item = result
        .iter()
        .find(|i| i.listing_url == listing_url_reported)
        .expect("expected to find the reported listing");
    let clean_item = result
        .iter()
        .find(|i| i.listing_url == listing_url_clean)
        .expect("expected to find the clean listing");

    assert!(
        reported_item.reported,
        "expected the specifically-reported listing to show reported: true"
    );
    assert!(
        !clean_item.reported,
        "expected the OTHER listing, from the SAME seller, to show reported: false"
    );

    query("DELETE FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_history_orders_newest_first() {
    let pool = test_pool().await;
    let email = "get_user_history_ordering_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    cleanup_test_seller_chain(&pool, "olx", "history_order_oldest_001").await;
    cleanup_test_seller_chain(&pool, "olx", "history_order_middle_001").await;
    cleanup_test_seller_chain(&pool, "olx", "history_order_newest_001").await;

    let (listing_oldest, _) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_order_oldest_001",
        "Oldest Listing",
    )
    .await;
    let (listing_middle, _) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_order_middle_001",
        "Middle Listing",
    )
    .await;
    let (listing_newest, _) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_order_newest_001",
        "Newest Listing",
    )
    .await;

    let now = Utc::now();
    set_analysis_created_at(&pool, listing_oldest, now - Duration::hours(3)).await;
    set_analysis_created_at(&pool, listing_middle, now - Duration::hours(2)).await;
    set_analysis_created_at(&pool, listing_newest, now - Duration::hours(1)).await;

    let result = get_user_history(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(result.len(), 3, "expected all three listings to appear");
    assert_eq!(
        result[0].listing_title,
        Some("Newest Listing".to_string()),
        "expected the newest listing to appear first"
    );
    assert_eq!(
        result[1].listing_title,
        Some("Middle Listing".to_string()),
        "expected the middle listing to appear second"
    );
    assert_eq!(
        result[2].listing_title,
        Some("Oldest Listing".to_string()),
        "expected the oldest listing to appear last"
    );

    cleanup_test_seller_chain(&pool, "olx", "history_order_oldest_001").await;
    cleanup_test_seller_chain(&pool, "olx", "history_order_middle_001").await;
    cleanup_test_seller_chain(&pool, "olx", "history_order_newest_001").await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_history_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let fake_user_id = Uuid::new_v4();
    let result = get_user_history(&pool, fake_user_id).await;

    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
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
async fn get_history_detail_not_found_nonexistent() {
    let pool = test_pool().await;
    let email = "get_history_detail_nonexistent_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let fake_analysis_id = Uuid::new_v4();

    let result = get_history_detail(&pool, fake_analysis_id, user.id)
        .await
        .expect("expected the query itself to succeed");

    assert!(
        result.is_none(),
        "expected None when no analysis matches this ID"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_detail_not_found_belongs_to_another_user() {
    let pool = test_pool().await;

    let email_owner = "history_detail_owner@example.com";
    let (owner, _) = create_test_user(&pool, email_owner).await;

    let email_intruder = "history_detail_intruder@example.com";
    let (intruder, _) = create_test_user(&pool, email_intruder).await;

    cleanup_test_seller_chain(&pool, "olx", "history_detail_owned_001").await;
    let (listing_id, _seller_id) = insert_test_history_chain(
        &pool,
        owner.id,
        "olx",
        "history_detail_owned_001",
        "Owner's Private Listing",
    )
    .await;

    let analysis_id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real analysis id");

    let result = get_history_detail(&pool, analysis_id, intruder.id)
        .await
        .expect("expected the query itself to succeed");

    assert!(
        result.is_none(),
        "expected None - a real security boundary, not leaking that this ID exists"
    );

    cleanup_test_seller_chain(&pool, "olx", "history_detail_owned_001").await;
    cleanup_test_user(&pool, email_owner).await;
    cleanup_test_user(&pool, email_intruder).await;
}

#[tokio::test]
async fn get_history_detail_success_no_reports() {
    let pool = test_pool().await;
    let email = "history_detail_no_reports_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    cleanup_test_seller_chain(&pool, "olx", "history_detail_no_reports_001").await;
    let (listing_id, _seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        "olx",
        "history_detail_no_reports_001",
        "Clean Listing Detail",
    )
    .await;

    let analysis_id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real analysis id");

    let result = get_history_detail(&pool, analysis_id, user.id)
        .await
        .expect("expected the query to succeed")
        .expect("expected the analysis detail to be found");

    assert_eq!(
        result.listing_title,
        Some("Clean Listing Detail".to_string())
    );
    assert_eq!(result.fraud_report_count, 0);
    assert!(!result.reported, "expected reported to be false");
    assert!(result.reports.is_empty(), "expected an empty reports list");

    cleanup_test_seller_chain(&pool, "olx", "history_detail_no_reports_001").await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_detail_success_with_multiple_reports() {
    let pool = test_pool().await;
    let email = "history_detail_multi_reports_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "history_detail_multi_reports_001";

    query("DELETE FROM fraud_reports WHERE seller_id IN (SELECT id FROM sellers WHERE platform = $1 AND platform_id = $2)")
        .bind(platform)
        .bind(platform_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller_chain(&pool, platform, platform_id).await;

    let (listing_id, seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        platform,
        platform_id,
        "Multiply Reported Listing",
    )
    .await;

    let analysis_id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real analysis id");

    let listing_url: String = query_scalar("SELECT listing_url FROM listings WHERE id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .expect("expected to find the real listing url");

    for report_type in ["scam", "fake_item"] {
        query(
            "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
             VALUES ($1, $2, $3, $4, $5, $6::report_types, $7, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(seller_id)
        .bind(user.id)
        .bind(platform)
        .bind(platform_id)
        .bind(report_type)
        .bind(&listing_url)
        .execute(&pool)
        .await
        .expect("expected to create the fraud report");
    }

    let result = get_history_detail(&pool, analysis_id, user.id)
        .await
        .expect("expected the query to succeed")
        .expect("expected the analysis detail to be found");

    assert!(result.reported, "expected reported to be true");
    assert_eq!(
        result.reports.len(),
        2,
        "expected BOTH reports to appear, not just one"
    );

    query("DELETE FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_history_detail_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let fake_analysis_id = Uuid::new_v4();
    let fake_user_id = Uuid::new_v4();
    let result = get_history_detail(&pool, fake_analysis_id, fake_user_id).await;

    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

#[tokio::test]
async fn get_user_reports_empty_when_no_reports() {
    let pool = test_pool().await;
    let email = "get_user_reports_empty_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let result = get_user_reports(&pool, user.id)
        .await
        .expect("expected the query itself to succeed");

    assert!(
        result.is_empty(),
        "expected an empty list, not an error, when no reports exist"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_reports_success_across_multiple_sellers() {
    let pool = test_pool().await;
    let email = "get_user_reports_multi_seller_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id_a = "reports_multi_seller_a_001";
    let platform_id_b = "reports_multi_seller_b_001";

    cleanup_test_seller_with_reports(&pool, platform, platform_id_a).await;
    cleanup_test_seller_with_reports(&pool, platform, platform_id_b).await;

    let seller_a_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_a_id)
    .bind(platform)
    .bind(platform_id_a)
    .bind("Seller A")
    .execute(&pool)
    .await
    .expect("expected to create seller A");

    let seller_b_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_b_id)
    .bind(platform)
    .bind(platform_id_b)
    .bind("Seller B")
    .execute(&pool)
    .await
    .expect("expected to create seller B");

    query(
        "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
         VALUES ($1, $2, $3, $4, $5, $6::report_types, $7, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(seller_a_id)
    .bind(user.id)
    .bind(platform)
    .bind(platform_id_a)
    .bind("scam")
    .bind("https://olx.com/item/report-against-a")
    .execute(&pool)
    .await
    .expect("expected to create the report against seller A");

    query(
        "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
         VALUES ($1, $2, $3, $4, $5, $6::report_types, $7, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(seller_b_id)
    .bind(user.id)
    .bind(platform)
    .bind(platform_id_b)
    .bind("fake_item")
    .bind("https://olx.com/item/report-against-b")
    .execute(&pool)
    .await
    .expect("expected to create the report against seller B");

    let result = get_user_reports(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(
        result.len(),
        2,
        "expected both reports, across two different sellers, to appear"
    );

    let names: Vec<&str> = result
        .iter()
        .map(|r| r.seller_name.as_deref().unwrap_or(""))
        .collect();

    assert!(
        names.contains(&"Seller A"),
        "expected the report against Seller A"
    );
    assert!(
        names.contains(&"Seller B"),
        "expected the report against Seller B"
    );

    cleanup_test_seller_with_reports(&pool, platform, platform_id_a).await;
    cleanup_test_seller_with_reports(&pool, platform, platform_id_b).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_reports_orders_newest_first() {
    let pool = test_pool().await;
    let email = "get_user_reports_ordering_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "reports_ordering_seller_001";
    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;

    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Ordering Test Seller")
    .execute(&pool)
    .await
    .expect("expected to create the seller");

    let now = Utc::now();

    let reports = [
        ("scam", now - Duration::hours(3)),        // oldest
        ("fake_item", now - Duration::hours(2)),   // middle
        ("no_delivery", now - Duration::hours(1)), // newest
    ];

    for (report_type, reported_at) in reports {
        query(
            "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
             VALUES ($1, $2, $3, $4, $5, $6::report_types, $7, $8)",
        )
        .bind(Uuid::now_v7())
        .bind(seller_id)
        .bind(user.id)
        .bind(platform)
        .bind(platform_id)
        .bind(report_type)
        .bind(format!("https://olx.com/item/ordering-{}", report_type))
        .bind(reported_at)
        .execute(&pool)
        .await
        .expect("expected to create the fraud report");
    }

    let result = get_user_reports(&pool, user.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(result.len(), 3, "expected all three reports to appear");
    assert_eq!(
        result[0].report_type,
        ReportTypes::NoDelivery,
        "expected the newest report first"
    );
    assert_eq!(
        result[1].report_type,
        ReportTypes::FakeItem,
        "expected the middle report second"
    );
    assert_eq!(
        result[2].report_type,
        ReportTypes::Scam,
        "expected the oldest report last"
    );

    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn get_user_reports_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let fake_user_id = Uuid::new_v4();
    let result = get_user_reports(&pool, fake_user_id).await;

    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
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

#[tokio::test]
async fn update_me_unauthorized() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let body = UpdateMeRequest {
        name: "Doesn't Matter".to_string(),
    };

    let result = update_me(State(pool), headers, Json(body)).await;

    match result {
        Err(DashboardError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn update_me_bad_request_empty_name() {
    let pool = test_pool().await;
    let email = "update_me_empty_name_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let body = UpdateMeRequest {
        name: "   ".to_string(),
    };

    let result = update_me(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(DashboardError::BadRequest(_)) => {}
        Err(other) => panic!("expected BadRequest, got a different error: {:?}", other),
        Ok(_) => panic!("expected a whitespace-only name to be rejected, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn update_me_bad_request_name_too_long() {
    let pool = test_pool().await;
    let email = "update_me_too_long_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let too_long_name = "a".repeat(101);

    let body = UpdateMeRequest {
        name: too_long_name,
    };

    let result = update_me(State(pool.clone()), headers, Json(body)).await;

    match result {
        Err(DashboardError::BadRequest(_)) => {}
        Err(other) => panic!("expected BadRequest, got a different error: {:?}", other),
        Ok(_) => panic!("expected a name over 100 characters to be rejected, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn update_me_success_trims_whitespace() {
    let pool = test_pool().await;
    let email = "update_me_trim_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let body = UpdateMeRequest {
        name: "   Bilal Khan   ".to_string(),
    };

    let result = update_me(State(pool.clone()), headers, Json(body))
        .await
        .expect("expected the update to succeed")
        .0;

    assert_eq!(
        result["name"],
        json!("Bilal Khan"),
        "expected the RESPONSE to show the trimmed name, not the raw input"
    );

    let saved_name: String = query_scalar("SELECT name FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("expected the query to succeed");

    assert_eq!(
        saved_name, "Bilal Khan",
        "expected the DATABASE to genuinely store the trimmed name"
    );

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn update_me_success_exactly_100_characters() {
    let pool = test_pool().await;
    let email = "update_me_exactly_100_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let exactly_100_name = "a".repeat(100);

    let body = UpdateMeRequest {
        name: exactly_100_name.clone(),
    };

    let result = update_me(State(pool.clone()), headers, Json(body))
        .await
        .expect("expected exactly 100 characters to be accepted, not rejected");

    assert_eq!(result.0["name"], json!(exactly_100_name));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn update_me_database_error() {
    let pool = test_pool().await;
    let email = "update_me_db_error_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let headers = auth_headers_for(&pool, user.id).await;

    cleanup_test_user(&pool, email).await;
    pool.close().await;

    let body = UpdateMeRequest {
        name: "Doesn't Matter".to_string(),
    };

    let result = update_me(State(pool), headers, Json(body)).await;

    match result {
        Err(DashboardError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => panic!("expected a genuine database error, but the request succeeded"),
    }
}
