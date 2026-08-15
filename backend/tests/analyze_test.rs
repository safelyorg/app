mod common;

use axum::http::{HeaderMap, HeaderValue};
use backend::{
    handlers::analyze::{
        RATE_LIMITS, authorize_request, build_requests, check_rate_limit, resolve_seller,
        run_claude_analysis,
    },
    models::{
        analysis::AnalyzeRequest,
        listings::{ListingCategory, Listings},
        sellers::{SellerVerification, Sellers, SellersRequest},
    },
    services::{
        auth::{create_session, find_or_create_user_by_email},
        fraud_reports::{build_network_summary, count_fraud_reports},
        sellers::{create_seller, find_seller},
    },
};
use chrono::{NaiveDate, Utc};
use common::test_pool;
use serial_test::serial;
use sqlx::query;
use std::{
    collections::HashMap,
    env::{remove_var, set_var, var},
    sync::Mutex,
    time::{Duration, Instant},
};
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

#[test]
fn build_requests_correctly_splits_seller_and_listing_data() {
    let fake_request = AnalyzeRequest {
        platform: "olx".to_string(),
        seller_id: None,
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("A test listing description".to_string()),
        category: None,
        image_urls: Some(vec!["https://example.com/image1.jpg".to_string()]),
        posted_date: None,
        platform_id: Some("platform_id_123".to_string()),
        seller_name: Some("Test Seller".to_string()),
        seller_handle: Some("test_seller_handle".to_string()),
        seller_phone: Some("03001234567".to_string()),
        seller_profile_url: Some("https://olx.com.pk/profile/test-seller".to_string()),
        seller_join_date: Some("2021".to_string()),
        seller_location: Some("Lahore".to_string()),
        seller_last_active: Some("Today".to_string()),
        domain_check_status: None,
        domain_check_real_name: None,
        domain_check_real_domain: None,
        domain_check_current_domain: None,
        domain_check_current_html: None,
        domain_check_real_html: None,
    };

    let (seller_req, listing_req) = build_requests(&fake_request);

    // Confirm the seller piece got the right seller-specific fields.
    assert_eq!(seller_req.name, Some("Test Seller".to_string()));
    assert_eq!(seller_req.handle, Some("test_seller_handle".to_string()));
    assert_eq!(seller_req.location, Some("Lahore".to_string()));

    // Confirm the listing piece got the right listing-specific fields.
    assert_eq!(listing_req.title, Some("Test Listing".to_string()));
    assert_eq!(listing_req.price, Some(50000));
    assert_eq!(
        listing_req.listing_url,
        "https://olx.com.pk/item/test-listing"
    );

    // Confirm the one shared field (platform) correctly landed in BOTH pieces.
    assert_eq!(seller_req.platform, "olx");
    assert_eq!(listing_req.platform, "olx");
}

#[tokio::test]
async fn resolve_new_seller() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "new_seller_test_001".to_string();

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let seller_request = SellersRequest {
        platform: platform.clone(),
        platform_id: Some(platform_id.clone()),
        name: Some("Test name".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test-seller".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let result = resolve_seller(&pool, &seller_request, &platform, &platform_id)
        .await
        .expect("expected to resolve the seller");

    assert_eq!(result.seller.verification, SellerVerification::Unknown);
    assert_eq!(result.fraud_count, 0);

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");
}

#[tokio::test]
async fn resolve_existing_seller_have_no_reports() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "sellers_have_no_report_test_001".to_string();

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let seller_request = SellersRequest {
        platform: platform.clone(),
        platform_id: Some(platform_id.clone()),
        name: Some("Test name".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test-seller".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected to pre-create the seller");

    let result = resolve_seller(&pool, &seller_request, &platform, &platform_id)
        .await
        .expect("expected to resolve the seller");

    assert_eq!(result.seller.verification, SellerVerification::Unknown);
    assert_eq!(result.fraud_count, 0);

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");
}

#[tokio::test]
async fn resolve_existing_seller_with_real_fraud_report() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "seller_have_reports_test_001".to_string();

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let seller_request = SellersRequest {
        platform: platform.clone(),
        platform_id: Some(platform_id.clone()),
        name: Some("Test name".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test-seller".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected to pre-create the seller");

    query("INSERT INTO fraud_reports (seller_id, platform, platform_id, report_type, description) VALUES ($1, $2, $3, $4::report_types, $5)")
        .bind(seller.id)
        .bind(&platform)
        .bind(&platform_id)
        .bind("scam")
        .bind("Test fraud report for scenario 3")
        .execute(&pool)
        .await
        .expect("expected to create a real fraud report");

    let result = resolve_seller(&pool, &seller_request, &platform, &platform_id)
        .await
        .expect("expected to resolve the seller");

    assert_eq!(result.seller.verification, SellerVerification::Reported);
    assert_eq!(result.fraud_count, 1);

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected final cleanup to succeed");
}

#[tokio::test]
async fn find_seller_not_exists() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "find_seller_not_exists".to_string();

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let result = find_seller(&pool, &platform, &platform_id)
        .await
        .expect("expected the query itself to succeed");

    assert!(result.is_none(), "expected no seller to be found");
}

#[tokio::test]
async fn zero_fraud_reports() {
    let pool = test_pool().await;
    let seller_id = Uuid::new_v4();

    let count = count_fraud_reports(&pool, seller_id)
        .await
        .expect("expected to count the fraud reports");

    assert_eq!(count, 0);
}

#[tokio::test]
async fn having_fraud_reports() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "sellers_have_reports_test_001".to_string();

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");

    let seller_request = SellersRequest {
        platform: platform.clone(),
        platform_id: Some(platform_id.clone()),
        name: Some("Test name".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test-seller".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected to create the seller");

    for _ in 0..2 {
        query("INSERT INTO fraud_reports (seller_id, platform, platform_id, report_type, description) VALUES ($1, $2, $3, $4::report_types, $5)")
            .bind(seller.id)
            .bind(&platform)
            .bind(&platform_id)
            .bind("scam")
            .bind("Test fraud report")
            .execute(&pool)
            .await
            .expect("expected to create a real fraud report");
    }

    let count = count_fraud_reports(&pool, seller.id)
        .await
        .expect("expected to count the fraud reports");

    assert_eq!(count, 2);

    query("DELETE FROM sellers WHERE platform = $1 AND platform_id = $2")
        .bind(&platform)
        .bind(&platform_id)
        .execute(&pool)
        .await
        .expect("expected cleanup to succeed");
}

#[test]
fn build_network_first_summary() {
    let fraud_count = 0;

    let result = build_network_summary(fraud_count);

    assert_eq!(
        result,
        "Clean record on Safely network. No fraud reports found.".to_string()
    );
}

#[test]
fn build_network_second_summary() {
    let fraud_count = 1;

    let result = build_network_summary(fraud_count);

    assert_eq!(
        result,
        "1 fraud report found on Safely network. Proceed with caution.".to_string(),
    );
}

#[test]
fn build_network_third_summary() {
    let fraud_count = 2;
    let result = build_network_summary(fraud_count);
    assert_eq!(
        result,
        "2 fraud reports found on Safely network. High risk seller."
    );
}

#[tokio::test]
#[serial]
async fn claude_analysis_pass() {
    dotenvy::dotenv().ok();

    let listing = Listings {
        id: Uuid::now_v7(),
        seller_id: None,
        platform: "olx".to_string(),
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("iPhone 13 Pro Max - Excellent Condition".to_string()),
        price: Some(150000),
        description: Some("Selling my iPhone 13 Pro Max, barely used, no scratches.".to_string()),
        category: Some(ListingCategory::MobilePhones),
        image_urls: Some(vec![
            "https://example.com/image1.jpg".to_string(),
            "https://example.com/image2.jpg".to_string(),
        ]),
        posted_date: Some(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()),
        first_seen_at: Utc::now(),
        last_analyzed_at: None,
        updated_at: Utc::now(),
    };

    let seller = Sellers {
        id: Uuid::now_v7(),
        platform: "olx".to_string(),
        platform_id: "seller_test_001".to_string(),
        name: Some("Ahmed Khan".to_string()),
        handle: Some("ahmed_khan_deals".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/ahmed-khan".to_string()),
        join_date: Some(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap()),
        verification: SellerVerification::Unknown,
        location: Some("Lahore".to_string()),
        last_active_text: Some("Today".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = run_claude_analysis(&listing, &seller)
        .await
        .expect("expected to run the claude analysis");

    assert!(
        !result.overall_risk_notes.is_empty(),
        "expected Claude to return real risk notes, not an empty response"
    );
}

#[tokio::test]
#[serial]
async fn claude_analysis_with_missing_fields_uses_defaults() {
    dotenvy::dotenv().ok();

    let listing = Listings {
        id: Uuid::now_v7(),
        seller_id: None,
        platform: "olx".to_string(),
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: None,
        price: None,
        description: Some("Selling my iPhone 13 Pro Max, barely used, no scratches.".to_string()),
        category: Some(ListingCategory::MobilePhones),
        image_urls: None,
        posted_date: Some(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()),
        first_seen_at: Utc::now(),
        last_analyzed_at: None,
        updated_at: Utc::now(),
    };

    let seller = Sellers {
        id: Uuid::now_v7(),
        platform: "olx".to_string(),
        platform_id: "seller_test_001".to_string(),
        name: Some("Ahmed Khan".to_string()),
        handle: Some("ahmed_khan_deals".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/ahmed-khan".to_string()),
        join_date: None,
        verification: SellerVerification::Unknown,
        location: Some("Lahore".to_string()),
        last_active_text: Some("Today".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = run_claude_analysis(&listing, &seller)
        .await
        .expect("expected the analysis to succeed even with missing fields");

    assert!(
        !result.overall_risk_notes.is_empty(),
        "expected Claude to still return real risk notes despite the missing fields"
    );
}

#[tokio::test]
#[serial]
async fn claude_analysis_failure() {
    dotenvy::dotenv().ok();

    let original_key = var("ANTHROPIC_API_KEY").ok();
    unsafe {
        remove_var("ANTHROPIC_API_KEY");
    }

    let listing = Listings {
        id: Uuid::now_v7(),
        seller_id: None,
        platform: "olx".to_string(),
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("iPhone 13 Pro Max - Excellent Condition".to_string()),
        price: Some(150000),
        description: Some("Selling my iPhone 13 Pro Max, barely used, no scratches.".to_string()),
        category: Some(ListingCategory::MobilePhones),
        image_urls: Some(vec![
            "https://example.com/image1.jpg".to_string(),
            "https://example.com/image2.jpg".to_string(),
        ]),
        posted_date: Some(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap()),
        first_seen_at: Utc::now(),
        last_analyzed_at: None,
        updated_at: Utc::now(),
    };
    let seller = Sellers {
        id: Uuid::now_v7(),
        platform: "olx".to_string(),
        platform_id: "seller_test_001".to_string(),
        name: Some("Ahmed Khan".to_string()),
        handle: Some("ahmed_khan_deals".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/ahmed-khan".to_string()),
        join_date: Some(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap()),
        verification: SellerVerification::Unknown,
        location: Some("Lahore".to_string()),
        last_active_text: Some("Today".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = run_claude_analysis(&listing, &seller).await;

    assert!(
        result.is_err(),
        "expected the analysis to fail when the API key is genuinely missing"
    );

    unsafe {
        if let Some(key) = original_key {
            set_var("ANTHROPIC_API_KEY", key);
        }
    }
}
