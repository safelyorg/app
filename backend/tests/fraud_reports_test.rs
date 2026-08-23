mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_with_reports, cleanup_test_user, create_test_user,
    test_pool,
};
use axum::{Json, extract::State, http::HeaderMap};
use backend::{
    errors::fraud_reports::FraudReportError,
    handlers::fraud_reports::create_fraud_report,
    models::fraud_reports::{FraudReportsRequest, ReportTypes},
};
use serde_json::json;
use sqlx::{query, query_scalar};
use uuid::Uuid;

#[tokio::test]
async fn create_fraud_report_unauthorized() {
    let pool = test_pool().await;

    let headers = HeaderMap::new();

    let request = FraudReportsRequest {
        platform: "olx".to_string(),
        platform_id: Some("doesnt_matter_001".to_string()),
        report_type: ReportTypes::Scam,
        description: Some("test description".to_string()),
        listing_url: Some("https://olx.com/item/doesnt-matter".to_string()),
    };

    let result = create_fraud_report(State(pool), headers, Json(request)).await;

    match result {
        Err(FraudReportError::Unauthorized) => {}
        Err(other) => panic!("expected Unauthorized, got a different error: {:?}", other),
        Ok(_) => panic!("expected an unauthenticated request to be rejected, but it succeeded"),
    }
}

#[tokio::test]
async fn create_fraud_report_not_found() {
    let pool = test_pool().await;
    let email = "create_fraud_report_not_found_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let request = FraudReportsRequest {
        platform: "olx".to_string(),
        platform_id: Some("definitely_does_not_exist_001".to_string()),
        report_type: ReportTypes::Scam,
        description: Some("test description".to_string()),
        listing_url: Some("https://olx.com/item/nonexistent-seller".to_string()),
    };

    let result = create_fraud_report(State(pool.clone()), headers, Json(request)).await;

    match result {
        Err(FraudReportError::NotFound(_)) => {}
        Err(other) => panic!("expected NotFound, got a different error: {:?}", other),
        Ok(_) => panic!("expected reporting a nonexistent seller to fail, but it succeeded"),
    }

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn create_fraud_report_success() {
    let pool = test_pool().await;
    let email = "create_fraud_report_success_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let platform = "olx";
    let platform_id = "create_fraud_report_seller_001";
    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;

    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Report Target Seller")
    .execute(&pool)
    .await
    .expect("expected to create the seller");

    let listing_url = "https://olx.com/item/create-fraud-report-test";
    let request = FraudReportsRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        report_type: ReportTypes::Scam,
        description: Some("Genuine test report".to_string()),
        listing_url: Some(listing_url.to_string()),
    };

    let result = create_fraud_report(State(pool.clone()), headers, Json(request))
        .await
        .expect("expected the report to be created successfully")
        .0;

    assert_eq!(result["success"], json!(true));

    let saved_report_type: Option<ReportTypes> =
        query_scalar("SELECT report_type FROM fraud_reports WHERE seller_id = $1 AND user_id = $2")
            .bind(seller_id)
            .bind(user.id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query to succeed");

    assert_eq!(
        saved_report_type,
        Some(ReportTypes::Scam),
        "expected a real fraud_reports row to genuinely exist"
    );

    let saved_verification: String =
        query_scalar("SELECT verification::text FROM sellers WHERE id = $1")
            .bind(seller_id)
            .fetch_one(&pool)
            .await
            .expect("expected the query to succeed");

    assert_eq!(
        saved_verification, "reported",
        "expected the seller's verification to genuinely become 'reported'"
    );

    query("DELETE FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .ok();

    cleanup_test_seller_with_reports(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn create_fraud_report_database_error() {
    let pool = test_pool().await;
    let email = "create_fraud_report_db_error_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    cleanup_test_user(&pool, email).await;
    pool.close().await;

    let request = FraudReportsRequest {
        platform: "olx".to_string(),
        platform_id: Some("doesnt_matter_001".to_string()),
        report_type: ReportTypes::Scam,
        description: Some("test description".to_string()),
        listing_url: Some("https://olx.com/item/doesnt-matter".to_string()),
    };

    let result = create_fraud_report(State(pool), headers, Json(request)).await;

    match result {
        Err(FraudReportError::InternalError(_)) => {}
        Err(other) => panic!("expected InternalError, got a different error: {:?}", other),
        Ok(_) => panic!("expected a genuine database error, but the request succeeded"),
    }
}
