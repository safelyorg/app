// What a saved scan shows on each plan (services/features/results.rs):
// Free gets the score and the top reasons, paid plans and their team
// members get everything. Uses the real test database.

mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_user, create_test_user,
    insert_test_history_chain, insert_test_subscription, test_pool,
};
use axum::extract::{Path, Query, State};
use backend::handlers::dashboard::{HistoryItemQuery, get_history_item};
use backend::services::features::results::sees_full_results;
use backend::services::features::team::invite;
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::{query, query_scalar};
use uuid::Uuid;

async fn detail(pool: &sqlx::Pool<sqlx::Postgres>, user_id: Uuid, id: Uuid) -> Value {
    let headers = auth_headers_for(pool, user_id).await;
    get_history_item(
        State(pool.clone()),
        headers,
        Path(id),
        Query(HistoryItemQuery { language: None }),
    )
    .await
    .expect("expected the scan")
    .0
}

#[tokio::test]
#[serial]
async fn free_sees_the_top_reasons_and_paid_sees_everything() {
    let pool = test_pool().await;
    let email = "results-owner@results-test.safely.sh";
    let member_email = "results-member@results-test.safely.sh";
    cleanup_test_user(&pool, email).await;
    cleanup_test_user(&pool, member_email).await;
    let (user, _) = create_test_user(&pool, email).await;
    cleanup_test_seller_chain(&pool, "alibaba", "results_test_001").await;
    let (listing_id, _) = insert_test_history_chain(
        &pool,
        user.id,
        "alibaba",
        "results_test_001",
        "Results test",
    )
    .await;
    let id: Uuid = query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    query(
        "UPDATE analysis SET signals = $2, risk_factors = '[]'::jsonb, social_candidates = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(json!([
        { "label": "Price analysis", "sub": "Far too cheap.", "value": "suspiciously low", "type": "bad", "category": "listing", "check_type": "anomaly" },
        { "label": "Account age", "sub": "Founded in 2003.", "value": "About 23 years", "type": "good", "category": "seller", "check_type": "existence" },
        { "label": "Contact info", "sub": "Phone given.", "value": "Confirmed", "type": "good", "category": "seller", "check_type": "existence" },
        { "label": "Sanctions lists", "sub": "Not on the lists.", "value": "Not on the lists", "type": "good", "category": "company", "check_type": "existence" }
    ]))
    .bind(json!([{ "platform": "Facebook", "variant_searched": "x", "found": true, "candidates": [] }]))
    .execute(&pool)
    .await
    .unwrap();

    // Free
    assert!(!sees_full_results(&pool, user.id).await);
    let free = detail(&pool, user.id, id).await;
    assert_eq!(free["full_result"], false);
    let signals = free["signals"].as_array().unwrap();
    assert_eq!(signals.len(), 4, "every check is still listed");
    assert_eq!(signals[0]["sub"], "Far too cheap.");
    assert_eq!(signals[2]["value"], "Confirmed");
    assert_eq!(signals[3]["sub"], "", "the 4th check is hidden");
    assert_eq!(signals[3]["value"], "");
    assert_eq!(signals[3]["label"], "Sanctions lists");
    assert_eq!(free["social_candidates"], json!([]));
    assert_eq!(free["risk_score"].is_number(), true);

    // Paid: everything.
    insert_test_subscription(&pool, user.id, "sub_results_test", "Team", "active").await;
    let paid = detail(&pool, user.id, id).await;
    assert_eq!(paid["full_result"], true);
    assert_eq!(paid["signals"][3]["sub"], "Not on the lists.");
    assert_eq!(paid["social_candidates"].as_array().unwrap().len(), 1);

    // A member of the paid team sees full results too.
    invite(&pool, user.id, member_email).await.unwrap();
    let (member, _) = create_test_user(&pool, member_email).await;
    assert!(sees_full_results(&pool, member.id).await);

    let _ = query("DELETE FROM team_members WHERE email = $1")
        .bind(member_email)
        .execute(&pool)
        .await;
    cleanup_test_seller_chain(&pool, "alibaba", "results_test_001").await;
    cleanup_test_user(&pool, member_email).await;
    cleanup_test_user(&pool, email).await;
}
