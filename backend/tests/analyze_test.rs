mod common;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue},
};
use backend::{
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
        claude::{ClaudeAnalysis, Finding, ImageAssessment, PriceAssessment},
        fraud_reports::{build_network_summary, count_fraud_reports},
        listings::{create_listing, get_monthly_visit_activity},
        sellers::{create_seller, find_seller},
        signals::{build_domain_signal, build_signals},
    },
};
use chrono::{NaiveDate, Utc};
use common::{cleanup_test_seller, cleanup_test_user, test_pool};
use serial_test::serial;
use sqlx::query;
use std::{
    collections::HashMap,
    env::{remove_var, set_var, var},
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

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
        // The 6 fields that actually matter for this test - genuinely
        // empty, meaning "no domain check happened at all".
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

    let signals_json = serde_json::json!([
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

    let signals_json = serde_json::json!([
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
