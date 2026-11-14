// Team scans in the dashboard's History (services/features/team_history.rs):
// the owner sees all team scans and each member, a member sees one
// teammate at a time, scans from before joining stay private, and team
// scans stay with the team after someone leaves. Uses the real test
// database.

mod common;

use crate::common::{
    auth_headers_for, cleanup_test_seller_chain, cleanup_test_user, create_test_user,
    insert_test_history_chain, insert_test_subscription, test_pool,
};
use axum::extract::{Path, Query, State};
use backend::errors::dashboard::DashboardError;
use backend::handlers::dashboard::{
    HistoryHtmlQuery, HistoryItemQuery, get_history_html, get_history_item, get_history_people,
};
use backend::models::analysis::RiskLevel;
use backend::services::features::analysis::{CreateAnalysisData, create_analysis};
use backend::services::features::team::invite;
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::{Pool, Postgres, query, query_scalar};
use uuid::Uuid;

const OWNER: &str = "th-owner@team-history-test.safely.sh";
const ALI: &str = "th-ali@team-history-test.safely.sh";
const SARA: &str = "th-sara@team-history-test.safely.sh";
const OUTSIDER: &str = "th-outsider@team-history-test.safely.sh";
const SCANS: [&str; 4] = [
    "th_owner_old",
    "th_ali_before",
    "th_ali_team",
    "th_sara_team",
];

async fn cleanup(pool: &Pool<Postgres>) {
    let _ = query("DELETE FROM team_members WHERE email LIKE '%@team-history-test.safely.sh'")
        .execute(pool)
        .await;
    for id in SCANS {
        cleanup_test_seller_chain(pool, "alibaba", id).await;
    }
    for email in [OWNER, ALI, SARA, OUTSIDER] {
        cleanup_test_user(pool, email).await;
    }
}

async fn rows(pool: &Pool<Postgres>, user_id: Uuid, who: Option<String>) -> String {
    let headers = auth_headers_for(pool, user_id).await;
    get_history_html(
        State(pool.clone()),
        headers,
        Query(HistoryHtmlQuery { who }),
    )
    .await
    .expect("expected the history rows")
    .0
}

async fn people(pool: &Pool<Postgres>, user_id: Uuid) -> Value {
    let headers = auth_headers_for(pool, user_id).await;
    get_history_people(State(pool.clone()), headers)
        .await
        .expect("expected the people list")
        .0
}

async fn open(pool: &Pool<Postgres>, user_id: Uuid, id: Uuid) -> Result<Value, DashboardError> {
    let headers = auth_headers_for(pool, user_id).await;
    get_history_item(
        State(pool.clone()),
        headers,
        Path(id),
        Query(HistoryItemQuery { language: None }),
    )
    .await
    .map(|j| j.0)
}

async fn first_scan(pool: &Pool<Postgres>, listing_id: Uuid) -> Uuid {
    query_scalar("SELECT id FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A real saved scan, as the analyze handler saves it.
async fn scan_in_team(pool: &Pool<Postgres>, user_id: Uuid, platform_id: &str, title: &str) {
    let (listing_id, _) =
        insert_test_history_chain(pool, user_id, "alibaba", platform_id, title).await;
    // The helper's own row is an "older" scan; keep only the real one.
    query("DELETE FROM analysis WHERE listing_id = $1")
        .bind(listing_id)
        .execute(pool)
        .await
        .unwrap();
    create_analysis(CreateAnalysisData {
        pool,
        listing_id,
        risk_score: 40,
        risk_level: RiskLevel::Caution,
        signals: json!([]),
        network_summary: "test".to_string(),
        claude_raw: String::new(),
        user_id,
        confidence_level: "high".to_string(),
        confidence_reasoning: "test".to_string(),
        risk_factors: json!([]),
        social_candidates: json!([]),
    })
    .await
    .expect("expected the scan to be saved");
}

#[tokio::test]
#[serial]
async fn owner_and_members_see_team_scans() {
    let pool = test_pool().await;
    cleanup(&pool).await;

    let (owner, _) = create_test_user(&pool, OWNER).await;
    let (ali, _) = create_test_user(&pool, ALI).await;
    let (sara, _) = create_test_user(&pool, SARA).await;
    let (outsider, _) = create_test_user(&pool, OUTSIDER).await;
    insert_test_subscription(&pool, owner.id, "sub_team_history_test", "Team", "active").await;

    // Before the team: the owner's older scan, and Ali's own scan.
    let (owner_listing, _) =
        insert_test_history_chain(&pool, owner.id, "alibaba", SCANS[0], "Owner old scan").await;
    let owner_scan = first_scan(&pool, owner_listing).await;
    insert_test_history_chain(&pool, ali.id, "alibaba", SCANS[1], "Ali before joining").await;

    // Not in a team yet: no dropdown.
    assert_eq!(people(&pool, owner.id).await["team"], false);

    invite(&pool, owner.id, ALI).await.unwrap();
    invite(&pool, owner.id, SARA).await.unwrap();
    scan_in_team(&pool, ali.id, SCANS[2], "Ali team scan").await;
    scan_in_team(&pool, sara.id, SCANS[3], "Sara team scan").await;

    // The dropdown: the owner gets "All team scans" and both members.
    let p = people(&pool, owner.id).await;
    assert_eq!(p["team"], true);
    assert_eq!(p["can_see_all"], true);
    let emails: Vec<&str> = p["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["email"].as_str().unwrap())
        .collect();
    assert_eq!(emails, vec![ALI, SARA]);

    // A member: no "All team scans", the owner first, then the others.
    let p = people(&pool, ali.id).await;
    assert_eq!(p["can_see_all"], false);
    assert_eq!(p["people"][0]["email"], OWNER);
    assert_eq!(p["people"][0]["is_owner"], true);
    assert_eq!(p["people"][1]["email"], SARA);
    assert_eq!(people(&pool, outsider.id).await["team"], false);

    // Owner, all team scans: everyone's team scans, not Ali's private one.
    let all = rows(&pool, owner.id, Some("team".into())).await;
    assert!(all.contains("Owner old scan"));
    assert!(all.contains("Ali team scan"));
    assert!(all.contains("Sara team scan"));
    assert!(!all.contains("Ali before joining"));
    assert!(all.contains(ALI), "the Scanned by column shows the email");

    // Owner, one member.
    let one = rows(&pool, owner.id, Some(sara.id.to_string())).await;
    assert!(one.contains("Sara team scan"));
    assert!(!one.contains("Ali team scan"));
    // One person's list has no "Scanned by" column (same columns as My scans).
    assert!(!one.contains(&format!("title=\"{SARA}\"")));
    assert!(all.contains(&format!("title=\"{ALI}\"")));

    // A member asking for all team scans gets their own list instead.
    let mine = rows(&pool, ali.id, Some("team".into())).await;
    assert!(mine.contains("Ali before joining"));
    assert!(!mine.contains("Sara team scan"));
    assert!(!mine.contains(ALI), "no Scanned by column in My scans");

    // A member looking at the owner, and at a teammate.
    let o = rows(&pool, ali.id, Some(owner.id.to_string())).await;
    assert!(o.contains("Owner old scan"));
    let s = rows(&pool, ali.id, Some(sara.id.to_string())).await;
    assert!(s.contains("Sara team scan"));

    // Opening a teammate's scan shows who made it; outsiders get "not found".
    let detail = open(&pool, ali.id, owner_scan)
        .await
        .expect("a teammate's scan");
    assert_eq!(detail["scanned_by"], OWNER);
    assert!(
        open(&pool, owner.id, owner_scan)
            .await
            .unwrap()
            .get("scanned_by")
            .is_none()
    );
    assert!(matches!(
        open(&pool, outsider.id, owner_scan).await,
        Err(DashboardError::NotFound(_))
    ));

    // The Report tab: everyone in the team sees the team's reports, with
    // who made them; a report from outside the team is not listed.
    let seller_id: Uuid = query_scalar(
        "SELECT l.seller_id FROM analysis a JOIN listings l ON l.id = a.listing_id WHERE a.id = $1",
    )
    .bind(owner_scan)
    .fetch_one(&pool)
    .await
    .unwrap();
    for who in [sara.id, outsider.id] {
        query("INSERT INTO fraud_reports (seller_id, user_id, platform, platform_id, report_type) VALUES ($1, $2, 'alibaba', $3, 'scam')")
            .bind(seller_id)
            .bind(who)
            .bind(SCANS[0])
            .execute(&pool)
            .await
            .unwrap();
    }
    let detail = open(&pool, ali.id, owner_scan).await.unwrap();
    let reports = detail["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 1, "only the team's report: {reports:?}");
    assert_eq!(reports[0]["by_email"], SARA);
    assert_eq!(reports[0]["by_you"], false);
    let detail = open(&pool, sara.id, owner_scan).await.unwrap();
    assert_eq!(detail["reports"][0]["by_you"], true);
    query("DELETE FROM fraud_reports WHERE seller_id = $1")
        .bind(seller_id)
        .execute(&pool)
        .await
        .unwrap();

    // Ali leaves: his team scan stays with the team, the owner still sees it.
    query("DELETE FROM team_members WHERE email = $1")
        .bind(ALI)
        .execute(&pool)
        .await
        .unwrap();
    let after = rows(&pool, owner.id, Some("team".into())).await;
    assert!(after.contains("Ali team scan"));
    // And Ali no longer sees the team.
    assert_eq!(people(&pool, ali.id).await["team"], false);
    assert!(matches!(
        open(&pool, ali.id, owner_scan).await,
        Err(DashboardError::NotFound(_))
    ));

    query("DELETE FROM subscriptions WHERE creem_subscription_id = 'sub_team_history_test'")
        .execute(&pool)
        .await
        .unwrap();
    cleanup(&pool).await;
}
