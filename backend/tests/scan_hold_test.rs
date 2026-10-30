mod common;

use crate::common::{
    TestPaidPlan, cleanup_test_user, create_test_user, insert_subscription_with_scans, test_pool,
};
use backend::services::{
    features::analysis::ScanHold,
    features::billing::{ScanCharge, get_scan_usage, give_back_scan, reserve_scan},
};
use serde_json::json;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

// A scan is counted when it starts and given back when it does not
// finish (an error, or the buyer closed or refreshed the tab). These
// tests check the counting and the giving back.

async fn used(pool: &Pool<Postgres>, user_id: Uuid) -> serde_json::Value {
    get_scan_usage(pool, user_id).await["used"].clone()
}

/// Waits (up to 3 seconds) for a scan given back in the background.
async fn wait_for_used(pool: &Pool<Postgres>, user_id: Uuid, expected: i64) {
    for _ in 0..30 {
        if used(pool, user_id).await == json!(expected) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!(
        "expected {} scans used, got {}",
        expected,
        used(pool, user_id).await
    );
}

#[tokio::test]
async fn a_free_scan_that_does_not_finish_is_given_back() {
    let pool = test_pool().await;
    let email = "scan_hold_free_give_back_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    let charge = reserve_scan(&pool, user.id)
        .await
        .expect("expected a free scan");
    assert_eq!(charge, ScanCharge::Free { user_id: user.id });
    assert_eq!(used(&pool, user.id).await, json!(1));

    give_back_scan(&pool, charge).await;
    assert_eq!(used(&pool, user.id).await, json!(0));

    // Giving back again never goes below 0.
    give_back_scan(&pool, charge).await;
    assert_eq!(used(&pool, user.id).await, json!(0));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn a_paid_scan_that_does_not_finish_is_given_back() {
    let pool = test_pool().await;
    let email = "scan_hold_paid_give_back_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;
    insert_subscription_with_scans(
        &pool,
        user.id,
        "sub_scan_hold_give_back_001",
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 10,
            bought_days_ago: 5,
        },
    )
    .await;

    let charge = reserve_scan(&pool, user.id)
        .await
        .expect("expected a paid scan");
    assert!(matches!(charge, ScanCharge::Paid { .. }));
    assert_eq!(used(&pool, user.id).await, json!(11));

    give_back_scan(&pool, charge).await;
    assert_eq!(used(&pool, user.id).await, json!(10));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn a_held_scan_is_given_back_when_dropped_and_kept_when_finished() {
    let pool = test_pool().await;
    let email = "scan_hold_drop_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    // Dropped (the scan failed or the tab was closed): given back.
    let charge = reserve_scan(&pool, user.id)
        .await
        .expect("expected a free scan");
    let hold = ScanHold::new(pool.clone(), charge);
    assert_eq!(used(&pool, user.id).await, json!(1));
    drop(hold);
    wait_for_used(&pool, user.id, 0).await;

    // Finished (the result was saved): it stays counted.
    let charge = reserve_scan(&pool, user.id)
        .await
        .expect("expected a free scan");
    ScanHold::new(pool.clone(), charge).finish();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(used(&pool, user.id).await, json!(1));

    cleanup_test_user(&pool, email).await;
}

#[tokio::test]
async fn a_user_on_the_last_free_scan_gets_it_back_after_a_failed_scan() {
    let pool = test_pool().await;
    let email = "scan_hold_last_scan_test@example.com";
    let (user, _) = create_test_user(&pool, email).await;

    // Use 4 of the 5 free scans.
    for _ in 0..4 {
        reserve_scan(&pool, user.id)
            .await
            .expect("expected a free scan");
    }
    // The 5th scan fails: it is given back...
    let charge = reserve_scan(&pool, user.id)
        .await
        .expect("expected the 5th scan");
    assert!(
        reserve_scan(&pool, user.id).await.is_err(),
        "the limit is reached"
    );
    give_back_scan(&pool, charge).await;
    // ...so the user can still run their 5th scan.
    assert!(reserve_scan(&pool, user.id).await.is_ok());

    cleanup_test_user(&pool, email).await;
}
