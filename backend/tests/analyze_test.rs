mod common;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue},
};
use backend::{
    errors::claude::ClaudeError,
    handlers::analyze::analyze,
    models::{
        analysis::{AnalyzeRequest, RiskLevel, Signal},
        listings::{ListingCategory, Listings, ListingsRequest},
        sellers::{SellerVerification, Sellers, SellersRequest},
    },
    services::{
        analysis::{
            BuildResponseData, CreateAnalysisData, RATE_LIMITS, authorize_request,
            build_all_signals, build_requests, check_rate_limit, create_analysis, resolve_seller,
            run_claude_analysis, save_and_build_response,
        },
        auth::{create_session, find_or_create_user_by_email},
        claude::{
            CallClaudeArguments, ClaudeAnalysis, Finding, ImageAssessment, PriceAssessment,
            call_claude, content,
        },
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::{create_listing, get_monthly_visit_activity},
        scoring::calculate_risk_score,
        sellers::{create_seller, find_seller},
        signals::{build_domain_signal, build_signals},
    },
};
use chrono::{Datelike, Duration as chrono_duration, NaiveDate, Utc};
use common::{cleanup_test_seller, cleanup_test_user, test_pool};
use serde_json::json;
use serial_test::serial;
use sqlx::query;
use std::{
    collections::HashMap,
    env::{remove_var, set_var, var},
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::common::{
    cleanup_test_seller_chain, create_test_user, insert_test_history_chain, set_analysis_created_at,
};

// Analyze Test
#[tokio::test]
async fn analyze_unauthorized_request() {
    let pool = test_pool().await;
    let headers = HeaderMap::new();

    let request = AnalyzeRequest {
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
        platform_id: Some("unauthorized_test_platform_id".to_string()),
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

    let result = analyze(State(pool.clone()), headers, Json(request)).await;

    assert!(
        result.is_err(),
        "expected an unauthenticated request to be rejected"
    );

    let leftover_seller = find_seller(&pool, "olx", "unauthorized_test_platform_id")
        .await
        .expect("expected the query itself to succeed");

    assert!(
        leftover_seller.is_none(),
        "expected NO seller to be created, since authorization should have stopped everything first"
    );
}

#[tokio::test]
async fn analyze_success() {
    let pool = test_pool().await;
    let email = "analyze_success_test@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let real_session_token = create_session(&pool, user.id)
        .await
        .expect("expected to create a real session");

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", real_session_token))
            .expect("expected to insert the header value"),
    );

    let platform = "olx".to_string();
    let platform_id = "analyze_success_platform_id".to_string();
    cleanup_test_seller(&pool, &platform, &platform_id).await;

    let request = AnalyzeRequest {
        platform: platform.clone(),
        seller_id: None,
        listing_url: "https://olx.com.pk/item/analyze-success-test".to_string(),
        listing_id: Some("analyze_success_listing".to_string()),
        title: Some("iPhone 13 Pro Max - Excellent Condition".to_string()),
        price: Some(150000),
        description: Some("Selling my iPhone 13 Pro Max, barely used.".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
        platform_id: Some(platform_id.clone()),
        seller_name: Some("Ahmed Khan".to_string()),
        seller_handle: Some("ahmed_khan_deals".to_string()),
        seller_phone: Some("03001234567".to_string()),
        seller_profile_url: Some("https://olx.com.pk/profile/ahmed-khan".to_string()),
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

    let result = analyze(State(pool.clone()), headers, Json(request))
        .await
        .expect("expected the full analyze flow to succeed");

    assert!(
        result.risk_score >= 0 && result.risk_score <= 100,
        "expected a genuine risk score between 0 and 100, got: {}",
        result.risk_score
    );
    assert!(
        !result.signals.is_empty(),
        "expected at least one real signal to be present"
    );
    assert_eq!(result.fraud_report_count, 0);

    query("DELETE FROM analysis WHERE user_id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("expected analysis cleanup to succeed");

    query("DELETE FROM listings WHERE platform = $1 AND listing_id = $2")
        .bind(&platform)
        .bind("analyze_success_listing")
        .execute(&pool)
        .await
        .expect("expected listing cleanup to succeed");

    cleanup_test_seller(&pool, &platform, &platform_id).await;
    cleanup_test_user(&pool, email).await;
}

// Authorize Request Test
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

// Check Rate Limit Test
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

// Build Requests Test
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

    assert_eq!(seller_req.name, Some("Test Seller".to_string()));
    assert_eq!(seller_req.handle, Some("test_seller_handle".to_string()));
    assert_eq!(seller_req.location, Some("Lahore".to_string()));
    assert_eq!(listing_req.title, Some("Test Listing".to_string()));
    assert_eq!(listing_req.price, Some(50000));
    assert_eq!(
        listing_req.listing_url,
        "https://olx.com.pk/item/test-listing"
    );

    assert_eq!(seller_req.platform, "olx");
    assert_eq!(listing_req.platform, "olx");
}

#[test]
fn build_requests_none_values_stay_none() {
    let fake_request = AnalyzeRequest {
        platform: "olx".to_string(),
        seller_id: None,
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: None,
        title: None,
        price: None,
        description: None,
        category: None,
        image_urls: None,
        posted_date: None,
        platform_id: None,
        seller_name: None,
        seller_handle: None,
        seller_phone: None,
        seller_profile_url: None,
        seller_join_date: None,
        seller_location: None,
        seller_last_active: None,
        domain_check_status: None,
        domain_check_real_name: None,
        domain_check_real_domain: None,
        domain_check_current_domain: None,
        domain_check_current_html: None,
        domain_check_real_html: None,
    };

    let (seller_req, listing_req) = build_requests(&fake_request);

    assert_eq!(seller_req.name, None);
    assert_eq!(seller_req.handle, None);
    assert_eq!(seller_req.location, None);
    assert_eq!(listing_req.title, None);
    assert_eq!(listing_req.price, None);
    assert_eq!(listing_req.category, None);
    assert_eq!(listing_req.image_urls, None);
    assert_eq!(listing_req.posted_date, None);
}

#[test]
fn build_requests_seller_id_lands_only_on_listing_request() {
    let fake_seller_id = Uuid::new_v4();
    let fake_request = AnalyzeRequest {
        platform: "olx".to_string(),
        seller_id: Some(fake_seller_id),
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("A test listing description".to_string()),
        category: None,
        image_urls: None,
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

    let (_seller_req, listing_req) = build_requests(&fake_request);

    assert_eq!(
        listing_req.seller_id,
        Some(fake_seller_id),
        "expected seller_id to correctly land on listing_req specifically"
    );
}

// Resolve Seller Tests
#[tokio::test]
async fn resolve_new_seller() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "new_seller_test_001".to_string();

    cleanup_test_seller(&pool, &platform, &platform_id).await;

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

    cleanup_test_seller(&pool, &platform, &platform_id).await;
}

#[tokio::test]
async fn resolve_existing_seller_have_no_reports() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "sellers_have_no_report_test_001".to_string();
    cleanup_test_seller(&pool, &platform, &platform_id).await;

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

    cleanup_test_seller(&pool, &platform, &platform_id).await;
}

#[tokio::test]
async fn resolve_existing_seller_with_real_fraud_report() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "seller_have_reports_test_001".to_string();

    cleanup_test_seller(&pool, &platform, &platform_id).await;

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

    cleanup_test_seller(&pool, &platform, &platform_id).await;
}

// Find Seller Tests
#[tokio::test]
async fn find_seller_exists() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "find_seller_exists_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Findable Seller".to_string()),
        handle: Some("findable_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/findable".to_string()),
        join_date: None,
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };
    let created_seller = create_seller(&pool, &request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let result = find_seller(&pool, platform, platform_id)
        .await
        .expect("expected the query to succeed");

    let found_seller = result.expect("expected a real seller to be found");
    assert_eq!(found_seller.id, created_seller.id);
    assert_eq!(found_seller.name, Some("Findable Seller".to_string()));

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn find_seller_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let result = find_seller(&pool, "olx", "doesnt_matter").await;
    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

#[tokio::test]
async fn find_seller_not_exists() {
    let pool = test_pool().await;
    let platform = "olx".to_string();
    let platform_id = "find_seller_not_exists".to_string();
    cleanup_test_seller(&pool, &platform, &platform_id).await;

    let result = find_seller(&pool, &platform, &platform_id)
        .await
        .expect("expected the query itself to succeed");

    assert!(result.is_none(), "expected no seller to be found");
}

// Count Fraud Reports Test
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
    cleanup_test_seller(&pool, &platform, &platform_id).await;

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

    cleanup_test_seller(&pool, &platform, &platform_id).await;
}

// Create Seller Tests
#[tokio::test]
async fn create_seller_creates_brand_new() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_new_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Brand New Seller".to_string()),
        handle: Some("new_seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/new-seller".to_string()),
        join_date: None,
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &request, SellerVerification::Unknown)
        .await
        .expect("expected a brand-new seller to be created");

    assert_eq!(seller.platform, platform);
    assert_eq!(seller.platform_id, platform_id);
    assert_eq!(seller.name, Some("Brand New Seller".to_string()));
    assert_eq!(seller.handle, Some("new_seller_handle".to_string()));
    assert_eq!(seller.location, Some("Lahore".to_string()));
    assert_eq!(seller.verification, SellerVerification::Unknown);

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_updates_and_fills_in_new_data() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_fill_in_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let initial_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Seller Name".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: None,
        location: None,
        last_active: Some("Today".to_string()),
    };

    let first_result = create_seller(&pool, &initial_request, SellerVerification::Unknown)
        .await
        .expect("expected the initial creation to succeed");

    assert!(
        first_result.location.is_none(),
        "expected location to start as None"
    );

    let updated_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Seller Name".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: None,
        location: Some("Karachi".to_string()),
        last_active: Some("Today".to_string()),
    };

    let updated_result = create_seller(&pool, &updated_request, SellerVerification::Unknown)
        .await
        .expect("expected the update to succeed");

    assert_eq!(
        updated_result.id, first_result.id,
        "expected the SAME seller row to be updated, not a new one created"
    );
    assert_eq!(
        updated_result.location,
        Some("Karachi".to_string()),
        "expected the previously-missing location to now be filled in"
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_never_overwrites_good_data_with_blanks() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_preserve_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let initial_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Genuinely Real Seller Name".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: None,
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };
    let first_result = create_seller(&pool, &initial_request, SellerVerification::Unknown)
        .await
        .expect("expected the initial creation to succeed");

    assert_eq!(
        first_result.name,
        Some("Genuinely Real Seller Name".to_string())
    );

    let blank_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: None,
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: None,
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let second_result = create_seller(&pool, &blank_request, SellerVerification::Unknown)
        .await
        .expect("expected the second call to succeed");

    assert_eq!(
        second_result.id, first_result.id,
        "expected the SAME seller row"
    );
    assert_eq!(
        second_result.name,
        Some("Genuinely Real Seller Name".to_string()),
        "expected the ORIGINAL, real name to be preserved, NOT wiped out by the blank"
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_verification_always_updates() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_verification_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Verification Test Seller".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: None,
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let first_result = create_seller(&pool, &request, SellerVerification::Unknown)
        .await
        .expect("expected the initial creation to succeed");
    assert_eq!(first_result.verification, SellerVerification::Unknown);

    let second_result = create_seller(&pool, &request, SellerVerification::Reported)
        .await
        .expect("expected the second call to succeed");

    assert_eq!(
        second_result.id, first_result.id,
        "expected the SAME seller row"
    );
    assert_eq!(
        second_result.verification,
        SellerVerification::Reported,
        "expected verification to be UNCONDITIONALLY updated, unlike other fields"
    );
    assert_eq!(
        second_result.name,
        Some("Verification Test Seller".to_string()),
        "expected name to remain correctly preserved, confirming other fields aren't affected"
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_join_date_parses_successfully() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_join_date_success_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Join Date Test Seller".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: Some("Member since 2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created successfully");

    assert_eq!(
        seller.join_date,
        NaiveDate::from_ymd_opt(2021, 1, 1),
        "expected the year 2021 to be genuinely extracted and stored as a real date"
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_join_date_malformed_becomes_none() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_seller_join_date_malformed_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Malformed Join Date Seller".to_string()),
        handle: Some("seller_handle".to_string()),
        phone: Some("03001234567".to_string()),
        profile_url: Some("https://olx.com.pk/profile/seller".to_string()),
        join_date: Some("Member since forever".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created successfully, even with malformed join_date");

    assert!(
        seller.join_date.is_none(),
        "expected malformed join_date text to quietly become None, not cause an error"
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_seller_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let request = SellersRequest {
        platform: "olx".to_string(),
        platform_id: Some("create_seller_db_error_001".to_string()),
        name: Some("Doesn't Matter".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };

    let result = create_seller(&pool, &request, SellerVerification::Unknown).await;
    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

// Build Network Summary Test
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

// Create Listing Test
#[tokio::test]
async fn create_listing_creates_brand_new() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_listing_new_seller_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Listing Test Seller".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let listing_url = "https://olx.com.pk/item/create-listing-new-001";
    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    let listing_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_new_001".to_string()),
        title: Some("Brand New Listing".to_string()),
        price: Some(75000),
        description: Some("A genuinely new listing".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let listing = create_listing(&pool, &listing_request, seller.id)
        .await
        .expect("expected a brand-new listing to be created");

    assert_eq!(listing.listing_url, listing_url);
    assert_eq!(listing.title, Some("Brand New Listing".to_string()));
    assert_eq!(listing.price, Some(75000));
    assert_eq!(listing.seller_id, Some(seller.id));
    assert!(
        listing.last_analyzed_at.is_none(),
        "expected last_analyzed_at to start as None on a brand-new listing"
    );

    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_listing_updates_and_fills_in_new_data() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_listing_fill_in_seller_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Fill In Test Seller".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let listing_url = "https://olx.com.pk/item/create-listing-fill-in-001";
    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    let initial_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_fill_in_001".to_string()),
        title: Some("Listing Title".to_string()),
        price: Some(50000),
        description: None,
        category: None,
        image_urls: None,
        posted_date: None,
    };
    let first_result = create_listing(&pool, &initial_request, seller.id)
        .await
        .expect("expected the initial creation to succeed");
    assert!(
        first_result.description.is_none(),
        "expected description to start as None"
    );

    let updated_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_fill_in_001".to_string()),
        title: Some("Listing Title".to_string()),
        price: Some(50000),
        description: Some("Now genuinely described".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let updated_result = create_listing(&pool, &updated_request, seller.id)
        .await
        .expect("expected the update to succeed");

    assert_eq!(
        updated_result.id, first_result.id,
        "expected the SAME listing row to be updated, not a new one created"
    );
    assert_eq!(
        updated_result.description,
        Some("Now genuinely described".to_string()),
        "expected the previously-missing description to now be filled in"
    );

    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_listing_never_overwrites_good_data_with_blanks() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_listing_preserve_seller_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Preserve Test Seller".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let listing_url = "https://olx.com.pk/item/create-listing-preserve-001";
    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    let initial_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_preserve_001".to_string()),
        title: Some("Genuinely Real Listing Title".to_string()),
        price: Some(50000),
        description: Some("Real description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };
    let first_result = create_listing(&pool, &initial_request, seller.id)
        .await
        .expect("expected the initial creation to succeed");

    assert_eq!(
        first_result.title,
        Some("Genuinely Real Listing Title".to_string())
    );

    let blank_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_preserve_001".to_string()),
        title: None,
        price: None,
        description: Some("Real description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let second_result = create_listing(&pool, &blank_request, seller.id)
        .await
        .expect("expected the second call to succeed");

    assert_eq!(
        second_result.id, first_result.id,
        "expected the SAME listing row"
    );
    assert_eq!(
        second_result.title,
        Some("Genuinely Real Listing Title".to_string()),
        "expected the ORIGINAL, real title to be preserved, NOT wiped out by the blank"
    );
    assert_eq!(
        second_result.price,
        Some(50000),
        "expected the ORIGINAL, real price to be preserved, NOT wiped out by the blank"
    );

    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_listing_last_analyzed_at_set_on_update() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "create_listing_last_analyzed_seller_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("Last Analyzed Test Seller".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };
    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let listing_url = "https://olx.com.pk/item/create-listing-last-analyzed-001";
    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    let request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.to_string(),
        listing_url: listing_url.to_string(),
        listing_id: Some("create_listing_last_analyzed_001".to_string()),
        title: Some("Last Analyzed Test Listing".to_string()),
        price: Some(50000),
        description: Some("Test description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let first_result = create_listing(&pool, &request, seller.id)
        .await
        .expect("expected the initial creation to succeed");
    assert!(
        first_result.last_analyzed_at.is_none(),
        "expected last_analyzed_at to start as None on a brand-new listing"
    );

    let second_result = create_listing(&pool, &request, seller.id)
        .await
        .expect("expected the second call to succeed");

    assert_eq!(
        second_result.id, first_result.id,
        "expected the SAME listing row"
    );
    assert!(
        second_result.last_analyzed_at.is_some(),
        "expected last_analyzed_at to genuinely be set now, after a real update"
    );

    let recorded_time = second_result.last_analyzed_at.unwrap();
    assert!(
        Utc::now() - recorded_time < chrono_duration::minutes(1),
        "expected the recorded time to be genuinely recent"
    );

    query("DELETE FROM listings WHERE listing_url = $1")
        .bind(listing_url)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn create_listing_database_error() {
    let pool = test_pool().await;
    pool.close().await;

    let fake_seller_id = Uuid::new_v4();
    let request = ListingsRequest {
        seller_id: Some(fake_seller_id),
        platform: "olx".to_string(),
        listing_url: "https://olx.com.pk/item/db-error-test".to_string(),
        listing_id: Some("db_error_test_001".to_string()),
        title: Some("Doesn't Matter".to_string()),
        price: None,
        description: None,
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let result = create_listing(&pool, &request, fake_seller_id).await;
    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

// Run Claude Analysis Test
#[tokio::test]
#[serial]
async fn claude_analysis_success() {
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

// Call Claude Test

#[tokio::test]
#[serial]
async fn call_claude_missing_api_key() {
    dotenvy::dotenv().ok();
    let original_key = var("ANTHROPIC_API_KEY").ok();
    unsafe {
        remove_var("ANTHROPIC_API_KEY");
    }

    let image_urls: Vec<String> = vec![];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Test Seller",
        seller_account_age: "3 years",
        title: "Test Listing",
        price: 50000,
        description: "A genuine test description",
        image_urls: &image_urls,
    };

    let result = call_claude(args).await;

    match result {
        Err(ClaudeError::MissingApiKey) => {}
        Err(other) => panic!("expected MissingApiKey, got a different error: {:?}", other),
        Ok(_) => panic!("expected the call to fail without a real API key, but it succeeded"),
    }

    unsafe {
        if let Some(key) = original_key {
            set_var("ANTHROPIC_API_KEY", key);
        }
    }
}

#[tokio::test]
#[serial]
async fn call_claude_success_complete_data() {
    dotenvy::dotenv().ok();

    let image_urls: Vec<String> = vec![];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Ahmed Khan",
        seller_account_age: "3 years",
        title: "iPhone 13 Pro Max - Excellent Condition",
        price: 150000,
        description: "Selling my iPhone 13 Pro Max, barely used, no scratches.",
        image_urls: &image_urls,
    };

    let result = call_claude(args)
        .await
        .expect("expected the call to genuinely succeed");

    assert!(
        !result.overall_risk_notes.is_empty(),
        "expected Claude to return real, non-empty risk notes"
    );
}

#[tokio::test]
#[serial]
async fn call_claude_success_with_minimal_data() {
    dotenvy::dotenv().ok();

    let image_urls: Vec<String> = vec![];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Unknown",
        seller_account_age: "Unknown",
        title: "Untitled",
        price: 0,
        description: "No Description",
        image_urls: &image_urls,
    };

    let result = call_claude(args)
        .await
        .expect("expected the call to still succeed, even with minimal/default data");

    assert!(
        !result.overall_risk_notes.is_empty(),
        "expected Claude to still return real risk notes despite the sparse input"
    );
}

#[tokio::test]
#[serial]
async fn call_claude_request_failed() {
    dotenvy::dotenv().ok();

    // Genuinely unreachable
    let response = reqwest::Client::new()
        .post("http://this-domain-genuinely-does-not-exist-12345.invalid")
        .send()
        .await;
    assert!(
        response.is_err(),
        "sanity check: this host should genuinely be unreachable"
    );
}

// Content Test
#[test]
fn content_includes_all_real_values() {
    let image_urls: Vec<String> = vec![];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Ahmed Khan",
        seller_account_age: "3 years",
        title: "iPhone 13 Pro Max",
        price: 150000,
        description: "Barely used, no scratches",
        image_urls: &image_urls,
    };

    let result = content(&args);

    assert!(
        result.contains("olx"),
        "expected the platform to appear in the prompt"
    );
    assert!(
        result.contains("Ahmed Khan"),
        "expected the seller name to appear"
    );
    assert!(
        result.contains("3 years"),
        "expected the account age to appear"
    );
    assert!(
        result.contains("iPhone 13 Pro Max"),
        "expected the title to appear"
    );
    assert!(result.contains("150000"), "expected the price to appear");
    assert!(
        result.contains("Barely used, no scratches"),
        "expected the description to appear"
    );
}

#[test]
fn content_never_includes_image_urls() {
    let image_urls: Vec<String> =
        vec!["https://example.com/genuinely-distinctive-image-url-marker.jpg".to_string()];
    let args = CallClaudeArguments {
        platform: "olx",
        seller_name: "Test Seller",
        seller_account_age: "1 year",
        title: "Test Listing",
        price: 1000,
        description: "Test description",
        image_urls: &image_urls,
    };

    let result = content(&args);

    assert!(
        !result.contains("genuinely-distinctive-image-url-marker"),
        "expected image_urls to be genuinely absent from the prompt, since this field is deliberately unused"
    );
}

// Build All Signals Test
#[test]
fn build_all_signals_without_domain_check() {
    let finding = Finding {
        found: true,
        evidence: "evidence".to_string(),
    };

    let image_assessment = ImageAssessment {
        verdict: "original".to_string(),
        reasoning: "reasoning".to_string(),
    };

    let price_assessment = PriceAssessment {
        verdict: "normal".to_string(),
        reasoning: "reasoning".to_string(),
    };

    let claude_analysis = ClaudeAnalysis {
        urgency_language: finding.clone(),
        advance_payment_request: finding.clone(),
        duplicate_listing: finding.clone(),
        image_authenticity: image_assessment,
        fraud_pattern_match: finding.clone(),
        contact_info_in_listing: finding.clone(),
        price_assessment: price_assessment,
        overall_risk_notes: "risk notes".to_string(),
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

    let analyze_request = AnalyzeRequest {
        platform: "olx".to_string(),
        seller_id: None,
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("Test description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
        platform_id: Some("platform_id_123".to_string()),
        seller_name: Some("Test Seller".to_string()),
        seller_handle: Some("test_handle".to_string()),
        seller_phone: Some("123456789".to_string()),
        seller_profile_url: Some("https://olx.com.pk/profile/test".to_string()),
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

    let signals_without_domain = build_signals(&claude_analysis, &seller);
    let all_signals = build_all_signals(&claude_analysis, &seller, &analyze_request);

    assert_eq!(all_signals.len(), signals_without_domain.len());
}

#[test]
fn build_all_signals_with_domain_check() {
    let finding = Finding {
        found: true,
        evidence: "evidence".to_string(),
    };

    let image_assessment = ImageAssessment {
        verdict: "original".to_string(),
        reasoning: "reasoning".to_string(),
    };

    let price_assessment = PriceAssessment {
        verdict: "normal".to_string(),
        reasoning: "reasoning".to_string(),
    };

    let claude_analysis = ClaudeAnalysis {
        urgency_language: finding.clone(),
        advance_payment_request: finding.clone(),
        duplicate_listing: finding.clone(),
        image_authenticity: image_assessment,
        fraud_pattern_match: finding.clone(),
        contact_info_in_listing: finding.clone(),
        price_assessment: price_assessment,
        overall_risk_notes: "risk notes".to_string(),
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

    let analyze_request = AnalyzeRequest {
        platform: "olx".to_string(),
        seller_id: None,
        listing_url: "https://olx.com.pk/item/test-listing".to_string(),
        listing_id: Some("12345".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("Test description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
        platform_id: Some("platform_id_123".to_string()),
        seller_name: Some("Test Seller".to_string()),
        seller_handle: Some("test_handle".to_string()),
        seller_phone: Some("123456789".to_string()),
        seller_profile_url: Some("https://olx.com.pk/profile/test".to_string()),
        seller_join_date: Some("2021".to_string()),
        seller_location: Some("Lahore".to_string()),
        seller_last_active: Some("Today".to_string()),
        domain_check_status: Some("suspicious".to_string()),
        domain_check_real_name: None,
        domain_check_real_domain: None,
        domain_check_current_domain: None,
        domain_check_current_html: None,
        domain_check_real_html: None,
    };

    let signals_without_domain = build_signals(&claude_analysis, &seller);
    let all_signals = build_all_signals(&claude_analysis, &seller, &analyze_request);

    assert_eq!(all_signals.len(), signals_without_domain.len() + 1);
    assert_eq!(all_signals[0].label, "Domain check");
}

// Build Domain Signal Test
#[test]
fn build_domain_signal_with_legitimate_status() {
    let status = Some("legitimate");
    let real_name = None;
    let real_domain = None;
    let current_domain = None;
    let current_domain_html = None;
    let real_domain_html = None;

    let result = build_domain_signal(
        status,
        real_name,
        real_domain,
        current_domain,
        current_domain_html,
        real_domain_html,
    );

    let signal = result.expect("expected a signal for a legitimate domain");

    assert_eq!(signal.label, "Domain check");
    assert_eq!(signal.value, "Verified");
    assert_eq!(signal.signal_type, "good");
    assert_eq!(
        signal.sub,
        "This matches the marketplace's real, verified domain."
    );
}

#[test]
fn build_domain_signal_with_suspicious_status() {
    let status = Some("suspicious");
    let real_name = None;
    let real_domain = None;
    let current_domain = None;
    let current_domain_html = None;
    let real_domain_html = None;

    let result = build_domain_signal(
        status,
        real_name,
        real_domain,
        current_domain,
        current_domain_html,
        real_domain_html,
    );

    let signal = result.expect("expected a signal for a suspicious domain");

    assert_eq!(signal.label, "Domain check");
    assert_eq!(signal.value, "Suspicious");
    assert_eq!(signal.signal_type, "bad");
    assert_eq!(
        signal.sub,
        "This does not match the marketplace's real domain (unknown). You're currently on an unrecognized domain instead."
    );
}

#[test]
fn build_domain_signal_with_something_else_status() {
    let status = Some("something_else");
    let real_name = None;
    let real_domain = None;
    let current_domain = None;
    let current_domain_html = None;
    let real_domain_html = None;

    let result = build_domain_signal(
        status,
        real_name,
        real_domain,
        current_domain,
        current_domain_html,
        real_domain_html,
    );

    assert!(
        result.is_none(),
        "expected no signal for an unrecognized status"
    );
}

#[test]
fn build_domain_signal_prefers_highlighted_html_over_plain_text() {
    let status = Some("suspicious");
    let real_name = Some("OLX");
    let real_domain = Some("olx.com.pk");
    let real_domain_html = Some("olx.com.pk");
    let current_domain = Some("0lx.com.pk");
    let current_domain_html = Some("<b>0</b>lx.com.pk");

    let result = build_domain_signal(
        status,
        real_name,
        real_domain,
        current_domain,
        current_domain_html,
        real_domain_html,
    );

    let signal = result.expect("expected a signal for a suspicious domain");

    assert!(
        signal.sub.contains("<b>0</b>lx.com.pk"),
        "expected the highlighted version to be used, got: {}",
        signal.sub
    );

    assert!(
        !signal.sub.contains("0lx.com.pk\""),
        "expected the plain version NOT to be used when the highlighted one was available"
    );
}

// Calculate Risk Score Test
#[test]
fn calculate_risk_score_baseline_zero() {
    let analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(score, 0, "expected a genuinely clean listing to score 0");
}

#[test]
fn calculate_risk_score_urgency_language() {
    let mut analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };
    analysis.urgency_language.found = true;

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 15,
        "expected urgency_language alone to add exactly 15 points"
    );
}

#[test]
fn calculate_risk_score_advance_payment_request() {
    let mut analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };
    analysis.advance_payment_request.found = true;

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 20,
        "expected advance_payment_request alone to add exactly 20 points"
    );
}

#[test]
fn calculate_risk_score_duplicate_listing() {
    let mut analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };
    analysis.duplicate_listing.found = true;

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 15,
        "expected duplicate_listing alone to add exactly 15 points"
    );
}

#[test]
fn calculate_risk_score_fraud_pattern_match() {
    let mut analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };
    analysis.fraud_pattern_match.found = true;

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 30,
        "expected fraud_pattern_match alone to add exactly 30 points"
    );
}

#[test]
fn calculate_risk_score_contact_info_in_listing() {
    let mut analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };
    analysis.contact_info_in_listing.found = true;

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 10,
        "expected contact_info_in_listing alone to add exactly 10 points"
    );
}

#[test]
fn calculate_risk_score_price_assessment_not_normal() {
    let analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "suspiciously_low".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 20,
        "expected a non-'normal' price verdict alone to add exactly 20 points"
    );
}

#[test]
fn calculate_risk_score_image_authenticity_not_original() {
    let analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "stolen".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "".to_string(),
    };

    let score = calculate_risk_score(&analysis, 0);

    assert_eq!(
        score, 10,
        "expected a non-'original' image verdict alone to add exactly 10 points"
    );
}

// Save and Build Response Test
#[tokio::test]
async fn save_and_build_response_success() {
    let pool = test_pool().await;

    let email = "save_response_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let seller_request = SellersRequest {
        platform: "olx".to_string(),
        platform_id: Some("save_response_seller_001".to_string()),
        name: Some("Test Seller".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };
    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected to create the seller");

    let listing_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: "olx".to_string(),
        listing_url: "https://olx.com.pk/item/save-response-test".to_string(),
        listing_id: Some("save_response_listing_001".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("Test description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };
    let listing = create_listing(&pool, &listing_request, seller.id)
        .await
        .expect("expected to create the listing");

    let claude_analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "No significant risk detected.".to_string(),
    };

    let signals = vec![Signal {
        label: "Price analysis".to_string(),
        sub: "Price looks normal for this category.".to_string(),
        value: "Normal".to_string(),
        signal_type: "good".to_string(),
    }];

    let data = BuildResponseData {
        pool: &pool,
        listing_id: listing.id,
        risk_score: 10,
        risk_level: RiskLevel::Low,
        signals: signals.clone(),
        claude_analysis,
        user_id: user.id,
        seller,
        fraud_count: 0,
        network_summary: "Clean record on Safely network. No fraud reports found.".to_string(),
    };

    let result = save_and_build_response(data)
        .await
        .expect("expected the response to be built successfully");

    assert_eq!(result.risk_score, 10);
    assert_eq!(result.risk_level, RiskLevel::Low);
    assert_eq!(result.fraud_report_count, 0);
    assert_eq!(result.signals.len(), 1);

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn save_and_build_response_database_failure() {
    let pool = test_pool().await;

    let fake_listing_id = Uuid::new_v4();
    let fake_user_id = Uuid::new_v4();

    let seller = Sellers {
        id: Uuid::now_v7(),
        platform: "olx".to_string(),
        platform_id: "fake_seller_for_failure_test".to_string(),
        name: Some("Test Seller".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test".to_string()),
        join_date: Some(NaiveDate::from_ymd_opt(2021, 1, 1).unwrap()),
        verification: SellerVerification::Unknown,
        location: Some("Lahore".to_string()),
        last_active_text: Some("Today".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let claude_analysis = ClaudeAnalysis {
        urgency_language: Finding {
            found: false,
            evidence: "".to_string(),
        },
        advance_payment_request: Finding {
            found: false,
            evidence: "".to_string(),
        },
        duplicate_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        image_authenticity: ImageAssessment {
            verdict: "original".to_string(),
            reasoning: "".to_string(),
        },
        fraud_pattern_match: Finding {
            found: false,
            evidence: "".to_string(),
        },
        contact_info_in_listing: Finding {
            found: false,
            evidence: "".to_string(),
        },
        price_assessment: PriceAssessment {
            verdict: "normal".to_string(),
            reasoning: "".to_string(),
        },
        overall_risk_notes: "No significant risk detected.".to_string(),
    };

    let signals = vec![Signal {
        label: "Price analysis".to_string(),
        sub: "Price looks normal for this category.".to_string(),
        value: "Normal".to_string(),
        signal_type: "good".to_string(),
    }];

    let data = BuildResponseData {
        pool: &pool,
        listing_id: fake_listing_id,
        risk_score: 10,
        risk_level: RiskLevel::Low,
        signals,
        claude_analysis,
        user_id: fake_user_id,
        seller,
        fraud_count: 0,
        network_summary: "Clean record on Safely network. No fraud reports found.".to_string(),
    };

    let result = save_and_build_response(data).await;
    assert!(
        result.is_err(),
        "expected save_and_build_response to fail when listing_id and user_id don't genuinely exist"
    );
}

// Create Analysis Test
#[tokio::test]
async fn create_analysis_success() {
    let pool = test_pool().await;
    let email = "create_analysis_test@example.com";
    cleanup_test_user(&pool, email).await;

    let (user, _) = find_or_create_user_by_email(&pool, email)
        .await
        .expect("expected to create the user");

    let platform = "olx".to_string();
    let platform_id = "create_analysis_seller_001".to_string();

    cleanup_test_seller(&pool, &platform, &platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.clone(),
        platform_id: Some(platform_id.clone()),
        name: Some("Test Seller".to_string()),
        handle: Some("test_handle".to_string()),
        phone: Some("123456789".to_string()),
        profile_url: Some("https://olx.com.pk/profile/test".to_string()),
        join_date: Some("2021".to_string()),
        location: Some("Lahore".to_string()),
        last_active: Some("Today".to_string()),
    };

    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected to create the seller");

    let listing_request = ListingsRequest {
        seller_id: Some(seller.id),
        platform: platform.clone(),
        listing_url: "https://olx.com.pk/item/create-analysis-test".to_string(),
        listing_id: Some("create_analysis_listing_001".to_string()),
        title: Some("Test Listing".to_string()),
        price: Some(50000),
        description: Some("Test description".to_string()),
        category: None,
        image_urls: None,
        posted_date: None,
    };

    let listing = create_listing(&pool, &listing_request, seller.id)
        .await
        .expect("expected to create the listing");

    let signals_json = json!([
        { "label": "Price analysis", "sub": "Normal", "value": "Normal", "signal_type": "good" }
    ]);

    let data = CreateAnalysisData {
        pool: &pool,
        listing_id: listing.id,
        risk_score: 15,
        risk_level: RiskLevel::Low,
        signals: signals_json.clone(),
        network_summary: "Clean record on Safely network. No fraud reports found.".to_string(),
        claude_raw: String::new(),
        user_id: user.id,
    };

    let analysis = create_analysis(data)
        .await
        .expect("expected the analysis to be created successfully");

    assert_eq!(analysis.listing_id, listing.id);
    assert_eq!(analysis.user_id, user.id);
    assert_eq!(analysis.risk_score, 15);
    assert_eq!(analysis.risk_level, RiskLevel::Low);
    assert_eq!(
        analysis.network_summary,
        Some("Clean record on Safely network. No fraud reports found.".to_string())
    );
    assert_eq!(analysis.signals, signals_json);

    query("DELETE FROM analysis WHERE id = $1")
        .bind(analysis.id)
        .execute(&pool)
        .await
        .expect("expected analysis cleanup to succeed");

    query("DELETE FROM listings WHERE id = $1")
        .bind(listing.id)
        .execute(&pool)
        .await
        .expect("expected listing cleanup to succeed");

    cleanup_test_seller(&pool, &platform, &platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn create_analysis_database_failure() {
    let pool = test_pool().await;

    let fake_listing_id = Uuid::new_v4();
    let fake_user_id = Uuid::new_v4();

    let signals_json = json!([
        { "label": "Price analysis", "sub": "Normal", "value": "Normal", "signal_type": "good" }
    ]);

    let data = CreateAnalysisData {
        pool: &pool,
        listing_id: fake_listing_id,
        risk_score: 15,
        risk_level: RiskLevel::Low,
        signals: signals_json,
        network_summary: "Clean record on Safely network. No fraud reports found.".to_string(),
        claude_raw: String::new(),
        user_id: fake_user_id,
    };

    let result = create_analysis(data).await;

    assert!(
        result.is_err(),
        "expected create_analysis to fail when listing_id and user_id don't genuinely exist"
    );
}

// Get Monthly Visit Activity Test
#[tokio::test]
#[serial]
async fn get_monthly_visit_activity_database_error() {
    let pool = test_pool().await;

    pool.close().await;

    let fake_seller_id = Uuid::new_v4();
    let result = get_monthly_visit_activity(&pool, fake_seller_id).await;

    assert!(
        result.is_err(),
        "expected a genuine database error when the connection pool is closed"
    );
}

#[tokio::test]
async fn get_monthly_visit_activity_no_activity() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "monthly_activity_no_activity_001";
    cleanup_test_seller(&pool, platform, platform_id).await;

    let seller_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        name: Some("No Activity Seller".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };
    let seller = create_seller(&pool, &seller_request, SellerVerification::Unknown)
        .await
        .expect("expected the seller to be created");

    let result = get_monthly_visit_activity(&pool, seller.id)
        .await
        .expect("expected the query to succeed");

    assert_eq!(result.len(), 12, "expected exactly 12 entries");
    assert!(
        result.iter().all(|&count| count == 0),
        "expected all 12 months to be genuinely 0, got: {:?}",
        result
    );

    cleanup_test_seller(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn get_monthly_visit_activity_single_month_correctly_placed() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "monthly_activity_single_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;

    let email = "monthly_activity_single_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (_listing_id, seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        platform,
        platform_id,
        "Single Month Test Listing",
    )
    .await;

    let result = get_monthly_visit_activity(&pool, seller_id)
        .await
        .expect("expected the query to succeed");

    let current_month_index = (Utc::now().month() - 1) as usize;
    assert_eq!(
        result[current_month_index], 1,
        "expected exactly 1 visit in the current month's position"
    );
    let total: i32 = result.iter().sum();
    assert_eq!(
        total, 1,
        "expected exactly one non-zero entry total, across all 12 months"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn get_monthly_visit_activity_multiple_in_same_month() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "monthly_activity_multiple_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;

    let email = "monthly_activity_single_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (listing_id, seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        platform,
        platform_id,
        "Single Month Test Listing",
    )
    .await;

    query(
        "INSERT INTO analysis (id, listing_id, risk_score, risk_level, signals, user_id, created_at)
         VALUES ($1, $2, $3, 'low'::risk_level_type, $4, $5, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(listing_id)
    .bind(15_i16)
    .bind(json!([]))
    .bind(user.id)
    .execute(&pool)
    .await
    .expect("expected to create the second analysis");

    let result = get_monthly_visit_activity(&pool, seller_id)
        .await
        .expect("expected the query to succeed");

    let current_month_index = (Utc::now().month() - 1) as usize;
    assert_eq!(
        result[current_month_index], 2,
        "expected exactly 2 visits genuinely counted in the current month"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn get_monthly_visit_activity_excludes_old_activity() {
    let pool = test_pool().await;
    let platform = "olx";
    let platform_id = "monthly_activity_old_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;

    let email = "monthly_activity_single_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let (listing_id, seller_id) = insert_test_history_chain(
        &pool,
        user.id,
        platform,
        platform_id,
        "Single Month Test Listing",
    )
    .await;

    set_analysis_created_at(&pool, listing_id, Utc::now() - chrono_duration::days(395)).await;
    let result = get_monthly_visit_activity(&pool, seller_id)
        .await
        .expect("expected the query to succeed");

    assert!(
        result.iter().all(|&count| count == 0),
        "expected genuinely old activity to be excluded entirely, got: {:?}",
        result
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
}

#[tokio::test]
async fn get_monthly_visit_activity_isolated_between_sellers() {
    let pool = test_pool().await;
    let email = "monthly_activity_isolation_user@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id_a = "monthly_activity_isolation_a_001";
    let platform_id_b = "monthly_activity_isolation_b_001";
    cleanup_test_seller_chain(&pool, platform, platform_id_a).await;
    cleanup_test_seller_chain(&pool, platform, platform_id_b).await;

    // Seller A has real, current activity.
    let (_listing_a, seller_a_id) = insert_test_history_chain(
        &pool,
        user.id,
        platform,
        platform_id_a,
        "Seller A's Listing",
    )
    .await;

    // Seller B genuinely has NO activity at all.
    let seller_b_request = SellersRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id_b.to_string()),
        name: Some("Seller B - No Activity".to_string()),
        handle: None,
        phone: None,
        profile_url: None,
        join_date: None,
        location: None,
        last_active: None,
    };

    let seller_b = create_seller(&pool, &seller_b_request, SellerVerification::Unknown)
        .await
        .expect("expected seller B to be created");

    let result_a = get_monthly_visit_activity(&pool, seller_a_id)
        .await
        .expect("expected seller A's query to succeed");
    let result_b = get_monthly_visit_activity(&pool, seller_b.id)
        .await
        .expect("expected seller B's query to succeed");

    let total_a: i32 = result_a.iter().sum();
    let total_b: i32 = result_b.iter().sum();

    assert_eq!(
        total_a, 1,
        "expected seller A to show their own real activity"
    );
    assert_eq!(
        total_b, 0,
        "expected seller B to show ZERO activity, genuinely unaffected by seller A's"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id_a).await;
    cleanup_test_seller(&pool, platform, platform_id_b).await;
    cleanup_test_user(&pool, email).await;
}
