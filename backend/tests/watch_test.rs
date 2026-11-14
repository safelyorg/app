// "Watch this supplier" (services/features/watch.rs): paid plans only,
// one list per team, the Team limit, and a background check that notes
// how things are first and then reports what got worse. Uses the real
// test database; the listing address is made up (.invalid), so no real
// site is visited.

mod common;

use crate::common::{
    cleanup_test_seller_chain, cleanup_test_user, create_test_user, insert_test_history_chain,
    insert_test_subscription, test_pool,
};
use backend::services::features::team::invite;
use backend::services::features::watch::{
    TEAM_WATCH_LIMIT, WatchError, add_watch, is_watched, run_due_checks, stop_watch, watch_list,
};
use serial_test::serial;
use sqlx::{Pool, Postgres, query, query_scalar};
use uuid::Uuid;

const OWNER: &str = "w-owner@watch-test.safely.sh";
const MEMBER: &str = "w-member@watch-test.safely.sh";
const FREE: &str = "w-free@watch-test.safely.sh";
const SUB: &str = "sub_watch_test";

async fn cleanup(pool: &Pool<Postgres>) {
    let _ = query("DELETE FROM watched_suppliers WHERE team_owner_id IN (SELECT id FROM users WHERE email LIKE '%@watch-test.safely.sh')")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM team_members WHERE email LIKE '%@watch-test.safely.sh'")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM subscriptions WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(pool)
        .await;
    let _ = query("DELETE FROM sellers WHERE platform_id LIKE 'wt_fake_%'")
        .execute(pool)
        .await;
    for id in ["wt_supplier", "wt_free"] {
        cleanup_test_seller_chain(pool, "alibaba", id).await;
    }
    for email in [OWNER, MEMBER, FREE] {
        cleanup_test_user(pool, email).await;
    }
}

/// A saved scan whose listing address is made up.
async fn scan(pool: &Pool<Postgres>, user_id: Uuid, platform_id: &str) -> (Uuid, Uuid) {
    let (listing_id, _) =
        insert_test_history_chain(pool, user_id, "alibaba", platform_id, "Watch test listing")
            .await;
    query("UPDATE listings SET listing_url = $2 WHERE id = $1")
        .bind(listing_id)
        .bind(format!("https://{platform_id}.invalid/item"))
        .execute(pool)
        .await
        .unwrap();
    let (analysis_id, seller_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT a.id, l.seller_id FROM analysis a JOIN listings l ON l.id = a.listing_id WHERE l.id = $1",
    )
    .bind(listing_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (analysis_id, seller_id)
}

async fn status_of(pool: &Pool<Postgres>, seller_id: Uuid) -> String {
    query_scalar("SELECT status FROM watched_suppliers WHERE seller_id = $1")
        .bind(seller_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[serial]
async fn watching_suppliers_end_to_end() {
    let pool = test_pool().await;
    cleanup(&pool).await;

    let (owner, _) = create_test_user(&pool, OWNER).await;
    let (member, _) = create_test_user(&pool, MEMBER).await;
    let (free, _) = create_test_user(&pool, FREE).await;

    // Free: not available.
    let (free_scan, _) = scan(&pool, free.id, "wt_free").await;
    assert!(matches!(
        add_watch(&pool, free.id, free_scan).await,
        Err(WatchError::NeedsPaidPlan)
    ));

    insert_test_subscription(&pool, owner.id, SUB, "Team", "active").await;
    invite(&pool, owner.id, MEMBER).await.unwrap();
    let (analysis_id, seller_id) = scan(&pool, owner.id, "wt_supplier").await;

    // A scan someone else made, outside the team: not found.
    assert!(matches!(
        add_watch(&pool, owner.id, free_scan).await,
        Err(WatchError::NotFound)
    ));

    add_watch(&pool, owner.id, analysis_id).await.unwrap();
    // Watching again changes nothing.
    add_watch(&pool, owner.id, analysis_id).await.unwrap();
    assert!(is_watched(&pool, owner.id, analysis_id).await);
    // The team shares the list.
    assert!(is_watched(&pool, member.id, analysis_id).await);
    let list = watch_list(&pool, member.id).await.unwrap();
    assert_eq!(list["used"], 1);
    assert_eq!(list["limit"], TEAM_WATCH_LIMIT);
    assert_eq!(list["every_days"], 7);
    assert_eq!(list["items"][0]["status"], "pending");
    assert_eq!(list["items"][0]["added_by"], OWNER);
    assert_eq!(list["items"][0]["added_by_you"], false);

    // Open goes to the newest scan of the supplier (the one History shows).
    let newer: Uuid = sqlx::query_scalar(
        "INSERT INTO analysis (listing_id, user_id, risk_score, risk_level, signals, team_owner_id, created_at)
         SELECT listing_id, user_id, risk_score, risk_level, signals, $2, NOW() + INTERVAL '1 minute'
         FROM analysis WHERE id = $1 RETURNING id",
    )
    .bind(analysis_id)
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let list = watch_list(&pool, owner.id).await.unwrap();
    assert_eq!(list["items"][0]["analysis_id"], newer.to_string());

    // First check: only notes how things are.
    assert!(run_due_checks(&pool, 10).await >= 1);
    let first = status_of(&pool, seller_id).await;
    assert!(
        first == "no_change" || first == "listing_not_checked",
        "first check: {first}"
    );
    let next: bool = query_scalar(
        "SELECT next_check_at > NOW() + INTERVAL '6 days' FROM watched_suppliers WHERE seller_id = $1",
    )
    .bind(seller_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(next, "Team: next check in a week");

    // A new scam report, then the next check: "changed".
    query("INSERT INTO fraud_reports (seller_id, user_id, platform, platform_id, report_type) VALUES ($1, $2, 'alibaba', 'wt_supplier', 'scam')")
        .bind(seller_id)
        .bind(free.id)
        .execute(&pool)
        .await
        .unwrap();
    query("UPDATE watched_suppliers SET next_check_at = NOW() WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .unwrap();
    run_due_checks(&pool, 10).await;
    assert_eq!(status_of(&pool, seller_id).await, "changed");
    let list = watch_list(&pool, owner.id).await.unwrap();
    assert_eq!(list["items"][0]["changes"][0]["kind"], "scam_report");

    // The Team limit.
    query("INSERT INTO sellers (platform, platform_id) SELECT 'alibaba', 'wt_fake_' || g FROM generate_series(1, $1) g")
        .bind((TEAM_WATCH_LIMIT - 1) as i32)
        .execute(&pool)
        .await
        .unwrap();
    query("INSERT INTO watched_suppliers (team_owner_id, seller_id) SELECT $1, id FROM sellers WHERE platform_id LIKE 'wt_fake_%'")
        .bind(owner.id)
        .execute(&pool)
        .await
        .unwrap();
    stop_watch(&pool, member.id, seller_id).await.unwrap();
    assert!(!is_watched(&pool, owner.id, analysis_id).await);
    add_watch(&pool, member.id, analysis_id).await.unwrap(); // 25 of 25
    query("DELETE FROM watched_suppliers WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .unwrap();
    query("INSERT INTO sellers (platform, platform_id) VALUES ('alibaba', 'wt_fake_extra')")
        .execute(&pool)
        .await
        .unwrap();
    query("INSERT INTO watched_suppliers (team_owner_id, seller_id) SELECT $1, id FROM sellers WHERE platform_id = 'wt_fake_extra'")
        .bind(owner.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        add_watch(&pool, owner.id, analysis_id).await,
        Err(WatchError::LimitReached(25))
    ));

    query("DELETE FROM fraud_reports WHERE platform_id = 'wt_supplier'")
        .execute(&pool)
        .await
        .unwrap();
    cleanup(&pool).await;
}
