mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_seller_with_reports,
    cleanup_test_user, create_test_user, test_pool,
};
use axum::{Json, extract::State, http::HeaderMap};
use backend::{
    errors::fraud_reports::FraudReportError,
    handlers::fraud_reports::{create_fraud_report, risk_level_for},
    models::analysis::Signal,
    models::fraud_reports::{FraudReportsRequest, ReportTypes},
    services::{
        analysis::b2b_score_from_signals, fraud_reports::already_reported,
        network_memory::fraud_report_history,
    },
};
use serde_json::{Value, json, to_value};
use sqlx::{Pool, Postgres, query, query_as, query_scalar};
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

    let result = create_fraud_report(State(pool.clone()), headers.clone(), Json(request))
        .await
        .expect("expected the report to be created successfully")
        .0;

    assert_eq!(result["success"], json!(true));
    // This person has no saved scan of the seller, so there is no new
    // score to send back.
    assert_eq!(result["risk_score"], json!(null));

    // One report per person per seller: the same person reporting the
    // same seller again is refused, and no second row is saved.
    let again = FraudReportsRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        report_type: ReportTypes::Scam,
        description: Some("Second report from the same person".to_string()),
        listing_url: Some(listing_url.to_string()),
    };
    match create_fraud_report(State(pool.clone()), headers, Json(again)).await {
        Err(FraudReportError::AlreadyReported) => {}
        Err(other) => panic!(
            "expected AlreadyReported, got a different error: {:?}",
            other
        ),
        Ok(_) => panic!("expected a second report from the same person to be refused"),
    }
    let rows: i64 =
        query_scalar("SELECT COUNT(*) FROM fraud_reports WHERE seller_id = $1 AND user_id = $2")
            .bind(seller_id)
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("expected the query to succeed");
    assert_eq!(rows, 1, "expected only the first report to be saved");

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

// ---- Helpers for the report-score tests ---------------------------

/// A seller and one of its listings. Returns (seller_id, listing_id).
async fn insert_seller_and_listing(
    pool: &Pool<Postgres>,
    platform: &str,
    platform_id: &str,
) -> (Uuid, Uuid) {
    let seller_id = Uuid::now_v7();
    query(
        "INSERT INTO sellers (id, platform, platform_id, name, verification, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'unknown'::seller_verification, NOW(), NOW())",
    )
    .bind(seller_id)
    .bind(platform)
    .bind(platform_id)
    .bind("Report Score Test Seller")
    .execute(pool)
    .await
    .expect("expected to create the seller");

    let listing_id = Uuid::now_v7();
    query(
        "INSERT INTO listings (id, seller_id, platform, listing_url, listing_id, title, first_seen_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
    )
    .bind(listing_id)
    .bind(seller_id)
    .bind(platform)
    .bind(format!("https://{}.com/item/{}", platform, platform_id))
    .bind(platform_id)
    .bind("Report Score Test Listing")
    .execute(pool)
    .await
    .expect("expected to create the listing");

    (seller_id, listing_id)
}

/// A saved scan of the listing by this user. Returns the scan's id.
async fn insert_scan(
    pool: &Pool<Postgres>,
    listing_id: Uuid,
    user_id: Uuid,
    score: i16,
    signals: &[Signal],
) -> Uuid {
    let id = Uuid::now_v7();
    query(
        "INSERT INTO analysis (id, listing_id, risk_score, risk_level, signals, user_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())",
    )
    .bind(id)
    .bind(listing_id)
    .bind(score)
    .bind(risk_level_for(score))
    .bind(to_value(signals).expect("signals to json"))
    .bind(user_id)
    .execute(pool)
    .await
    .expect("expected to create the scan");
    id
}

/// A saved scan's score, level and "Safely history" value.
async fn read_scan(pool: &Pool<Postgres>, id: Uuid) -> (i16, String, String) {
    let (score, level, signals): (i16, String, Value) =
        query_as("SELECT risk_score, risk_level::text, signals FROM analysis WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("expected to read the scan");
    let history = signals
        .as_array()
        .and_then(|list| list.iter().find(|s| s["label"] == "Safely history"))
        .and_then(|s| s["value"].as_str())
        .unwrap_or("")
        .to_string();
    (score, level, history)
}

/// Reports first (they point at the seller), then scans, listing, seller.
async fn cleanup_report_test(pool: &Pool<Postgres>, platform: &str, platform_id: &str) {
    let _ = query(
        "DELETE FROM fraud_reports WHERE seller_id IN (
            SELECT id FROM sellers WHERE platform = $1 AND platform_id = $2
        )",
    )
    .bind(platform)
    .bind(platform_id)
    .execute(pool)
    .await;
    cleanup_test_seller_chain(pool, platform, platform_id).await;
}

fn report_request(platform: &str, platform_id: &str) -> FraudReportsRequest {
    FraudReportsRequest {
        platform: platform.to_string(),
        platform_id: Some(platform_id.to_string()),
        report_type: ReportTypes::Scam,
        description: None,
        listing_url: Some(format!("https://{}.com/item/{}", platform, platform_id)),
    }
}

/// The score a new scan of a clean supplier would give with this many
/// reports - the same function the scan uses.
fn expected_supplier_score(reports: i64) -> i16 {
    b2b_score_from_signals(&[fraud_report_history(reports)], reports)
}

fn level_text(score: i16) -> String {
    serde_json::to_value(risk_level_for(score))
        .ok()
        .and_then(|v| v.as_str().map(str::to_lowercase))
        .unwrap_or_default()
}

// ---- Report score tests -------------------------------------------

#[test]
fn supplier_report_scores_rise_step_by_step() {
    // 2 reports: at least 35 but not High yet (it used to jump to 67).
    let two = expected_supplier_score(2);
    assert!(
        (35..67).contains(&two),
        "2 reports should give 35-66, got {}",
        two
    );
    // 3 reports and up: the table's minimum.
    for (reports, minimum) in [(3, 67), (4, 77), (5, 87), (6, 95), (9, 95)] {
        assert_eq!(
            expected_supplier_score(reports),
            minimum,
            "{} reports should give {}",
            reports,
            minimum
        );
    }
    // 1 report adds only a little, no jump.
    assert!(expected_supplier_score(1) <= expected_supplier_score(0) + 5);
}

#[tokio::test]
async fn a_report_updates_every_users_saved_scan_of_the_supplier() {
    let pool = test_pool().await;
    let email_a = "report_updates_all_scans_a_test@example.com";
    let email_b = "report_updates_all_scans_b_test@example.com";
    let (user_a, _) = create_test_user(&pool, email_a).await;
    let (user_b, _) = create_test_user(&pool, email_b).await;
    let headers_a = auth_headers_for(&pool, user_a.id).await;
    let headers_b = auth_headers_for(&pool, user_b.id).await;

    let platform = "thomasnet";
    let platform_id = "report_updates_all_scans_001";
    cleanup_report_test(&pool, platform, platform_id).await;
    let (_, listing_id) = insert_seller_and_listing(&pool, platform, platform_id).await;

    // Both people scanned the supplier before anyone reported it.
    let clean = [fraud_report_history(0)];
    let start = expected_supplier_score(0);
    let scan_a = insert_scan(&pool, listing_id, user_a.id, start, &clean).await;
    let scan_b = insert_scan(&pool, listing_id, user_b.id, start, &clean).await;

    // A reports: A gets the new score back, and B's saved scan changes
    // too - B doesn't have to scan again.
    let one = expected_supplier_score(1);
    let result = create_fraud_report(
        State(pool.clone()),
        headers_a,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected A's report to be saved")
    .0;
    assert_eq!(result["risk_score"], json!(one));
    for scan in [scan_a, scan_b] {
        let (score, level, history) = read_scan(&pool, scan).await;
        assert_eq!(score, one);
        assert_eq!(level, level_text(one));
        assert_eq!(history, "Reported once");
    }

    // B reports too: a different person can still report, and A's
    // saved scan gets the 2-report score without scanning again.
    let two = expected_supplier_score(2);
    let result = create_fraud_report(
        State(pool.clone()),
        headers_b,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected B's report to be saved")
    .0;
    assert_eq!(result["risk_score"], json!(two));
    for scan in [scan_a, scan_b] {
        let (score, level, history) = read_scan(&pool, scan).await;
        assert_eq!(score, two);
        assert_eq!(level, level_text(two));
        assert_eq!(history, "Reported 2 times");
    }

    cleanup_report_test(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email_a).await;
    cleanup_test_user(&pool, email_b).await;
}

#[tokio::test]
async fn a_report_keeps_a_higher_supplier_score() {
    let pool = test_pool().await;
    let email = "report_keeps_higher_score_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let platform = "thomasnet";
    let platform_id = "report_keeps_higher_score_001";
    cleanup_report_test(&pool, platform, platform_id).await;
    let (_, listing_id) = insert_seller_and_listing(&pool, platform, platform_id).await;

    // A supplier that asks for Western Union only - already risky.
    let risky = [
        Signal {
            label: "Payment terms".to_string(),
            sub: "Asks for Western Union only.".to_string(),
            value: "Western Union only".to_string(),
            signal_type: "bad".to_string(),
            category: "payment".to_string(),
            check_type: "pattern".to_string(),
        },
        fraud_report_history(0),
    ];
    let before = b2b_score_from_signals(&risky, 0);
    let scan = insert_scan(&pool, listing_id, user.id, before, &risky).await;

    let _ = create_fraud_report(
        State(pool.clone()),
        headers,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected the report to be saved");

    let (score, _, _) = read_scan(&pool, scan).await;
    assert!(
        score >= before,
        "a report must never lower the score: {} -> {}",
        before,
        score
    );

    cleanup_report_test(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn olx_reports_add_20_then_15_to_saved_scans() {
    let pool = test_pool().await;
    let email_a = "olx_report_points_a_test@example.com";
    let email_b = "olx_report_points_b_test@example.com";
    let (user_a, _) = create_test_user(&pool, email_a).await;
    let (user_b, _) = create_test_user(&pool, email_b).await;
    let headers_a = auth_headers_for(&pool, user_a.id).await;
    let headers_b = auth_headers_for(&pool, user_b.id).await;

    let platform = "olx";
    let platform_id = "olx_report_points_001";
    cleanup_report_test(&pool, platform, platform_id).await;
    let (_, listing_id) = insert_seller_and_listing(&pool, platform, platform_id).await;
    let scan = insert_scan(&pool, listing_id, user_a.id, 30, &[]).await;

    // OLX keeps its own rule: 1 report 20, 2 reports 35.
    let result = create_fraud_report(
        State(pool.clone()),
        headers_a,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected the first report to be saved")
    .0;
    assert_eq!(result["risk_score"], json!(50));
    assert_eq!(read_scan(&pool, scan).await.0, 50);

    let _ = create_fraud_report(
        State(pool.clone()),
        headers_b,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected the second report to be saved");
    let (score, level, _) = read_scan(&pool, scan).await;
    assert_eq!(score, 65);
    assert_eq!(level, "caution");

    cleanup_report_test(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email_a).await;
    cleanup_test_user(&pool, email_b).await;
}

#[tokio::test]
async fn already_reported_is_true_only_for_the_reporter() {
    let pool = test_pool().await;
    let email_a = "already_reported_a_test@example.com";
    let email_b = "already_reported_b_test@example.com";
    let (user_a, _) = create_test_user(&pool, email_a).await;
    let (user_b, _) = create_test_user(&pool, email_b).await;
    let headers_a = auth_headers_for(&pool, user_a.id).await;

    let platform = "thomasnet";
    let platform_id = "already_reported_001";
    cleanup_report_test(&pool, platform, platform_id).await;
    let (seller_id, _) = insert_seller_and_listing(&pool, platform, platform_id).await;

    assert!(!already_reported(&pool, user_a.id, seller_id).await.unwrap());

    let _ = create_fraud_report(
        State(pool.clone()),
        headers_a,
        Json(report_request(platform, platform_id)),
    )
    .await
    .expect("expected the report to be saved");

    // The scan sends this as "reported_by_you".
    assert!(already_reported(&pool, user_a.id, seller_id).await.unwrap());
    assert!(!already_reported(&pool, user_b.id, seller_id).await.unwrap());

    cleanup_report_test(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email_a).await;
    cleanup_test_user(&pool, email_b).await;
}

#[tokio::test]
async fn the_database_blocks_a_repeat_report_from_the_same_person() {
    // Migration 0015: even if two reports arrive at the same moment and
    // both pass the server's check, the database keeps only one.
    let pool = test_pool().await;
    let email = "database_blocks_repeat_report_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "thomasnet";
    let platform_id = "database_blocks_repeat_001";
    cleanup_report_test(&pool, platform, platform_id).await;
    let (seller_id, _) = insert_seller_and_listing(&pool, platform, platform_id).await;

    let insert = || {
        query(
            "INSERT INTO fraud_reports (id, seller_id, user_id, platform, platform_id, report_type, listing_url, reported_at)
             VALUES ($1, $2, $3, $4, $5, 'scam'::report_types, $6, NOW())",
        )
        .bind(Uuid::now_v7())
        .bind(seller_id)
        .bind(user.id)
        .bind(platform)
        .bind(platform_id)
        .bind("https://thomasnet.com/item/database-blocks-repeat")
        .execute(&pool)
    };
    insert()
        .await
        .expect("expected the first report to be saved");
    let error = insert()
        .await
        .expect_err("expected the second report to be refused");
    assert!(
        error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation()),
        "expected a unique-rule error, got {:?}",
        error
    );

    cleanup_report_test(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

#[test]
fn saved_signals_can_be_read_back() {
    // A report scores a saved scan again from its stored checks, so the
    // stored JSON must turn back into the same checks.
    let original = vec![fraud_report_history(2)];
    let stored = to_value(&original).expect("to json");
    assert_eq!(stored[0]["type"], "bad");
    let back: Vec<Signal> = serde_json::from_value(stored).expect("from json");
    assert_eq!(back[0].value, "Reported 2 times");
    assert_eq!(back[0].signal_type, "bad");
}
