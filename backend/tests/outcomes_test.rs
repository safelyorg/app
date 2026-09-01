mod common;

use axum::{Json, extract::State, http::HeaderMap};
use backend::handlers::outcomes::create_outcome;
use backend::models::outcomes::{OutcomeAction, OutcomeRequest};
use backend::services::outcomes::record_outcome;
use common::{
    admin_pool, auth_headers_for, cleanup_test_seller_chain, cleanup_test_user, create_test_user,
    insert_test_analysis_for_outcomes,
};
use sqlx::query_as;
use uuid::Uuid;

/// It confirms a genuinely unauthenticated request is correctly
/// rejected, before anything is ever written to the database.
#[tokio::test]
async fn create_outcome_unauthorized_request() {
    let pool = admin_pool().await;
    let headers = HeaderMap::new();
    let request = OutcomeRequest {
        analysis_id: Uuid::new_v4(),
        action: OutcomeAction::Proceeded,
    };
    let result = create_outcome(State(pool), headers, Json(request)).await;
    assert!(
        result.is_err(),
        "expected an unauthenticated request to be rejected"
    );
}

/// It confirms a genuine, real "proceeded" outcome is correctly
/// recorded, tied to the right analysis and the right user.
#[tokio::test]
async fn create_outcome_proceeded_success() {
    let pool = admin_pool().await;
    let email = "outcome_proceeded_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let platform = "olx";
    let platform_id = "outcome_proceeded_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, user.id, platform, platform_id).await;

    let request = OutcomeRequest {
        analysis_id,
        action: OutcomeAction::Proceeded,
    };
    let result = create_outcome(State(pool.clone()), headers, Json(request)).await;
    assert!(
        result.is_ok(),
        "expected a genuine, valid outcome to be recorded successfully"
    );

    let recorded: Option<(String,)> =
        query_as("SELECT action::text FROM outcomes WHERE analysis_id = $1 AND user_id = $2")
            .bind(analysis_id)
            .bind(user.id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(
        recorded,
        Some(("proceeded".to_string(),)),
        "expected the real outcome row to show 'proceeded'"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

/// It confirms a genuine, real "aborted" outcome is correctly
/// recorded too - the other real action value, not just "proceeded."
#[tokio::test]
async fn create_outcome_aborted_success() {
    let pool = admin_pool().await;
    let email = "outcome_aborted_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let platform = "olx";
    let platform_id = "outcome_aborted_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, user.id, platform, platform_id).await;

    let request = OutcomeRequest {
        analysis_id,
        action: OutcomeAction::Aborted,
    };
    let result = create_outcome(State(pool.clone()), headers, Json(request)).await;
    assert!(
        result.is_ok(),
        "expected a genuine, valid outcome to be recorded successfully"
    );

    let recorded: Option<(String,)> =
        query_as("SELECT action::text FROM outcomes WHERE analysis_id = $1 AND user_id = $2")
            .bind(analysis_id)
            .bind(user.id)
            .fetch_optional(&pool)
            .await
            .expect("expected the query itself to succeed");

    assert_eq!(
        recorded,
        Some(("aborted".to_string(),)),
        "expected the real outcome row to show 'aborted'"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

/// It confirms a genuinely nonexistent analysis_id correctly fails,
/// rather than silently succeeding - the foreign key to `analysis`
/// should be respected, not bypassed.
#[tokio::test]
async fn create_outcome_nonexistent_analysis_fails() {
    let pool = admin_pool().await;
    let email = "outcome_bad_analysis_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;
    let headers = auth_headers_for(&pool, user.id).await;

    let request = OutcomeRequest {
        analysis_id: Uuid::new_v4(),
        action: OutcomeAction::Proceeded,
    };
    let result = create_outcome(State(pool.clone()), headers, Json(request)).await;
    assert!(
        result.is_err(),
        "expected recording an outcome against a genuinely nonexistent analysis to fail"
    );

    cleanup_test_user(&pool, email).await;
}

// Record Outcome Tests

/// It confirms a genuine, real "proceeded" outcome is correctly
/// inserted, and that the returned struct's fields genuinely match
/// what was actually written to the database.
#[tokio::test]
async fn record_outcome_proceeded_success() {
    let pool = admin_pool().await;
    let email = "record_outcome_proceeded_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "record_outcome_proceeded_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, user.id, platform, platform_id).await;

    let outcome = record_outcome(&pool, analysis_id, user.id, OutcomeAction::Proceeded)
        .await
        .expect("expected the outcome to be recorded successfully");

    assert_eq!(outcome.analysis_id, analysis_id);
    assert_eq!(outcome.user_id, user.id);
    assert!(matches!(outcome.action, OutcomeAction::Proceeded));

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

/// It confirms a genuine, real "aborted" outcome round-trips
/// correctly too - the other real action value, not just "proceeded."
#[tokio::test]
async fn record_outcome_aborted_success() {
    let pool = admin_pool().await;
    let email = "record_outcome_aborted_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "record_outcome_aborted_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, user.id, platform, platform_id).await;

    let outcome = record_outcome(&pool, analysis_id, user.id, OutcomeAction::Aborted)
        .await
        .expect("expected the outcome to be recorded successfully");

    assert_eq!(outcome.analysis_id, analysis_id);
    assert_eq!(outcome.user_id, user.id);
    assert!(matches!(outcome.action, OutcomeAction::Aborted));

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

/// It confirms the real found_at timestamp is genuinely, freshly set
/// at the moment of insertion - not left null, not backdated.
#[tokio::test]
async fn record_outcome_sets_a_real_recent_timestamp() {
    let pool = admin_pool().await;
    let email = "record_outcome_timestamp_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "record_outcome_timestamp_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, user.id, platform, platform_id).await;

    let before = chrono::Utc::now();
    let outcome = record_outcome(&pool, analysis_id, user.id, OutcomeAction::Proceeded)
        .await
        .expect("expected the outcome to be recorded successfully");
    let after = chrono::Utc::now();

    assert!(
        outcome.recorded_at >= before && outcome.recorded_at <= after,
        "expected recorded_at to genuinely fall within the real window this test ran in"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}

/// It confirms a genuinely nonexistent analysis_id correctly fails at
/// the database level - the real foreign key to `analysis` should
/// stop this, not silently allow an orphaned outcome row.
#[tokio::test]
async fn record_outcome_nonexistent_analysis_fails() {
    let pool = admin_pool().await;
    let email = "record_outcome_bad_analysis_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (user, _) = create_test_user(&pool, email).await;

    let result = record_outcome(&pool, Uuid::new_v4(), user.id, OutcomeAction::Proceeded).await;

    assert!(
        result.is_err(),
        "expected a genuinely nonexistent analysis_id to be rejected by the real foreign key"
    );

    cleanup_test_user(&pool, email).await;
}

/// It confirms a genuinely nonexistent user_id correctly fails too -
/// testing the OTHER real foreign key independently, not just
/// analysis_id.
#[tokio::test]
async fn record_outcome_nonexistent_user_fails() {
    let pool = admin_pool().await;
    let email = "record_outcome_bad_user_owner_test@example.com";
    cleanup_test_user(&pool, email).await;
    let (owner, _) = create_test_user(&pool, email).await;

    let platform = "olx";
    let platform_id = "record_outcome_bad_user_seller_001";
    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    let analysis_id =
        insert_test_analysis_for_outcomes(&pool, owner.id, platform, platform_id).await;

    let result = record_outcome(&pool, analysis_id, Uuid::new_v4(), OutcomeAction::Proceeded).await;

    assert!(
        result.is_err(),
        "expected a genuinely nonexistent user_id to be rejected by the real foreign key"
    );

    cleanup_test_seller_chain(&pool, platform, platform_id).await;
    cleanup_test_user(&pool, email).await;
}
